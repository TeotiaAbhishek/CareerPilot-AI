//! Reads your master resume (Markdown) into structured parts, and numbers
//! every fact in it so the model can cite them.
//!
//! Evidence ids:
//! - `S`    the summary / objective paragraph
//! - `K1`.. each skills line ("Programming: Python, SQL")
//! - `W1`.. each job under Experience, `W1.1`.. its bullets
//! - `P1`.. each project, `P1.1`.. its bullets
//! - `ED1`.. each education entry, `C1`.. each certification
//!
//! Parsing is deterministic (no model), so names, employers, titles and dates
//! always come straight from your resume.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MasterResume {
    pub name: String,
    /// Lines under the name (location, phone, email, links), kept as written.
    pub contact: Vec<String>,
    pub summary: Option<String>,
    pub skills: Vec<SkillGroup>,
    pub experience: Vec<Entry>,
    pub projects: Vec<Entry>,
    pub education: Vec<Entry>,
    /// Any other section (Certifications, Awards...), kept as written.
    pub other: Vec<OtherSection>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillGroup {
    /// "" when the line has no "Group:" label.
    pub group: String,
    pub items: Vec<String>,
}

/// A job, project or degree: a heading, detail lines and bullets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub heading: String,
    /// e.g. "**Associate Data Scientist**", "Mumbai, India", "Sep 2022 – Sep 2023".
    pub details: Vec<String>,
    pub bullets: Vec<Bullet>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bullet {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OtherSection {
    pub heading: String,
    /// (evidence id, text). Bullets get ids like C1; plain lines get "".
    pub lines: Vec<(String, String)>,
}

/// One citable fact.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Summary,
    Skills,
    Experience,
    Projects,
    Education,
    Other,
}

fn section_kind(heading: &str) -> Kind {
    let h = heading.to_lowercase();
    if ["objective", "summary", "profile", "about"]
        .iter()
        .any(|k| h.contains(k))
    {
        Kind::Summary
    } else if h.contains("skill") {
        Kind::Skills
    } else if ["experience", "employment", "work history"]
        .iter()
        .any(|k| h.contains(k))
    {
        Kind::Experience
    } else if h.contains("project") {
        Kind::Projects
    } else if h.contains("education") {
        Kind::Education
    } else {
        Kind::Other
    }
}

fn bullet_text(line: &str) -> Option<&str> {
    line.strip_prefix("* ")
        .or_else(|| line.strip_prefix("- "))
        .or_else(|| line.strip_prefix("• "))
        .map(str::trim)
}

impl MasterResume {
    pub fn parse(markdown: &str) -> Self {
        let mut resume = MasterResume::default();
        let mut kind: Option<Kind> = None;
        let mut summary_lines: Vec<String> = Vec::new();
        let mut cert_count = 0;

        for raw in markdown.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(heading) = line.strip_prefix("## ") {
                kind = Some(section_kind(heading));
                if kind == Some(Kind::Other) {
                    resume.other.push(OtherSection {
                        heading: heading.trim().to_string(),
                        lines: Vec::new(),
                    });
                }
                continue;
            }
            if let Some(name) = line.strip_prefix("# ") {
                if resume.name.is_empty() {
                    resume.name = name.trim().to_string();
                }
                continue;
            }

            match kind {
                None => resume.contact.push(line.to_string()),
                Some(Kind::Summary) => summary_lines.push(line.to_string()),
                Some(Kind::Skills) => {
                    let text = bullet_text(line).unwrap_or(line);
                    resume.skills.push(parse_skill_line(text));
                }
                Some(k @ (Kind::Experience | Kind::Projects | Kind::Education)) => {
                    let (list, prefix) = match k {
                        Kind::Experience => (&mut resume.experience, "W"),
                        Kind::Projects => (&mut resume.projects, "P"),
                        _ => (&mut resume.education, "ED"),
                    };
                    if let Some(heading) = line.strip_prefix("### ") {
                        list.push(Entry {
                            id: format!("{prefix}{}", list.len() + 1),
                            heading: heading.trim().to_string(),
                            ..Entry::default()
                        });
                        continue;
                    }
                    // Text before the first "###" starts an untitled entry.
                    if list.is_empty() {
                        list.push(Entry {
                            id: format!("{prefix}1"),
                            ..Entry::default()
                        });
                    }
                    let entry = list.last_mut().expect("just ensured");
                    match bullet_text(line) {
                        Some(text) => entry.bullets.push(Bullet {
                            id: format!("{}.{}", entry.id, entry.bullets.len() + 1),
                            text: text.to_string(),
                        }),
                        None => entry.details.push(line.to_string()),
                    }
                }
                Some(Kind::Other) => {
                    let section = resume.other.last_mut().expect("pushed with the heading");
                    match bullet_text(line) {
                        Some(text) => {
                            cert_count += 1;
                            section
                                .lines
                                .push((format!("C{cert_count}"), text.to_string()));
                        }
                        None => section.lines.push((String::new(), line.to_string())),
                    }
                }
            }
        }

