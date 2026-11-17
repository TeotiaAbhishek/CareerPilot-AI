//! Prompts for drafting. Ported from v1's document_generator.py, changed so
//! the model works from numbered evidence and must cite it.

use serde_json::{Value, json};

use super::resume::{MasterResume, entry_line, skill_line};

/// Bump when a prompt changes, so drafts record which prompt wrote them.
pub const PROMPT_VERSION: &str = "v2-order";

/// Rules against text that reads as AI-written, from Wikipedia's "Signs of
/// AI writing". Shared by resumes, cover letters and emails; `style.rs`
/// flags what still slips through.
macro_rules! plain_writing {
    () => {
        "PLAIN WRITING (it must read like the candidate wrote it, not a chatbot):
- Plain verbs: used, built, led, ran, cut, wrote, fixed. Never: leverage, utilise, spearhead, delve, showcase, underscore, foster, bolster, harness, elevate, unlock, empower, embark.
- No puffery: pivotal, vibrant, seamless, dynamic, cutting-edge, groundbreaking, transformative, meticulous, holistic, world-class, passionate, results-driven, proven track record.
- Don't tack a significance clause onto the end of a sentence (\", ensuring ...\", \", highlighting ...\", \", showcasing ...\", \", reflecting ...\"). End the sentence, or state the concrete result.
- Write \"is\" or \"has\", not \"serves as\", \"stands as\" or \"boasts\". No \"not only ... but also\" or \"not just X, but Y\".
- Don't start sentences with Additionally, Furthermore, Moreover or Overall.
- Don't group things in threes out of habit: list what the evidence has, no more.
- Vary sentence length and shape. Not every bullet should follow the same pattern.
- Be specific: what was built, for whom, with which tool, and the number when the evidence has one.
- Straight quotes. No dashes as punctuation: use a comma, a full stop or \"and\".
"
    };
}
pub(crate) use plain_writing;

