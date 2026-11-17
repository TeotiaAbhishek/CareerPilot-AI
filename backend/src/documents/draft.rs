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
    /// One title from your headline line, e.g. "Data Engineer".
    #[serde(default)]
    pub headline: Option<String>,
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

/// "Python (pandas, NumPy)" -> "python": the name without its details.
fn base_name(skill: &str) -> String {
    skill
        .split(" (")
        .next()
        .unwrap_or(skill)
        .trim()
        .to_lowercase()
}

fn clean_line(line: Line) -> Line {
    let mut evidence: Vec<String> = Vec::new();
    // Models sometimes put several ids in one string: "W1.1, W1.2".
    for id in line
        .evidence
        .iter()
        .flat_map(|ids| ids.split([',', ';', ' ']))
    {
        let id = id.trim().trim_matches(['[', ']']).to_string();
        if !id.is_empty() && !evidence.contains(&id) {
            evidence.push(id);
        }
    }
    Line {
        text: clean_text(&line.text),
        evidence,
        // Kept, so saving an edited draft doesn't lose its subheadings.
        group: line.group,
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
        line.group = line
            .evidence
            .iter()
            .find_map(|id| {
                entry
                    .bullets
                    .iter()
                    .find(|b| &b.id == id)
                    .and_then(|b| b.group.clone())
            })
            // Cited only the job ("W1"): the subheading whose bullets share
            // the most words with it.
            .or_else(|| closest_group(entry, &line.text));
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

fn closest_group(entry: &Entry, text: &str) -> Option<String> {
    let words = |t: &str| -> std::collections::HashSet<String> {
        t.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(String::from)
            .collect()
    };
    let line = words(text);
    entry
        .bullets
        .iter()
        .filter_map(|b| {
            Some((
                b.group.as_ref()?,
                words(&b.text).intersection(&line).count(),
            ))
        })
        .filter(|(_, shared)| *shared >= 3)
        .max_by_key(|(_, shared)| *shared)
        .map(|(g, _)| g.clone())
}

fn draft_entry(entry: &Entry, bullets: Vec<Line>) -> DraftEntry {
    DraftEntry {
        id: entry.id.clone(),
        heading: entry.heading.clone(),
        details: entry.details.clone(),
        bullets: group_bullets(entry, bullets),
    }
}

/// The skills section, always in your own groups and spelling. The model's
/// picks come first, then skills of yours the job ad names; with `all` (a
/// two-page resume) every other skill of yours follows, so the list backs up
/// everything the bullets mention. Groups are ordered by their first pick.
pub fn arrange_skills(draft: &mut ResumeDraft, master: &MasterResume, job_text: &str, all: bool) {
    let job = format!(" {} ", job_text.to_lowercase());
    let named = |skill: &str| {
        let name = base_name(skill);
        !name.is_empty()
            && job.match_indices(&name).any(|(i, _)| {
                let before = job[..i].chars().next_back();
                let after = job[i + name.len()..].chars().next();
                before.is_none_or(|c| !c.is_alphanumeric())
                    && after.is_none_or(|c| !c.is_alphanumeric())
            })
    };
    // Picks, best first: the model's, then the job's.
    let mut picks: Vec<String> = draft.skills.iter().flat_map(|g| g.items.clone()).collect();
    for skill in master.skills.iter().flat_map(|g| &g.items) {
        if named(skill) && !picks.contains(skill) {
            picks.push(skill.clone());
        }
    }
    let rank = |skill: &String| picks.iter().position(|p| p == skill);
    let mut groups: Vec<(usize, SkillGroup)> = master
        .skills
        .iter()
        .enumerate()
        .filter_map(|(i, group)| {
            let mut items: Vec<&String> = group
                .items
                .iter()
                .filter(|s| all || rank(s).is_some())
                .collect();
            if items.is_empty() {
                return None;
            }
            items.sort_by_key(|s| rank(s).unwrap_or(usize::MAX));
            let first = rank(items[0]).unwrap_or(picks.len() + i);
            Some((
                first,
                SkillGroup {
                    group: group.group.clone(),
                    items: items.into_iter().cloned().collect(),
                },
            ))
        })
        .collect();
    groups.sort_by_key(|(first, _)| *first);
    if !groups.is_empty() {
        draft.skills = groups.into_iter().map(|(_, g)| g).collect();
    }
}

/// On a two-page resume nothing from your jobs goes missing: every bullet of
/// the older jobs, and your certifications, come back in your own words when
/// the model dropped them, each under its subheading. In the most recent role
/// (a project per subheading) the model's picks stand.
pub fn keep_job_bullets(draft: &mut ResumeDraft, master: &MasterResume) {
    for (i, entry) in master.experience.iter().enumerate() {
        let Some(out) = draft.experience.iter_mut().find(|e| e.id == entry.id) else {
            continue;
        };
        let cited: Vec<String> = out
            .bullets
            .iter()
            .flat_map(|l| l.evidence.clone())
            .collect();
        let missing: Vec<Line> = original_bullets(entry)
            .into_iter()
            .filter(|l| !cited.contains(&l.evidence[0]))
            .filter(|l| {
                i > 0
                    || l.group
                        .as_deref()
                        .is_some_and(|g| g.to_lowercase().starts_with("certification"))
            })
            .collect();
        if !missing.is_empty() {
            out.bullets.extend(missing);
            out.bullets = group_bullets(entry, std::mem::take(&mut out.bullets));
        }
    }
}

/// Your contact lines with the headline cut to the one title the model
/// picked for this job. A title that isn't one of yours becomes your first.
fn resume_contact(master: &MasterResume, picked: Option<&str>) -> Vec<String> {
    let mut contact = master.contact.clone();
    if let Some((i, titles)) = master.headline() {
        let wanted = picked.unwrap_or("").trim().to_lowercase();
        let title = titles
            .iter()
            .find(|t| t.to_lowercase() == wanted)
            .unwrap_or(&titles[0]);
        contact[i] = title.clone();
    }
    contact
}

/// Split a resume's contact lines into the headline (the one job title code
/// put there, e.g. "Data Engineer") and the rest, so it can be shown larger.
pub fn split_headline(contact: &[String]) -> (Option<&String>, Vec<&String>) {
    let is_title = |line: &String| {
        !line.is_empty()
            && line.split_whitespace().count() <= 5
            && !line
                .chars()
                .any(|c| c.is_ascii_digit() || "@[|·,:/".contains(c))
    };
    match contact.iter().position(is_title) {
        Some(i) => (
            Some(&contact[i]),
            contact
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, l)| l)
                .collect(),
        ),
        None => (None, contact.iter().collect()),
    }
}

