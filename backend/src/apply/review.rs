//! The reviewer: a second model call that checks the first one's answers
//! before they go into the form.
//!
//! The answering call fills in what code couldn't. The reviewer reads the
//! same job ad, facts and resume evidence, and flags any answer that claims
//! something the evidence doesn't support, overstates, picks the wrong option
//! or doesn't answer the question. Flagged answers go back to the answering
//! call once, with the reviewer's note. Whatever is still flagged after that
//! comes to you as a question instead.

use serde::Deserialize;
use serde_json::{Value, json};

use super::browser::Field;

pub const SYSTEM: &str = "You review answers that an assistant wrote into a job application form for one candidate. \
Check each answer against ONLY the job ad, the candidate's facts, answer bank and resume evidence below.

Flag an answer (ok: false) when it:
- claims experience, skills, qualifications, licences or checks the evidence doesn't show, or overstates years;
- contradicts the candidate's facts (work rights, location, notice, salary);
- picks an option that doesn't match the evidence, or doesn't answer the question asked;
- answers a question only the candidate can decide (declarations, consent, criminal history, health, referees, conflicts of interest);
- for a written answer: is generic, names the wrong company or role, or makes claims the evidence doesn't support.

Don't flag style or wording you'd merely phrase differently. When you flag one, say in one short sentence what is wrong.";

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "reviews": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "ok": { "type": "boolean" },
                        "issue": { "type": ["string", "null"] }
                    },
                    "required": ["id", "ok", "issue"]
                }
            }
        },
        "required": ["reviews"]
    })
}

/// The reviewer's prompt: the shared context, then each field with its answer.
pub fn user_prompt(context: &str, fields: &[Field], answers: &[(String, String)]) -> String {
    let items: Vec<Value> = answers
        .iter()
        .filter_map(|(id, value)| {
            let field = fields.iter().find(|f| &f.id == id)?;
            let mut v = json!({ "id": id, "question": field.label, "answer": value });
            if field.kind.is_choice() {
                v["options"] = json!(field.options);
            }
            Some(v)
        })
        .collect();
    format!(
        "{context}\nANSWERS TO CHECK (JSON)\n{}\n\nReturn one review per answer id.",
        serde_json::to_string_pretty(&items).unwrap_or_default()
    )
}

#[derive(Deserialize)]
struct Reviews {
    reviews: Vec<Review>,
}

#[derive(Deserialize)]
struct Review {
    id: String,
    ok: bool,
    issue: Option<String>,
}

/// The answers the reviewer flagged: (field id, what's wrong). An answer the
/// reviewer skipped counts as fine.
pub fn flagged(raw: Value, answers: &[(String, String)]) -> Result<Vec<(String, String)>, String> {
    let parsed: Reviews =
        serde_json::from_value(raw).map_err(|e| format!("doesn't match the schema: {e}"))?;
    let mut out: Vec<(String, String)> = Vec::new();
    for review in parsed.reviews {
        if review.ok
            || !answers.iter().any(|(id, _)| id == &review.id)
            || out.iter().any(|(id, _)| id == &review.id)
        {
            continue;
        }
        let issue = review
            .issue
            .map(|i| i.trim().to_string())
            .filter(|i| !i.is_empty())
            .unwrap_or_else(|| "the reviewer didn't say why".into());
        out.push((review.id, issue));
    }
    Ok(out)
}

/// Extra lines for the answering call's second try: each flagged answer and
/// the reviewer's note.
pub fn revision_note(
    fields: &[Field],
    answers: &[(String, String)],
    issues: &[(String, String)],
) -> String {
    let mut note = String::from(
        "\n\nA reviewer checked your earlier answers to these fields and flagged them. Answer them again, fixing the problem. If the evidence doesn't support an answer, answer null.\n",
    );
    for (id, issue) in issues {
        let label = fields
            .iter()
            .find(|f| &f.id == id)
            .map_or("", |f| f.label.as_str());
        let before = answers
            .iter()
            .find(|(i, _)| i == id)
            .map_or("", |(_, v)| v.as_str());
        note.push_str(&format!(
            "- {id} \"{label}\": you answered {before:?}. Reviewer: {issue}\n"
        ));
    }
    note
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
                label: "Years with Snowflake".into(),
                required: true,
                options: vec!["None".into(), "1 year".into(), "3 years".into()],
                value: String::new(),
            },
            Field {
                id: "f2".into(),
                kind: FieldKind::Textarea,
                label: "Why this role?".into(),
                required: true,
                options: vec![],
                value: String::new(),
            },
        ]
    }

    #[test]
    fn only_flagged_answers_we_gave_come_back() {
        let answers = vec![
            ("f1".to_string(), "3 years".to_string()),
            ("f2".to_string(), "I love data.".to_string()),
        ];
        let raw = json!({ "reviews": [
            { "id": "f1", "ok": false, "issue": "The resume never mentions Snowflake." },
            { "id": "f2", "ok": true, "issue": null },
            { "id": "f9", "ok": false, "issue": "not ours" }
        ]});
        assert_eq!(
            flagged(raw, &answers).unwrap(),
            vec![(
                "f1".to_string(),
                "The resume never mentions Snowflake.".to_string()
            )]
        );
        assert!(flagged(json!({ "nope": [] }), &answers).is_err());
    }

    #[test]
    fn prompts_carry_the_answers_and_the_notes() {
        let answers = vec![("f1".to_string(), "3 years".to_string())];
        let prompt = user_prompt("JOB\nTitle: Data Analyst\n", &fields(), &answers);
        assert!(prompt.contains("Years with Snowflake"));
        assert!(prompt.contains("\"answer\": \"3 years\""));
        assert!(prompt.contains("\"options\""));

        let note = revision_note(
            &fields(),
            &answers,
            &[("f1".into(), "No Snowflake in the resume.".into())],
        );
        assert!(note.contains("you answered \"3 years\""));
        assert!(note.contains("No Snowflake in the resume."));
    }
}
