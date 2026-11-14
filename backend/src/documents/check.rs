//! The fabrication checker. Rule-based, so it's fast, free and predictable.
//!
//! For every line the model wrote it checks:
//! 1. it cites at least one evidence id, and each id exists;
//! 2. every number in it (80%, 300M+, 1:20) appears in the evidence it cites;
//! 3. every name-like word (Snowflake, AWS, PySpark) appears somewhere in
//!    your resume, or in the job's title, company or location.
//!
//! "error" flags block approval until you fix the text or dismiss the flag.
//! "info" flags are only for your attention.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    draft::{LetterDraft, Line, ResumeDraft, word_count},
    resume::MasterResume,
};

pub const MAX_LETTER_WORDS: usize = 350;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flag {
    /// Stable id, used to dismiss it.
    pub key: String,
    /// Where it is: "summary", "experience.W1.0", "paragraphs.2", "document".
    pub path: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Info,
}

/// Words that start sentences in resumes and letters and aren't claims.
const COMMON_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "i",
    "i'm",
    "i've",
    "my",
    "me",
    "we",
    "our",
    "you",
    "your",
    "this",
    "that",
    "these",
    "those",
    "it",
    "in",
    "on",
    "at",
    "to",
    "for",
    "with",
    "by",
    "from",
    "of",
    "and",
    "or",
    "as",
    "across",
    "through",
    "while",
    "during",
    "using",
    "over",
    "under",
    "after",
    "before",
    "since",
    "having",
    "with",
    "dear",
    "hiring",
    "manager",
    "team",
    "kind",
    "regards",
    "sincerely",
    "thank",
    "thanks",
    "yours",
    "built",
    "build",
    "building",
    "developed",
    "develop",
    "developing",
    "designed",
    "design",
    "created",
    "create",
    "delivered",
    "deliver",
    "led",
    "lead",
    "leading",
    "managed",
    "manage",
    "improved",
    "improve",
    "automated",
    "automate",
    "performed",
    "maintained",
    "maintain",
    "supported",
    "support",
    "implemented",
    "implement",
    "analysed",
    "analyzed",
    "analyse",
    "analyze",
    "engineered",
    "partnered",
    "collaborated",
    "collaborate",
    "enabled",
    "enable",
    "reduced",
    "reduce",
    "increased",
    "increase",
    "applied",
    "apply",
    "used",
    "use",
    "worked",
    "work",
    "working",
    "translated",
    "presented",
    "communicated",
    "owned",
    "streamlined",
    "optimised",
    "optimized",
    "deployed",
    "deploy",
    "trained",
    "monitored",
    "produced",
    "prepared",
    "transformed",
    "cleaned",
    "modelled",
    "modeled",
    "wrote",
    "write",
    "my",
    "most",
    "recently",
    "currently",
    "previously",
    "at",
    "data",
    "experience",
    "experienced",
    "skilled",
    "proven",
    "strong",
    "hands-on",
    "passionate",
    "motivated",
    "detail-oriented",
    "results-driven",
    "professional",
    "graduate",
    "master",
    "masters",
    "bachelor",
    "university",
    "degree",
    "role",
    "position",
    "opportunity",
    "company",
    "organisation",
    "organization",
    "business",
    "stakeholders",
    "customers",
    "clients",
    "insights",
    "australia",
    "australian",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "monday",
    "friday",
    "if",
    "when",
    "where",
    "which",
    "what",
    "how",
    "why",
    "there",
    "here",
    "also",
    "both",
    "each",
    "all",
    "every",
    "key",
    "highlights",
    "summary",
    "skills",
    "projects",
    "education",
    "certifications",
    "tools",
    "programming",
    "machine",
    "learning",
    "cloud",
    "databases",
    "visualisation",
    "visualization",
    "engineering",
    "analytics",
    "science",
    "scientist",
    "analyst",
    "engineer",
    "developer",
    "associate",
    "senior",
    "junior",
];

/// What the checker compares against.
pub struct CheckContext {
    evidence: HashMap<String, String>,
    resume_text: String,
    /// Lower-cased words from the job's title, company and location.
    job_words: Vec<String>,
}