/// A letter's header has no headline, only your contact details.
fn letter_contact(master: &MasterResume) -> Vec<String> {
    let mut contact = master.contact.clone();
    if let Some((i, _)) = master.headline() {
        contact.remove(i);
    }
    contact
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
            let all = || master.skills.iter().flat_map(|g| &g.items);
            // "Python" also matches "Python (pandas, NumPy)", spelled as you wrote it.
            let found = all()
                .find(|s| s.to_lowercase() == wanted)
                .or_else(|| all().find(|s| base_name(s) == base_name(&wanted)));
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
        contact: resume_contact(master, answer.headline.as_deref()),
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
        contact: letter_contact(master),
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
            "# Jane\n\n## Skills\n\n* **Programming:** Python (pandas, NumPy), SQL\n\n## Experience\n\n### Acme\n\n**Analyst**\n\n* Built reports.\n* Wrote SQL.\n\n### Beta\n\n* Did ETL.\n\n## Projects\n\n### One\n\n* Made a thing.\n\n### Two\n\n* Made another.\n",
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
            headline: None,
            summary: line("Analyst — SQL", &["S", "S", "[K1]"]),
            skills: vec![SkillGroup {
                group: "Core".into(),
                items: vec![
                    "sql".into(),
                    "Snowflake".into(),
                    "SQL".into(),
                    "python".into(),
                ],
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
                items: vec!["SQL".into(), "Python (pandas, NumPy)".into()]
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
            headline: None,
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
            headline: None,
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

    #[test]
    fn the_headline_is_cut_to_the_title_picked_for_the_job() {
        let master = MasterResume::parse(
            "# Jane\n\nData Scientist | Data Engineer\nSydney · jane@x.com\n\n## Experience\n\n### Acme\n\n* Built it.\n",
        );
        let answer = |headline: Option<&str>| ResumeAnswer {
            headline: headline.map(String::from),
            summary: line("x", &["S"]),
            skills: vec![],
            experience: vec![],
            projects: vec![],
        };
        let (draft, _) = assemble_resume(&master, answer(Some("data engineer")));
        assert_eq!(draft.contact, vec!["Data Engineer", "Sydney · jane@x.com"]);
        // Not one of yours: your first title.
        let (draft, _) = assemble_resume(&master, answer(Some("Head of AI")));
        assert_eq!(draft.contact[0], "Data Scientist");
        // A letter's header has no headline.
        let letter = assemble_letter(&master, LetterAnswer { paragraphs: vec![] });
        assert_eq!(letter.contact, vec!["Sydney · jane@x.com"]);
    }

    #[test]
    fn skills_keep_your_groups_with_the_picks_first() {
        let master = MasterResume::parse(
            "## Skills\n\n* **Programming:** Python (pandas, NumPy), SQL, R\n* **BI:** Power BI, Tableau\n* **Cloud:** Azure\n",
        );
        // The model renamed a group and picked only SQL.
        let picked = || ResumeDraft {
            skills: vec![SkillGroup {
                group: "Core".into(),
                items: vec!["SQL".into()],
            }],
            ..ResumeDraft::default()
        };
        let job = "You know Python, SQL and Power BI. Report to the CFO.";
        let group = |name: &str, items: &[&str]| SkillGroup {
            group: name.into(),
            items: items.iter().map(|s| s.to_string()).collect(),
        };

        // One page: the picks and the job's skills, in your groups.
        let mut draft = picked();
        arrange_skills(&mut draft, &master, job, false);
        assert_eq!(
            draft.skills,
            vec![
                group("Programming", &["SQL", "Python (pandas, NumPy)"]),
                group("BI", &["Power BI"]),
            ],
            "R isn't matched inside other words"
        );

        // Two pages: everything, picks first.
        let mut draft = picked();
        arrange_skills(&mut draft, &master, job, true);
        assert_eq!(
            draft.skills,
            vec![
                group("Programming", &["SQL", "Python (pandas, NumPy)", "R"]),
                group("BI", &["Power BI", "Tableau"]),
                group("Cloud", &["Azure"]),
            ]
        );
    }

    #[test]
    fn a_bullet_citing_only_the_job_finds_its_subheading() {
        let master = MasterResume::parse(
            "## Experience\n\n### Independent\n\n#### Lakehouse\n\n* Built a Databricks medallion lakehouse with dbt tests.\n\n#### Job app\n\n* Built a job search app that scrapes SEEK daily.\n",
        );
        let answer = ResumeAnswer {
            headline: None,
            summary: line("x", &["S"]),
            skills: vec![],
            experience: vec![EntryAnswer {
                id: "W1".into(),
                bullets: vec![
                    line(
                        "Built a job search app that scrapes SEEK listings daily.",
                        &["W1"],
                    ),
                    line(
                        "Built a medallion lakehouse on Databricks with dbt tests.",
                        &["W1.1, W1"],
                    ),
                ],
            }],
            projects: vec![],
        };
        let (draft, _) = assemble_resume(&master, answer);
        let got: Vec<_> = draft.experience[0]
            .bullets
            .iter()
            .map(|l| (l.group.as_deref(), l.evidence.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    Some("Lakehouse"),
                    vec!["W1.1".to_string(), "W1".to_string()]
                ),
                (Some("Job app"), vec!["W1".to_string()]),
            ]
        );
    }

    #[test]
    fn two_pages_keep_every_older_job_bullet_and_your_certifications() {
        let master = MasterResume::parse(
            "## Experience\n\n### Independent\n\n#### Lakehouse\n\n* Built a lakehouse.\n* Tested it.\n\n#### Certifications\n\n* SQL (Advanced).\n\n### NPCI\n\n#### Fraud modelling\n\n* Built a model.\n\n#### Fraud reporting\n\n* Automated reports.\n* Built dashboards.\n\n### Capgemini\n\n* Built ETL.\n* Built reports.\n",
        );
        let answer = ResumeAnswer {
            headline: None,
            summary: line("x", &["S"]),
            skills: vec![],
            experience: vec![
                EntryAnswer {
                    id: "W1".into(),
                    bullets: vec![line("Built a Databricks lakehouse.", &["W1.1"])],
                },
                EntryAnswer {
                    id: "W2".into(),
                    bullets: vec![
                        line("Built dashboards for banks.", &["W2.3"]),
                        line("Built a fraud model.", &["W2.1"]),
                    ],
                },
                EntryAnswer {
                    id: "W3".into(),
                    bullets: vec![line("Built SQL ETL.", &["W3.1"])],
                },
            ],
            projects: vec![],
        };
        let (mut draft, _) = assemble_resume(&master, answer);
        keep_job_bullets(&mut draft, &master);
        let texts = |i: usize| -> Vec<(Option<String>, String)> {
            draft.experience[i]
                .bullets
                .iter()
                .map(|l| (l.group.clone(), l.text.clone()))
                .collect()
        };
        let g = |group: Option<&str>, text: &str| (group.map(String::from), text.to_string());
        // The most recent role: the model's picks, plus your certifications.
        assert_eq!(
            texts(0),
            vec![
                g(Some("Lakehouse"), "Built a Databricks lakehouse."),
                g(Some("Certifications"), "SQL (Advanced)."),
            ]
        );
        // Older jobs: everything, the model's wording first in each subheading.
        assert_eq!(
            texts(1),
            vec![
                g(Some("Fraud modelling"), "Built a fraud model."),
                g(Some("Fraud reporting"), "Built dashboards for banks."),
                g(Some("Fraud reporting"), "Automated reports."),
            ]
        );
        assert_eq!(
            texts(2),
            vec![g(None, "Built SQL ETL."), g(None, "Built reports.")]
        );
    }

    #[test]
    fn the_headline_is_split_from_the_contact_lines() {
        let contact = vec![
            "Data Engineer".to_string(),
            "Sydney, NSW · 0400 000 000 · a@b.com".to_string(),
            "[LinkedIn](https://x) · [GitHub](https://y)".to_string(),
        ];
        let (headline, rest) = split_headline(&contact);
        assert_eq!(headline.map(String::as_str), Some("Data Engineer"));
        assert_eq!(rest.len(), 2);
        // A letter's contact lines have no title.
        let (headline, rest) = split_headline(&contact[1..]);
        assert_eq!(headline, None);
        assert_eq!(rest.len(), 2);
    }

    #[test]
    fn saving_a_draft_keeps_its_subheadings() {
        let draft = ResumeDraft {
            experience: vec![DraftEntry {
                id: "W1".into(),
                bullets: vec![Line {
                    text: "Built a lakehouse.".into(),
                    evidence: vec!["W1.1".into()],
                    group: Some("Lakehouse".into()),
                }],
                ..DraftEntry::default()
            }],
            ..ResumeDraft::default()
        };
        let saved = tidy_resume(draft);
        assert_eq!(
            saved.experience[0].bullets[0].group.as_deref(),
            Some("Lakehouse")
        );
    }
}
