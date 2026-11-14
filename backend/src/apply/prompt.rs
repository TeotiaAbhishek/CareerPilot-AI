//! The model's part: fields that your profile and answer bank don't cover,
//! like "How many years' experience do you have with SQL?" or "Why are you
//! interested in this role?".
//!
//! The model sees the job ad, your work rights, location and preferences,
//! your answer bank and your resume evidence. It never sees your name, email,
//! phone or address: code fills those in.

use serde::Deserialize;
use serde_json::{Value, json};

use super::browser::Field;
use super::fill::{KnownAnswer, match_option};
use crate::profile::{Facts, profile_for_prompt};

pub const SYSTEM: &str = "You fill in job application forms for one candidate. \
Answer each field using ONLY the job ad, the candidate's facts, answer bank and resume evidence below.

Rules:
- For select, radio and checkbox fields, the value must be exactly one of the listed options, copied character for character.
- If the information isn't there, answer null. Never guess, never invent experience, qualifications, licences or checks.
- Open questions about motivation or fit (\"Why are you interested in this role?\", \"Why should we hire you?\", \"Tell us about yourself\"): write 2 to 4 sentences in the first person. Link 2 or 3 real things from the resume evidence to what the job ad asks for. Name the company if it is known. No claims the evidence doesn't support, no flattery, no clichés.
- Answer null for questions only the candidate can decide: declarations, consent, criminal history, health or disability, referees, conflicts of interest.
- Years of experience \"as a <role>\" or \"in <area>\": use YEARS PER ROLE. Add up every role that did the same kind of work, even under a different title (data analyst work counts for \"data consultant\" or \"BI analyst\"). Pick the closest option that does not overstate. Only answer \"no experience\" when no listed role is related.
- Salary: use the candidate's salary expectation if given, otherwise null.
- Text answers are short and plain, in Australian English, with no em dashes.";

#[derive(Debug, Deserialize)]
struct ModelAnswers {
    answers: Vec<ModelAnswer>,
}

#[derive(Debug, Deserialize)]
struct ModelAnswer {
    id: String,
    value: Option<String>,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "answers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "value": { "type": ["string", "null"] }
                    },
                    "required": ["id", "value"]
                }
            }
        },
        "required": ["answers"]
    })
}

/// Your facts for the model: work rights, location, preferences, notice,
/// salary and start date. No contact details.
pub fn facts_for_prompt(f: &Facts) -> String {
    let or_unknown = |v: &str| {
        if v.trim().is_empty() {
            "unknown".to_string()
        } else {
            format!("{v:?}")
        }
    };
    format!(
        "{}\napplication:\n  notice_period: {}\n  salary_expectation: {}\n  available_from: {}\n",
        profile_for_prompt(f),
        or_unknown(&f.notice_period),
        or_unknown(&f.salary_expectation),
        f.available_from
            .map_or("unknown".to_string(), |d| format!("\"{d}\"")),
    )
}

/// How much of the job ad the model sees. Enough for the duties and
/// requirements, small enough to keep the call quick.
const DESCRIPTION_CHARS: usize = 4000;

pub struct PromptInput<'a> {
    pub job_title: &'a str,
    pub company: Option<&'a str>,
    /// The ad's plain text, if we have it.
    pub job_description: Option<&'a str>,
    pub facts: &'a Facts,
    pub answers: &'a [KnownAnswer],
    pub evidence: &'a str,
    pub fields: &'a [Field],
}

pub fn user_prompt(input: &PromptInput<'_>) -> String {
    let mut bank = String::new();
    for a in input.answers.iter().take(40) {
        bank.push_str(&format!("Q: {}\nA: {}\n", a.question, a.answer));
    }
    if bank.is_empty() {
        bank.push_str("(empty)\n");
    }
    let fields: Vec<Value> = input
        .fields
        .iter()
        .map(|f| {
            let mut v =
                json!({ "id": f.id, "kind": f.kind, "label": f.label, "required": f.required });
            if f.kind.is_choice() {
                v["options"] = json!(f.options);
            }
            v
        })
        .collect();
    let description: String = input
        .job_description
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .unwrap_or("(not available)")
        .chars()
        .take(DESCRIPTION_CHARS)
        .collect();
    format!(
        "JOB\nTitle: {}\nCompany: {}\nAd:\n{}\n\nCANDIDATE FACTS\n{}\nANSWER BANK\n{}\nRESUME EVIDENCE\n{}\n\
         FIELDS TO ANSWER (JSON)\n{}\n\nReturn one answer per field id.",
        input.job_title,
        input.company.unwrap_or("unknown"),
        description,
        facts_for_prompt(input.facts),
        bank,
        input.evidence.trim_end(),
        serde_json::to_string_pretty(&fields).unwrap_or_default(),
    )
}