pub const RESUME_SYSTEM: &str = concat!("You are a technical recruiter with 25 years of experience hiring for data, analytics and AI roles. You are rewriting a candidate's resume for one job so that a recruiter shortlists it in a 30-second scan, and an ATS matches it.

The EVIDENCE list is the ONLY source of truth about the candidate. Each item has an id in square brackets.

HOW A RECRUITER READS IT:
- The top third decides: a headline and summary that say what the candidate does, and the strongest evidence for THIS job first.
- One achievement per bullet: what was built or done, with which tools, and the result or number. About 15-30 words. Never merge facts from different evidence bullets into one long bullet.
- Keep the substance. Rewording a bullet toward the job is good; deleting relevant work to save space is not. Follow the LENGTH in the TASK.
- A complete skills list: every skill the experience mentions belongs there, the job's skills first.
- Projects count as experience for a graduate. Keep the projects most relevant to the job, each with its own bullets.

YOU MAY:
- choose the headline: the ONE title from HEADLINE OPTIONS that best fits the job, copied exactly
- write the professional summary: 3 sentences, 45-70 words. First, the headline title you chose and what the candidate does, in the job's own terms. Second, the strongest evidence for THIS job, with its number. Third, the paid work that best matches the job (for example reporting for stakeholders) or the most relevant project, and the degree. Never open with \"Data professional\", and don't list every tool
- order the skills so the job's ones come first (code adds back any you leave out)
- rewrite and reorder bullets under each job, to emphasise what the job asks for; drop only weaker projects or bullets of the most recent role
- order the projects (subheadings) of the most recent role by how well they match the job: code keeps your order
- use the job's wording where the evidence supports it

YOU MUST NOT:
- invent skills, technologies, employers, projects, certifications, responsibilities, achievements or metrics
- change or add numbers: every number must appear in the evidence you cite
- claim a technology because the job asks for it
- exaggerate seniority or describe completed study as ongoing

CITATIONS:
- Every bullet and the summary must list the ids of the evidence it is based on, one id per array item.
- A bullet under a job cites that job's bullet ids (W1.3), not the job's id (W1).
- Some jobs list their bullets under subheadings (for example one project each). Code puts each bullet back under the subheading of the evidence it cites, so don't repeat the subheading's name at the start of a bullet.
- If you can't support a statement with evidence, leave it out.

",
    plain_writing!(),
    "
STYLE:
- Australian English. Concise, specific, ATS-friendly. The TASK says how long.
- Start bullets with a strong verb. No em dashes or en dashes.
- Include every job id from the evidence under \"experience\" (you choose its bullets).
- Return only JSON that matches the schema.");

pub const LETTER_SYSTEM: &str = concat!("You are writing a concise, professional cover letter for one job.

The EVIDENCE list is the ONLY source of truth about the candidate. Each item has an id in square brackets.

YOU MUST NOT:
- invent skills, technologies, achievements, responsibilities, metrics, employers or dates
- change or add numbers: every number must appear in the evidence you cite
- claim knowledge of the company beyond what the job description says
- describe completed study as ongoing

",
    plain_writing!(),
    "
STYLE:
- Australian English. Confident, natural, not exaggerated. 250-350 words in 3-4 short paragraphs.
- Avoid clichés like \"I am writing to express my interest\", \"I am thrilled to apply\" or \"perfect fit\".
- No greeting or sign-off: those are added for you. No em dashes or en dashes.
- Paragraph 1: the role and the strongest reason the candidate is relevant.
- Paragraph 2: the most relevant experience, with measurable evidence.
- Paragraph 3: relevant skills, projects or education.
- Last paragraph: a short close about contributing and discussing the role.
- Every paragraph must list the ids of the evidence it uses.
- Return only JSON that matches the schema.");

pub struct JobForPrompt<'a> {
    pub title: &'a str,
    pub company: Option<&'a str>,
    pub location: Option<&'a str>,
    pub work_type: Option<&'a str>,
    pub description: &'a str,
    pub matched: &'a [String],
    pub missing: &'a [String],
}

/// The evidence block, grouped so the model sees which bullet belongs where.
pub fn evidence_block(master: &MasterResume) -> String {
    let mut out = String::new();
    if let Some(summary) = &master.summary {
        out.push_str(&format!("SUMMARY\n[S] {summary}\n\n"));
    }
    if !master.skills.is_empty() {
        out.push_str("SKILLS\n");
        for (i, group) in master.skills.iter().enumerate() {
            out.push_str(&format!("[K{}] {}\n", i + 1, skill_line(group)));
        }
        out.push('\n');
    }
    for (title, entries) in [
        ("EXPERIENCE", &master.experience),
        ("PROJECTS", &master.projects),
        ("EDUCATION", &master.education),
    ] {
        if entries.is_empty() {
            continue;
        }
        out.push_str(title);
        out.push('\n');
        for entry in entries {
            out.push_str(&format!("[{}] {}\n", entry.id, entry_line(entry)));
            let mut group: Option<&String> = None;
            for bullet in &entry.bullets {
                if bullet.group.is_some() && bullet.group.as_ref() != group {
                    group = bullet.group.as_ref();
                    out.push_str(&format!("  {}:\n", bullet.group.as_deref().unwrap_or("")));
                }
                out.push_str(&format!("  [{}] {}\n", bullet.id, bullet.text));
            }
        }
        out.push('\n');
    }
    for section in &master.other {
        let cited: Vec<_> = section
            .lines
            .iter()
            .filter(|(id, _)| !id.is_empty())
            .collect();
        if cited.is_empty() {
            continue;
        }
        out.push_str(&section.heading.to_uppercase());
        out.push('\n');
        for (id, text) in cited {
            out.push_str(&format!("[{id}] {text}\n"));
        }
        out.push('\n');
    }
    out
}

pub fn user_prompt(
    master: &MasterResume,
    job: &JobForPrompt,
    note: Option<&str>,
    task: &str,
) -> String {
    let opt = |v: Option<&str>| v.unwrap_or("not stated").to_string();
    let list = |items: &[String]| {
        if items.is_empty() {
            "none".to_string()
        } else {
            items.join("; ")
        }
    };
    let note = note
        .filter(|n| !n.trim().is_empty())
        .map(|n| {
            format!(
                "\nCANDIDATE'S NOTE FOR THIS DRAFT\n===============================\n{}\n",
                n.trim()
            )
        })
        .unwrap_or_default();
    let headline = master
        .headline()
        .map(|(_, titles)| {
            format!(
                "HEADLINE OPTIONS\n================\n{}\n\n",
                titles.join("; ")
            )
        })
        .unwrap_or_default();
    format!(
        "EVIDENCE
========
{evidence}
{headline}TARGET JOB
==========
Title: {title}
Company: {company}
Location: {location}
Work type: {work_type}

RELEVANCE ANALYSIS
==================
Matched requirements: {matched}
Missing requirements: {missing}

JOB DESCRIPTION
===============
{description}
{note}
TASK
====
{task}",
        evidence = evidence_block(master),
        title = job.title,
        company = opt(job.company),
        location = opt(job.location),
        work_type = opt(job.work_type),
        matched = list(job.matched),
        missing = list(job.missing),
        description = job.description,
    )
}

/// The resume task, sized to fill `pages` pages (1 or 2).
pub fn resume_task(pages: i32) -> String {
    let length = if pages >= 2 {
        "LENGTH: two full pages. For the most recent role, keep 3-5 bullets for each of the 3-5 projects \
or subheadings most relevant to the job. Rewrite every bullet of the older roles toward the job \
(code adds back any you leave out). Use the evidence: don't pad."
    } else {
        "LENGTH: one page. Keep 2-3 bullets for each of the 2-3 most relevant projects or subheadings \
of the most recent role, 3-4 for the next role and 2-3 for older ones."
    };
    format!(
        "Tailor the resume for this job. Emphasise the strongest genuine matches. \
Don't try to cover missing requirements by inventing experience. Cite evidence ids for every line.\n{length}"
    )
}

pub const LETTER_TASK: &str = "Write the body of a tailored cover letter for this job. \
Focus on the strongest genuine connections. Use measurable achievements where relevant. \
Don't mention missing requirements or invent experience to cover them. Cite evidence ids for every paragraph.";

fn line_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": { "type": "string" },
            "evidence": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["text", "evidence"]
    })
}