impl CheckContext {
    pub fn new(
        master: &MasterResume,
        job_title: &str,
        company: Option<&str>,
        location: Option<&str>,
    ) -> Self {
        let job_words = [Some(job_title), company, location]
            .into_iter()
            .flatten()
            .flat_map(words)
            .map(|w| w.to_lowercase())
            .collect();
        Self {
            evidence: master
                .evidence()
                .into_iter()
                .map(|e| (e.id, e.text))
                .collect(),
            resume_text: master.full_text().to_lowercase(),
            job_words,
        }
    }

    fn check_line(&self, path: &str, line: &Line, flags: &mut Vec<Flag>) {
        let mut push = |kind: &str, detail: &str, message: String| {
            flags.push(Flag {
                key: format!("{kind}|{path}|{detail}"),
                path: path.to_string(),
                severity: Severity::Error,
                message,
            })
        };

        if line.evidence.is_empty() {
            push(
                "no_evidence",
                "",
                "Doesn't cite anything from your resume. Check it's true.".into(),
            );
        }
        let mut cited = String::new();
        for id in &line.evidence {
            match self.evidence.get(id) {
                Some(text) => {
                    cited.push_str(text);
                    cited.push('\n');
                }
                None => push(
                    "unknown_evidence",
                    id,
                    format!("Cites {id}, which isn't in your resume."),
                ),
            }
        }

        for number in numbers(&line.text) {
            if !cited.contains(&number) {
                push(
                    "number",
                    &number,
                    format!(
                        "\"{number}\" isn't in the evidence this line cites. Make sure it's accurate."
                    ),
                );
            }
        }

        for word in words(&line.text) {
            // "I'd", "Acme's": check the part before the apostrophe.
            let word = word
                .split(['\'', '’'])
                .next()
                .unwrap_or_default()
                .to_string();
            if self.is_known(&word) {
                continue;
            }
            push(
                "term",
                &word,
                format!("\"{word}\" isn't anywhere in your resume."),
            );
        }
    }

    /// Fine unless it's a name-like word (has a capital) that isn't in your
    /// resume, the job's details, or the list of everyday words.
    fn is_known(&self, word: &str) -> bool {
        if !word.chars().any(char::is_uppercase) || word.chars().count() < 2 {
            return true;
        }
        let lower = word.to_lowercase();
        if COMMON_WORDS.contains(&lower.as_str())
            || self.resume_text.contains(&lower)
            || self.job_words.contains(&lower)
        {
            return true;
        }
        // "SQL-heavy", "Power BI-based": fine when every part is.
        word.contains('-')
            && word
                .split('-')
                .all(|part| part.is_empty() || self.is_known(part))
    }

    pub fn check_resume(&self, draft: &ResumeDraft) -> Vec<Flag> {
        let mut flags = Vec::new();
        self.check_line("summary", &draft.summary, &mut flags);
        for (section, entries) in [
            ("experience", &draft.experience),
            ("projects", &draft.projects),
        ] {
            for entry in entries {
                for (i, bullet) in entry.bullets.iter().enumerate() {
                    self.check_line(&format!("{section}.{}.{i}", entry.id), bullet, &mut flags);
                }
            }
        }
        dedupe(flags)
    }

    pub fn check_letter(&self, draft: &LetterDraft) -> Vec<Flag> {
        let mut flags = Vec::new();
        for (i, paragraph) in draft.paragraphs.iter().enumerate() {
            self.check_line(&format!("paragraphs.{i}"), paragraph, &mut flags);
        }
        let count = word_count(draft);
        if count > MAX_LETTER_WORDS {
            flags.push(Flag {
                key: "length|document|".into(),
                path: "document".into(),
                severity: Severity::Info,
                message: format!("{count} words. Aim for {MAX_LETTER_WORDS} or fewer."),
            });
        }
        dedupe(flags)
    }
}

/// Notes from assembling the draft (e.g. a skill code removed), as info flags.
pub fn note_flags(notes: &[String]) -> Vec<Flag> {
    notes
        .iter()
        .enumerate()
        .map(|(i, note)| Flag {
            key: format!("note|document|{i}"),
            path: "document".into(),
            severity: Severity::Info,
            message: note.clone(),
        })
        .collect()
}