        if !summary_lines.is_empty() {
            resume.summary = Some(summary_lines.join(" "));
        }
        resume
    }

    /// Every citable fact, in resume order.
    pub fn evidence(&self) -> Vec<Evidence> {
        let mut out = Vec::new();
        if let Some(summary) = &self.summary {
            out.push(Evidence {
                id: "S".into(),
                text: summary.clone(),
            });
        }
        for (i, group) in self.skills.iter().enumerate() {
            out.push(Evidence {
                id: format!("K{}", i + 1),
                text: skill_line(group),
            });
        }
        for entry in self
            .experience
            .iter()
            .chain(&self.projects)
            .chain(&self.education)
        {
            out.push(Evidence {
                id: entry.id.clone(),
                text: entry_line(entry),
            });
            for bullet in &entry.bullets {
                out.push(Evidence {
                    id: bullet.id.clone(),
                    text: bullet.text.clone(),
                });
            }
        }
        for section in &self.other {
            for (id, text) in &section.lines {
                if !id.is_empty() {
                    out.push(Evidence {
                        id: id.clone(),
                        text: text.clone(),
                    });
                }
            }
        }
        out
    }

    /// Every skill in the resume, lower-cased, for checking the model's skill list.
    pub fn all_skills(&self) -> Vec<String> {
        self.skills
            .iter()
            .flat_map(|g| g.items.iter().map(|s| s.to_lowercase()))
            .collect()
    }

    /// The whole resume as plain text (for "is this word anywhere in it?" checks).
    pub fn full_text(&self) -> String {
        let mut parts = vec![self.name.clone()];
        parts.extend(self.contact.iter().cloned());
        parts.extend(self.evidence().into_iter().map(|e| e.text));
        for entry in self
            .experience
            .iter()
            .chain(&self.projects)
            .chain(&self.education)
        {
            parts.extend(entry.details.iter().cloned());
        }
        for section in &self.other {
            parts.push(section.heading.clone());
            parts.extend(section.lines.iter().map(|(_, t)| t.clone()));
        }
        parts.join("\n")
    }
}

/// "**Programming:** Python, SQL" -> group "Programming", items [Python, SQL].
fn parse_skill_line(text: &str) -> SkillGroup {
    let plain = text.replace("**", "");
    let (group, items) = match plain.split_once(':') {
        Some((group, items)) => (group.trim().to_string(), items),
        None => (String::new(), plain.as_str()),
    };
    SkillGroup {
        group,
        items: items
            .split(',')
            .map(|s| s.trim().trim_end_matches('.').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

pub fn skill_line(group: &SkillGroup) -> String {
    if group.group.is_empty() {
        group.items.join(", ")
    } else {
        format!("{}: {}", group.group, group.items.join(", "))
    }
}

/// "NPCI | Associate Data Scientist | Mumbai, India | Sep 2022 – Sep 2023"
pub fn entry_line(entry: &Entry) -> String {
    let mut parts = vec![entry.heading.clone()];
    parts.extend(entry.details.iter().map(|d| d.replace("**", "")));
    parts.retain(|p| !p.is_empty());
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESUME: &str = "# Jane Citizen

Sydney, NSW
[LinkedIn](https://linkedin.com/in/jane)

## Objective

Data analyst with SQL and Python
experience.

## Skills

* **Programming:** Python, SQL, PySpark
* **Visualisation:** Power BI, Tableau.

## Experience

### Acme Bank

**Data Analyst**
Sep 2022 – Sep 2023

* Built a fraud model with 80% recall.
* Automated daily reports for 400+ users.

### Beta Co

**Analyst**
* Wrote SQL ETL jobs.

## Projects

### Lakehouse

**Tools:** Databricks, dbt

* Built a medallion lakehouse.

## Education

### UTS

**Master of Data Science**
Feb 2024 – Jan 2026

## Certifications

* SQL (Advanced) – HackerRank
";

    #[test]
    fn parses_every_section() {
        let r = MasterResume::parse(RESUME);
        assert_eq!(r.name, "Jane Citizen");
        assert_eq!(
            r.contact,
            vec!["Sydney, NSW", "[LinkedIn](https://linkedin.com/in/jane)"]
        );
        assert_eq!(
            r.summary.as_deref(),
            Some("Data analyst with SQL and Python experience.")
        );
        assert_eq!(r.skills.len(), 2);
        assert_eq!(r.skills[0].group, "Programming");
        assert_eq!(r.skills[1].items, vec!["Power BI", "Tableau"]);

        assert_eq!(r.experience.len(), 2);
        assert_eq!(r.experience[0].id, "W1");
        assert_eq!(
            r.experience[0].details,
            vec!["**Data Analyst**", "Sep 2022 – Sep 2023"]
        );
        assert_eq!(r.experience[0].bullets[1].id, "W1.2");
        assert_eq!(r.experience[1].bullets[0].id, "W2.1");
        assert_eq!(r.projects[0].bullets[0].id, "P1.1");
        assert_eq!(r.education[0].id, "ED1");
        assert_eq!(r.other[0].heading, "Certifications");
        assert_eq!(r.other[0].lines[0].0, "C1");
    }

    #[test]
    fn evidence_numbers_every_fact() {
        let ids: Vec<String> = MasterResume::parse(RESUME)
            .evidence()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "S", "K1", "K2", "W1", "W1.1", "W1.2", "W2", "W2.1", "P1", "P1.1", "ED1", "C1"
            ]
        );
    }

    #[test]
    fn entry_line_joins_details_without_markdown() {
        let r = MasterResume::parse(RESUME);
        assert_eq!(
            entry_line(&r.experience[0]),
            "Acme Bank | Data Analyst | Sep 2022 – Sep 2023"
        );
    }

    #[test]
    fn parses_the_real_v1_resume_shape() {
        // No "##" before the contact lines, bullets with "-": still works.
        let r = MasterResume::parse("# A\n\nb@c.com\n\n## Experience\n\n### X\n\n- did y\n");
        assert_eq!(r.contact, vec!["b@c.com"]);
        assert_eq!(r.experience[0].bullets[0].text, "did y");
    }
}