pub fn resume_schema() -> Value {
    let entry = json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "bullets": { "type": "array", "items": line_schema() }
        },
        "required": ["id", "bullets"]
    });
    json!({
        "type": "object",
        "properties": {
            "summary": line_schema(),
            "skills": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "group": { "type": "string" },
                        "items": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["group", "items"]
                }
            },
            "experience": { "type": "array", "items": entry },
            "projects": { "type": "array", "items": entry },
            "headline": { "type": "string" }
        },
        "required": ["headline", "summary", "skills", "experience", "projects"]
    })
}

pub fn letter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "paragraphs": { "type": "array", "items": line_schema() }
        },
        "required": ["paragraphs"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_numbers_the_evidence_and_has_no_contact_details() {
        let master = MasterResume::parse(
            "# Jane\n\njane@example.com\n0400 000 000\n\n## Skills\n\n* **Programming:** Python\n\n## Experience\n\n### Acme\n\n**Analyst**\n\n* Built reports.\n",
        );
        let job = JobForPrompt {
            title: "Data Analyst",
            company: Some("Acme"),
            location: None,
            work_type: None,
            description: "SQL please",
            matched: &[],
            missing: &["dbt".to_string()],
        };
        let prompt = user_prompt(&master, &job, Some("focus on SQL"), &resume_task(2));
        assert!(prompt.contains("[K1] Programming: Python"));
        assert!(prompt.contains("[W1] Acme | Analyst"));
        assert!(prompt.contains("  [W1.1] Built reports."));
        assert!(prompt.contains("Missing requirements: dbt"));
        assert!(prompt.contains("focus on SQL"));
        assert!(prompt.contains("LENGTH: two full pages"));
        assert!(resume_task(1).contains("LENGTH: one page"));
        assert!(
            !prompt.contains("jane@example.com"),
            "contact details never go to the model"
        );
        assert!(!prompt.contains("0400"));
    }
}