/// True for an answer the model wrote in its own words (a sentence or more),
/// as opposed to a fact like "2 weeks". You should read those before Submit.
pub fn is_written(field: &Field, value: &str) -> bool {
    !field.kind.is_choice() && value.split_whitespace().count() >= 8
}

/// Keep only usable answers: a field we asked about, and for choices, one of
/// its options. Returns (field id, value) pairs.
pub fn usable_answers(raw: Value, fields: &[Field]) -> Result<Vec<(String, String)>, String> {
    let parsed: ModelAnswers =
        serde_json::from_value(raw).map_err(|e| format!("doesn't match the schema: {e}"))?;
    let mut out = Vec::new();
    for answer in parsed.answers {
        let Some(field) = fields.iter().find(|f| f.id == answer.id) else {
            continue;
        };
        let Some(value) = answer
            .value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        if out.iter().any(|(id, _): &(String, String)| id == &field.id) {
            continue;
        }
        let value = if field.kind.is_choice() {
            match match_option(&value, &field.options) {
                Some(v) => v,
                None => continue,
            }
        } else {
            value
                .chars()
                .take(2000)
                .collect::<String>()
                .replace(['—', '–'], ", ")
        };
        out.push((field.id.clone(), value));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::browser::FieldKind;

    fn fields() -> Vec<Field> {
        vec![
            Field {
                id: "f1".into(),
                kind: FieldKind::Select,
                label: "Years with SQL".into(),
                required: true,
                options: vec!["1 year".into(), "2 years".into(), "3 years".into()],
                value: String::new(),
            },
            Field {
                id: "f2".into(),
                kind: FieldKind::Text,
                label: "Why this role?".into(),
                required: true,
                options: vec![],
                value: String::new(),
            },
        ]
    }

    #[test]
    fn prompt_has_no_contact_details() {
        let facts = Facts {
            full_name: "Test Person".into(),
            email: "test@example.com".into(),
            phone: "0400 000 000".into(),
            street_address: "1 Test St".into(),
            australian_work_rights: true,
            notice_period: "2 weeks".into(),
            ..Facts::default()
        };
        let fields = fields();
        let prompt = user_prompt(&PromptInput {
            job_title: "Data Analyst",
            company: Some("Acme"),
            job_description: Some("We need strong SQL."),
            facts: &facts,
            answers: &[],
            evidence: "[W1.1] Built SQL pipelines",
            fields: &fields,
        });
        assert!(prompt.contains("australian_work_rights: true"));
        assert!(prompt.contains("notice_period: \"2 weeks\""));
        assert!(prompt.contains("\"options\""));
        assert!(prompt.contains("We need strong SQL."));
        for secret in ["Test Person", "test@example.com", "0400", "1 Test St"] {
            assert!(!prompt.contains(secret), "{secret} leaked into the prompt");
        }
    }

    #[test]
    fn only_valid_answers_are_kept() {
        let raw = json!({ "answers": [
            { "id": "f1", "value": "two years" },
            { "id": "f1", "value": "2 years" },
            { "id": "f2", "value": null },
            { "id": "f9", "value": "made up field" }
        ]});
        assert_eq!(
            usable_answers(raw, &fields()).unwrap(),
            vec![("f1".to_string(), "2 years".to_string())]
        );
        assert!(usable_answers(json!({ "nope": 1 }), &fields()).is_err());
    }

    #[test]
    fn written_answers_are_sentences_not_facts() {
        let fields = fields();
        let why = "I enjoy turning messy data into clear reports, and your team does exactly that.";
        assert!(is_written(&fields[1], why));
        assert!(!is_written(&fields[1], "2 weeks"));
        assert!(!is_written(&fields[0], why));
    }
}
