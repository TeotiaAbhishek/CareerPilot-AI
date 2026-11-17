//! A draft resume or cover letter, built from the model's answer and your
//! master resume.
//!
//! The model only chooses and rewrites: the summary, which skills to show,
//! each job's bullets, and which projects to include. Everything factual that
//! frames them (your name, contact lines, employers, titles, dates, degrees,
//! certifications) is copied from the master resume by code, so the model
//! can't change it.

use serde::{Deserialize, Serialize};

use super::resume::{Entry, MasterResume, OtherSection, SkillGroup, groups};

/// One sentence, bullet or paragraph, with the evidence ids it relies on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    /// The subheading it sits under (a project inside a job). Set by code
    /// from the evidence it cites, never by the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DraftEntry {
    pub id: String,
    pub heading: String,
    pub details: Vec<String>,
    pub bullets: Vec<Line>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResumeDraft {
    pub name: String,
    pub contact: Vec<String>,
    pub summary: Line,
    pub skills: Vec<SkillGroup>,
    pub experience: Vec<DraftEntry>,
    pub projects: Vec<DraftEntry>,
    pub education: Vec<DraftEntry>,
    pub other: Vec<OtherSection>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LetterDraft {
    pub name: String,
    pub contact: Vec<String>,
    pub greeting: String,
    pub paragraphs: Vec<Line>,
    pub closing: String,
}

// ---------- What the model returns ----------

#[derive(Debug, Deserialize)]
pub struct ResumeAnswer {
    pub summary: Line,
    #[serde(default)]
    pub skills: Vec<SkillGroup>,
    #[serde(default)]
    pub experience: Vec<EntryAnswer>,
    #[serde(default)]
    pub projects: Vec<EntryAnswer>,
}

#[derive(Debug, Deserialize)]
pub struct EntryAnswer {
    pub id: String,
    #[serde(default)]
    pub bullets: Vec<Line>,
}

#[derive(Debug, Deserialize)]
pub struct LetterAnswer {
    pub paragraphs: Vec<Line>,
}

/// No em or en dashes (a strong sign of AI writing) and straight quotes.
/// "A — B" becomes "A, B"; "2019–2021" becomes "2019-2021".
pub fn clean_text(text: &str) -> String {
    let text = text
        .replace(['‘', '’'], "'")
        .replace(['“', '”'], "\"")
        .replace(" — ", ", ")
        .replace(" – ", ", ")
        .replace('—', ", ")
        .replace('–', "-");
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.replace(",,", ",").replace(" ,", ",")
}

fn clean_line(line: Line) -> Line {
    let mut evidence: Vec<String> = Vec::new();
    for id in line.evidence {
        let id = id.trim().trim_matches(['[', ']']).to_string();
        if !id.is_empty() && !evidence.contains(&id) {
            evidence.push(id);
        }
    }
    Line {
        text: clean_text(&line.text),
        evidence,
        group: None,
    }
}

/// Keep the master's bullets, each citing itself.
fn original_bullets(entry: &Entry) -> Vec<Line> {
    entry
        .bullets
        .iter()
        .map(|b| Line {
            text: b.text.clone(),
            evidence: vec![b.id.clone()],
            group: b.group.clone(),
        })
        .collect()
}

/// Put each bullet under the subheading of the first bullet it cites, and
/// keep the subheadings in your order. Bullets keep their order within one.
fn group_bullets(entry: &Entry, mut bullets: Vec<Line>) -> Vec<Line> {
    let order = groups(entry);
    if order.is_empty() {
        return bullets;
    }
    for line in &mut bullets {
        line.group = line.evidence.iter().find_map(|id| {
            entry
                .bullets
                .iter()
                .find(|b| &b.id == id)
                .and_then(|b| b.group.clone())
        });
    }
    // Lines without a subheading go first, straight under the job.
    bullets.sort_by_key(|l| {
        l.group
            .as_ref()
            .and_then(|g| order.iter().position(|o| o == g))
            .map_or(0, |i| i + 1)
    });
    bullets
}

fn draft_entry(entry: &Entry, bullets: Vec<Line>) -> DraftEntry {
    DraftEntry {
        id: entry.id.clone(),
        heading: entry.heading.clone(),
        details: entry.details.clone(),
        bullets: group_bullets(entry, bullets),
    }
}

/// Build the draft. Returns the draft and notes about what code corrected
/// (skills the model added that aren't yours, unknown entries).
pub fn assemble_resume(master: &MasterResume, answer: ResumeAnswer) -> (ResumeDraft, Vec<String>) {
    let mut notes = Vec::new();

    // Skills: only ones in your resume, spelled as you spell them.
    let mut skills: Vec<SkillGroup> = Vec::new();
    for group in answer.skills {
        let mut items: Vec<String> = Vec::new();
        for item in group.items {
            let wanted = item.trim().to_lowercase();
            let found = master
                .skills
                .iter()
                .flat_map(|g| &g.items)
                .find(|s| s.to_lowercase() == wanted);
            match found {
                Some(s) if !items.contains(s) => items.push(s.clone()),
                Some(_) => {}
                None if !wanted.is_empty() => notes.push(format!(
                    "Removed the skill \"{}\": it isn't in your resume.",
                    item.trim()
                )),
                None => {}
            }
        }
        if !items.is_empty() {
            skills.push(SkillGroup {
                group: clean_text(&group.group),
                items,
            });
        }
    }
    if skills.is_empty() {
        skills = master.skills.clone();
    }

    // Every job stays, in your order. The model only rewrites the bullets.
    let experience = master
        .experience
        .iter()
        .map(|entry| {
            let bullets: Vec<Line> = answer
                .experience
                .iter()
                .find(|a| a.id == entry.id)
                .map(|a| {
                    a.bullets
                        .iter()
                        .cloned()
                        .map(clean_line)
                        .filter(|l| !l.text.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let bullets = if bullets.is_empty() {
                original_bullets(entry)
            } else {
                bullets
            };
            draft_entry(entry, bullets)
        })
        .collect();
    for a in &answer.experience {
        if !master.experience.iter().any(|e| e.id == a.id) {
            notes.push(format!(
                "Ignored a job \"{}\" that isn't in your resume.",
                a.id
            ));
        }
    }

    // Projects: the model picks which (and their order).
    let mut projects: Vec<DraftEntry> = Vec::new();
    for a in answer.projects {
        let Some(entry) = master.projects.iter().find(|p| p.id == a.id) else {
            notes.push(format!(
                "Ignored a project \"{}\" that isn't in your resume.",
                a.id
            ));
            continue;
        };
        if projects.iter().any(|p| p.id == entry.id) {
            continue;
        }
        let bullets: Vec<Line> = a
            .bullets
            .into_iter()
            .map(clean_line)
            .filter(|l| !l.text.is_empty())
            .collect();
        let bullets = if bullets.is_empty() {
            original_bullets(entry)
        } else {
            bullets
        };
        projects.push(draft_entry(entry, bullets));
    }
    if projects.is_empty() {
        projects = master
            .projects
            .iter()
            .map(|p| draft_entry(p, original_bullets(p)))
            .collect();
    }

    let draft = ResumeDraft {
        name: master.name.clone(),
        contact: master.contact.clone(),
        summary: clean_line(answer.summary),
        skills,
        experience,
        projects,
        education: master
            .education
            .iter()
            .map(|e| draft_entry(e, original_bullets(e)))
            .collect(),
        other: master.other.clone(),
    };
    (draft, notes)
}

pub fn assemble_letter(master: &MasterResume, answer: LetterAnswer) -> LetterDraft {
    LetterDraft {
        name: master.name.clone(),
        contact: master.contact.clone(),
        greeting: "Dear Hiring Manager,".into(),
        paragraphs: answer
            .paragraphs
            .into_iter()
            .map(clean_line)
            .filter(|l| !l.text.is_empty())
            .collect(),
        closing: "Kind regards,".into(),
    }
}

pub fn word_count(letter: &LetterDraft) -> usize {
    letter
        .paragraphs
        .iter()
        .map(|p| p.text.split_whitespace().count())
        .sum()
}

/// Tidy an edited draft from the UI (trim text, drop empty bullets).
pub fn tidy_resume(mut draft: ResumeDraft) -> ResumeDraft {
    draft.summary = clean_line(draft.summary);
    for entry in draft.experience.iter_mut().chain(draft.projects.iter_mut()) {
        entry.bullets = std::mem::take(&mut entry.bullets)
            .into_iter()
            .map(clean_line)
            .filter(|l| !l.text.is_empty())
            .collect();
    }
    draft
}

pub fn tidy_letter(mut draft: LetterDraft) -> LetterDraft {
    draft.paragraphs = std::mem::take(&mut draft.paragraphs)
        .into_iter()
        .map(clean_line)
        .filter(|l| !l.text.is_empty())
        .collect();
    draft.greeting = draft.greeting.trim().to_string();
    draft.closing = draft.closing.trim().to_string();
    draft
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master() -> MasterResume {
        MasterResume::parse(
            "# Jane\n\n## Skills\n\n* **Programming:** Python, SQL\n\n## Experience\n\n### Acme\n\n**Analyst**\n\n* Built reports.\n* Wrote SQL.\n\n### Beta\n\n* Did ETL.\n\n## Projects\n\n### One\n\n* Made a thing.\n\n### Two\n\n* Made another.\n",
        )
    }

    fn line(text: &str, ids: &[&str]) -> Line {
        Line {
            text: text.into(),
            evidence: ids.iter().map(|s| s.to_string()).collect(),
            group: None,
        }
    }

    #[test]
    fn keeps_every_job_and_only_real_skills() {
        let answer = ResumeAnswer {
            summary: line("Analyst — SQL", &["S", "S", "[K1]"]),
            skills: vec![SkillGroup {
                group: "Core".into(),
                items: vec!["sql".into(), "Snowflake".into(), "SQL".into()],
            }],
            experience: vec![EntryAnswer {
                id: "W1".into(),
                bullets: vec![line("Built SQL reports.", &["W1.1"])],
            }],
            projects: vec![
                EntryAnswer {
                    id: "P2".into(),
                    bullets: vec![],
                },
                EntryAnswer {
                    id: "P9".into(),
                    bullets: vec![],
                },
            ],
        };
        let (draft, notes) = assemble_resume(&master(), answer);

        // A spaced dash becomes a comma.
        assert_eq!(draft.summary, line("Analyst, SQL", &["S", "K1"]));
        assert_eq!(
            draft.skills,
            vec![SkillGroup {
                group: "Core".into(),
                items: vec!["SQL".into()]
            }]
        );
        assert_eq!(
            draft.experience.len(),
            2,
            "a job the model skipped is still there"
        );
        assert_eq!(draft.experience[0].details, vec!["**Analyst**"]);
        assert_eq!(
            draft.experience[1].bullets,
            vec![line("Did ETL.", &["W2.1"])]
        );
        assert_eq!(draft.projects.len(), 1);
        assert_eq!(draft.projects[0].heading, "Two");
        assert_eq!(notes.len(), 2, "{notes:?}");
    }

    #[test]
    fn falls_back_to_your_skills_and_projects() {
        let answer = ResumeAnswer {
            summary: line("x", &["S"]),
            skills: vec![],
            experience: vec![],
            projects: vec![],
        };
        let (draft, _) = assemble_resume(&master(), answer);
        assert_eq!(draft.skills, master().skills);
        assert_eq!(draft.projects.len(), 2);
    }

    #[test]
    fn bullets_go_back_under_their_subheadings() {
        let master = MasterResume::parse(
            "## Experience\n\n### Independent\n\n#### Lakehouse\n\n* Built it.\n\n#### Job app\n\n* Shipped it.\n* Tested it.\n",
        );
        let answer = ResumeAnswer {
            summary: line("x", &["S"]),
            skills: vec![],
            // The model mixes the order up and invents a group: code fixes both.
            experience: vec![EntryAnswer {
                id: "W1".into(),
                bullets: vec![
                    line("Tested the job app.", &["W1.3"]),
                    Line {
                        group: Some("Made up".into()),
                        ..line("Built a lakehouse.", &["W1.1"])
                    },
                    line("Shipped the job app.", &["W1.2"]),
                ],
            }],
            projects: vec![],
        };
        let (draft, _) = assemble_resume(&master, answer);
        let got: Vec<_> = draft.experience[0]
            .bullets
            .iter()
            .map(|l| (l.group.as_deref(), l.text.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (Some("Lakehouse"), "Built a lakehouse."),
                (Some("Job app"), "Tested the job app."),
                (Some("Job app"), "Shipped the job app."),
            ]
        );
    }
}