fn dedupe(flags: Vec<Flag>) -> Vec<Flag> {
    let mut out: Vec<Flag> = Vec::new();
    for flag in flags {
        if !out.iter().any(|f| f.key == flag.key) {
            out.push(flag);
        }
    }
    out
}

/// "80% recall across 300M+ txns, 1:20 ratio, 2.5x" -> ["80", "300", "1:20", "2.5"].
fn numbers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || (matches!(chars[i], '.' | ',' | ':')
                        && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit())))
            {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

/// Word-like tokens, keeping characters used in tech names (C#, C++, Node.js, scikit-learn).
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| {
        !(c.is_alphanumeric() || matches!(c, '+' | '#' | '.' | '-' | '\'' | '’' | '/'))
    })
    .map(|w| w.trim_matches(|c: char| matches!(c, '.' | '-' | '\'' | '’' | '/')))
    .filter(|w| w.chars().any(char::is_alphabetic))
    .map(String::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> CheckContext {
        let master = MasterResume::parse(
            "# Jane\n\n## Skills\n\n* **Programming:** Python, SQL, PySpark\n\n## Experience\n\n### NPCI\n\n* Built a fraud model with 80% recall at a 1:20 ratio across 300M+ transactions.\n",
        );
        CheckContext::new(
            &master,
            "Data Analyst",
            Some("Atlassian"),
            Some("Sydney NSW"),
        )
    }

    fn line(text: &str, ids: &[&str]) -> Line {
        Line {
            text: text.into(),
            evidence: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn kinds(flags: &[Flag]) -> Vec<String> {
        flags
            .iter()
            .map(|f| f.key.split('|').next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_faithful_line_has_no_flags() {
        let mut flags = Vec::new();
        ctx().check_line(
            "x",
            &line(
                "Developed a PySpark fraud model with 80% recall at 1:20 across 300M+ transactions",
                &["W1.1"],
            ),
            &mut flags,
        );
        assert!(flags.is_empty(), "{flags:?}");
    }

    #[test]
    fn flags_missing_or_unknown_evidence() {
        let mut flags = Vec::new();
        ctx().check_line("x", &line("Built dashboards", &[]), &mut flags);
        ctx().check_line("y", &line("Built dashboards", &["W7.1"]), &mut flags);
        assert_eq!(kinds(&flags), vec!["no_evidence", "unknown_evidence"]);
    }

    #[test]
    fn flags_numbers_not_in_the_cited_evidence() {
        let mut flags = Vec::new();
        ctx().check_line(
            "x",
            &line("Improved recall by 35% with 80% recall", &["W1.1"]),
            &mut flags,
        );
        assert_eq!(kinds(&flags), vec!["number"]);
        assert!(flags[0].message.contains("\"35\""));
    }

    #[test]
    fn flags_names_that_arent_in_the_resume() {
        let mut flags = Vec::new();
        ctx().check_line(
            "x",
            &line(
                "I'd built SQL-heavy Snowflake and AWS pipelines with Python for Atlassian's team in Sydney",
                &["W1.1"],
            ),
            &mut flags,
        );
        assert_eq!(kinds(&flags), vec!["term", "term"]);
        assert!(flags[0].message.contains("Snowflake"));
        assert!(flags[1].message.contains("AWS"));
    }

    #[test]
    fn long_letters_get_an_info_flag() {
        let letter = LetterDraft {
            paragraphs: vec![line(&"word ".repeat(400), &["S"])],
            ..LetterDraft::default()
        };
        let flags = ctx().check_letter(&letter);
        let length = flags.iter().find(|f| f.key.starts_with("length")).unwrap();
        assert_eq!(length.severity, Severity::Info);
    }

    #[test]
    fn finds_numbers_and_words() {
        assert_eq!(
            numbers("80% of 300M+, 1:20 and 2.5x in 2024."),
            vec!["80", "300", "1:20", "2.5", "2024"]
        );
        assert_eq!(
            words("Used C#, Node.js and scikit-learn."),
            vec!["Used", "C#", "Node.js", "and", "scikit-learn"]
        );
    }
}
