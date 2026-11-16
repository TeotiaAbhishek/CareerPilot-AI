//! Deciding what goes in each field, without the model where possible.
//!
//! In order: your documents (resume and cover letter uploads), your profile
//! facts (name, email, phone, address...), then the answer bank. Whatever is
//! left goes to the model (see `prompt.rs`), and what the model can't answer
//! comes to you as a question.

use chrono::NaiveDate;
use uuid::Uuid;

use super::browser::{Field, FieldKind};
use crate::profile::Facts;

/// Which approved document a file field gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doc {
    Resume,
    CoverLetter,
}

/// Where a value came from, for the run log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Profile(&'static str),
    AnswerBank(Option<Uuid>),
    DocumentChoice,
    /// Years of experience, counted from the dates on your resume.
    ResumeDates,
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Source::Profile(what) => format!("profile: {what}"),
            Source::AnswerBank(Some(_)) => "answer bank".into(),
            Source::AnswerBank(None) => "your answer".into(),
            Source::DocumentChoice => "approved documents".into(),
            Source::ResumeDates => "counted from your resume dates".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Fill {
        field_id: String,
        label: String,
        value: String,
        source: Source,
    },
    Upload {
        field_id: String,
        label: String,
        doc: Doc,
    },
}

#[derive(Debug, Default)]
pub struct Plan {
    pub actions: Vec<Action>,
    /// Empty fields nothing above could answer: next stop, the model.
    pub unknown: Vec<Field>,
    /// A required cover-letter upload, but you have no approved cover letter.
    pub needs_cover_letter: bool,
    /// The form takes a cover letter but doesn't need one, and you have none
    /// approved for this job. The runner asks you before leaving it out.
    pub cover_letter_optional: bool,
}

/// An answer the agent can reuse: from the answer bank (with its id), or one
/// you gave for this application (no id).
#[derive(Debug, Clone)]
pub struct KnownAnswer {
    pub id: Option<Uuid>,
    pub question: String,
    pub answer: String,
}

pub struct FillContext<'a> {
    pub facts: &'a Facts,
    pub answers: &'a [KnownAnswer],
    pub has_cover_letter: bool,
    /// The file names the agent uploads, e.g. "Test Person - Resume - Acme.docx".
    pub resume_file: &'a str,
    pub cover_letter_file: Option<&'a str>,
    /// Documents already uploaded in this run. Sites like SEEK reset their
    /// upload box afterwards, so this stops the same file going up twice.
    pub uploaded: &'a [Doc],
}

/// Plan the empty fields on one page. Fields that already have a value (the
/// site filled them in, or an earlier step did) are left alone.
pub fn plan(fields: &[Field], ctx: &FillContext<'_>) -> Plan {
    let mut plan = Plan::default();

    // SEEK lists the resumés saved on your profile as options, with an
    // Upload button beside them. Once ours is in that list, pick it. Only
    // after uploading it in this run, though: every CareerPilot resumé has
    // the same name, so an older copy in the list may be another job's.
    let mut listed: Vec<Doc> = Vec::new();
    let mut handled: Vec<&str> = Vec::new();
    for (doc, file) in [
        (Doc::Resume, Some(ctx.resume_file)),
        (Doc::CoverLetter, ctx.cover_letter_file),
    ] {
        let Some(file) = file else { continue };
        if !ctx.uploaded.contains(&doc) {
            continue;
        }
        for field in fields.iter().filter(|f| f.kind.is_choice()) {
            let ours: Vec<&String> = field
                .options
                .iter()
                .filter(|o| names_file(o, file))
                .collect();
            // The copy the site marks as just added, else the first (newest).
            let pick = ours.iter().find(|o| is_fresh(o)).or(ours.first()).copied();
            if let Some(option) = pick {
                listed.push(doc);
                handled.push(&field.id);
                if &field.value != option {
                    plan.actions
                        .push(fill(field, option.clone(), Source::DocumentChoice));
                }
            }
        }
    }

    for field in fields {
        let label = normalise(&field.label);
        if handled.contains(&field.id.as_str()) {
            continue;
        }

        // "Resumé: select / upload / don't include". Job boards often pick
        // the resumé you uploaded there last time for you, so this one is
        // checked even when it already has a value: the agent always sends
        // the resumé you approved for this job.
        if let Some(value) = document_choice(field, &label, ctx.has_cover_letter) {
            if !ctx.has_cover_letter && file_doc(&normalise(&value)) == Some(Doc::CoverLetter) {
                plan.cover_letter_optional = true;
            }
            if field.value != value {
                plan.actions
                    .push(fill(field, value, Source::DocumentChoice));
            }
            continue;
        }

        // Everything else: only fields that are still empty.
        if !field.is_empty() {
            continue;
        }

        if field.kind == FieldKind::File {
            // Sites like SEEK have both a hidden file box and an Upload button
            // for the same document: one upload each.
            let planned = |doc: Doc| {
                plan.actions
                    .iter()
                    .any(|a| matches!(a, Action::Upload { doc: d, .. } if *d == doc))
            };
            let done =
                |doc: Doc| listed.contains(&doc) || ctx.uploaded.contains(&doc) || planned(doc);
            match file_doc(&label) {
                Some(Doc::Resume) if done(Doc::Resume) => {}
                Some(Doc::Resume) => plan.actions.push(upload(field, Doc::Resume)),
                Some(Doc::CoverLetter) if done(Doc::CoverLetter) => {}
                Some(Doc::CoverLetter) if ctx.has_cover_letter => {
                    plan.actions.push(upload(field, Doc::CoverLetter));
                }
                Some(Doc::CoverLetter) if field.required => plan.needs_cover_letter = true,
                Some(Doc::CoverLetter) => plan.cover_letter_optional = true,
                None => plan.unknown.push(field.clone()),
            }
            continue;
        }

        if let Some((what, value)) = profile_value(field, &label, ctx.facts) {
            let value = if field.kind.is_choice() {
                match_option(&value, &field.options)
            } else {
                Some(value)
            };
            if let Some(value) = value {
                plan.actions.push(fill(field, value, Source::Profile(what)));
                continue;
            }
        }

        if let Some(known) = find_answer(&label, ctx.answers) {
            let value = if field.kind.is_choice() {
                match_option(&known.answer, &field.options)
            } else {
                Some(known.answer.clone())
            };
            if let Some(value) = value {
                plan.actions
                    .push(fill(field, value, Source::AnswerBank(known.id)));
                continue;
            }
        }

        plan.unknown.push(field.clone());
    }
    plan
}

fn fill(field: &Field, value: String, source: Source) -> Action {
    Action::Fill {
        field_id: field.id.clone(),
        label: field.label.clone(),
        value,
        source,
    }
}

fn upload(field: &Field, doc: Doc) -> Action {
    Action::Upload {
        field_id: field.id.clone(),
        label: field.label.clone(),
        doc,
    }
}

/// Lower case, accents on "résumé" removed, punctuation to spaces.
pub fn normalise(text: &str) -> String {
    let text = text
        .to_lowercase()
        .replace(['é', 'è'], "e")
        .replace("(required)", " ");
    let mapped: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// True when an option names our file ("Test Person - Resume - Acme.docx",
/// maybe with "Added just now" after it, or saved as a PDF).
fn names_file(option: &str, file: &str) -> bool {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let stem = normalise(stem);
    !stem.is_empty() && normalise(option).contains(&stem)
}

/// "Added just now", "Uploaded today", "1 minute ago".
fn is_fresh(option: &str) -> bool {
    let o = normalise(option);
    ["just now", "today", "second", "minute"]
        .iter()
        .any(|w| o.contains(w))
}

fn has_any(label: &str, phrases: &[&str]) -> bool {
    let padded = format!(" {label} ");
    phrases.iter().any(|p| padded.contains(&format!(" {p} ")))
}

fn file_doc(label: &str) -> Option<Doc> {
    if has_any(label, &["cover letter", "covering letter", "cover"]) {
        Some(Doc::CoverLetter)
    } else if has_any(label, &["resume", "cv", "curriculum vitae"]) {
        Some(Doc::Resume)
    } else {
        None
    }
}

/// Job boards often ask "Resumé: upload / select / don't include" first.
fn document_choice(field: &Field, label: &str, has_cover_letter: bool) -> Option<String> {
    if field.kind != FieldKind::Radio && field.kind != FieldKind::Select {
        return None;
    }
    let find = |words: &[&str]| {
        field
            .options
            .iter()
            .find(|o| has_any(&normalise(o), words))
            .cloned()
    };
    let upload = find(&["upload"])?;
    // The question's own label can be missing or vague ("Documents"), so the
    // options decide too: "Upload a resumé" is about the resumé.
    let doc = file_doc(label).or_else(|| file_doc(&normalise(&upload)));
    match doc {
        Some(Doc::Resume) => Some(upload),
        Some(Doc::CoverLetter) if has_cover_letter => Some(upload),
        Some(Doc::CoverLetter) => {
            find(&["don t include", "do not include", "no cover letter", "skip"])
        }
        None => None,
    }
}

fn first_name(full: &str) -> String {
    full.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn last_name(full: &str) -> String {
    let parts: Vec<&str> = full.split_whitespace().collect();
    if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    }
}

fn date_value(kind: &FieldKind, date: Option<NaiveDate>) -> String {
    match (kind, date) {
        (FieldKind::Date, Some(d)) => d.format("%Y-%m-%d").to_string(),
        (_, Some(d)) => d.format("%-d %B %Y").to_string(),
        (_, None) => String::new(),
    }
}

/// A value straight from your profile, if the label clearly asks for one.
fn profile_value(field: &Field, label: &str, f: &Facts) -> Option<(&'static str, String)> {
    let words = label.split(' ').count();
    let pick: Option<(&'static str, String)> = if has_any(
        label,
        &["first name", "given name", "given names", "firstname"],
    ) {
        Some(("first name", first_name(&f.full_name)))
    } else if has_any(label, &["last name", "surname", "family name", "lastname"]) {
        Some(("last name", last_name(&f.full_name)))
    } else if has_any(label, &["full name", "your name"]) || label == "name" {
        Some(("name", f.full_name.clone()))
    } else if field.kind == FieldKind::Email
        || has_any(label, &["email", "e mail", "email address"])
    {
        Some(("email", f.email.clone()))
    } else if field.kind == FieldKind::Tel
        || has_any(
            label,
            &[
                "phone",
                "mobile",
                "telephone",
                "contact number",
                "phone number",
            ],
        )
    {
        Some(("phone", f.phone.clone()))
    } else if has_any(label, &["linkedin"]) {
        Some(("LinkedIn", f.linkedin_url.clone()))
    } else if has_any(label, &["website", "portfolio", "personal site"]) {
        Some(("website", f.website_url.clone()))
    } else if has_any(
        label,
        &["postcode", "post code", "postal code", "zip", "zip code"],
    ) {
        Some(("postcode", f.postcode.clone()))
    } else if has_any(label, &["street address", "address line 1", "street"])
        || (label == "address" || label == "home address")
    {
        Some(("address", f.street_address.clone()))
    } else if has_any(label, &["city", "suburb", "town"]) && words <= 4 {
        Some(("city", f.city.clone()))
    } else if has_any(label, &["state", "territory"]) && words <= 4 {
        Some(("state", f.state.clone()))
    } else if has_any(label, &["country"]) && words <= 4 {
        Some(("country", f.country.clone()))
    } else if has_any(label, &["notice period"]) {
        Some(("notice period", f.notice_period.clone()))
    } else if has_any(label, &["salary", "remuneration"])
        && has_any(
            label,
            &["expected", "expectation", "expectations", "expecting"],
        )
    {
        Some(("salary expectation", f.salary_expectation.clone()))
    } else if has_any(
        label,
        &[
            "start date",
            "available from",
            "earliest start",
            "availability date",
        ],
    ) {
        Some(("available from", date_value(&field.kind, f.available_from)))
    } else {
        None
    };
    pick.filter(|(_, v)| !v.trim().is_empty())
}

const STOP_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "do",
    "does",
    "you",
    "your",
    "have",
    "has",
    "of",
    "to",
    "in",
    "for",
    "is",
    "are",
    "with",
    "what",
    "which",
    "how",
    "any",
    "please",
    "currently",
    "this",
    "role",
    "we",
    "if",
    "or",
    "and",
    "be",
    "can",
    "will",
    "would",
];

fn content_words(label: &str) -> Vec<&str> {
    let mut words: Vec<&str> = label
        .split(' ')
        .filter(|w| !w.is_empty() && !STOP_WORDS.contains(w))
        .collect();
    words.sort_unstable();
    words.dedup();
    words
}

/// The closest saved answer to this question: the same words (ignoring
/// little words like "do you"), or nearly the same.
pub fn find_answer<'a>(label: &str, answers: &'a [KnownAnswer]) -> Option<&'a KnownAnswer> {
    let words = content_words(label);
    if words.is_empty() {
        return None;
    }
    let mut best: Option<(f64, &KnownAnswer)> = None;
    for known in answers {
        let question = normalise(&known.question);
        if question == label {
            return Some(known);
        }
        let other = content_words(&question);
        let shared = words.iter().filter(|w| other.contains(w)).count();
        let union = words.len() + other.len() - shared;
        let score = shared as f64 / union.max(1) as f64;
        if score >= 0.75 && best.is_none_or(|(b, _)| score > b) {
            best = Some((score, known));
        }
    }
    best.map(|(_, known)| known)
}

/// The option that means `value`: the same words, or "Yes"/"No" phrased
/// differently ("yes" -> "Yes, I do").
pub fn match_option(value: &str, options: &[String]) -> Option<String> {
    let wanted = normalise(value);
    if wanted.is_empty() {
        return None;
    }
    if let Some(exact) = options.iter().find(|o| normalise(o) == wanted) {
        return Some(exact.clone());
    }
    for (short, words) in [("yes", ["yes", "true"]), ("no", ["no", "false"])] {
        if words.contains(&wanted.as_str()) || wanted.starts_with(&format!("{short} ")) {
            return options
                .iter()
                .find(|o| normalise(o) == short || normalise(o).starts_with(&format!("{short} ")))
                .cloned();
        }
    }
    let containing: Vec<&String> = options
        .iter()
        .filter(|o| {
            let o = normalise(o);
            // Whole words only, and nothing too short to mean anything.
            o.len() >= 3
                && wanted.len() >= 3
                && (has_any(&o, &[wanted.as_str()]) || has_any(&wanted, &[o.as_str()]))
        })
        .collect();
    match containing.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: &str, kind: FieldKind, label: &str, options: &[&str]) -> Field {
        Field {
            id: id.into(),
            kind,
            label: label.into(),
            required: true,
            options: options.iter().map(|o| o.to_string()).collect(),
            value: String::new(),
        }
    }

    fn facts() -> Facts {
        Facts {
            full_name: "Test Middle Person".into(),
            email: "test@example.com".into(),
            phone: "0400 000 000".into(),
            city: "Sydney".into(),
            state: "NSW".into(),
            country: "Australia".into(),
            notice_period: "2 weeks".into(),
            available_from: NaiveDate::from_ymd_opt(2026, 12, 1),
            ..Facts::default()
        }
    }

    fn value_of(plan: &Plan, id: &str) -> Option<String> {
        plan.actions.iter().find_map(|a| match a {
            Action::Fill {
                field_id, value, ..
            } if field_id == id => Some(value.clone()),
            _ => None,
        })
    }

    #[test]
    fn profile_fields_come_from_your_facts() {
        let fields = vec![
            field("f1", FieldKind::Text, "First name *", &[]),
            field("f2", FieldKind::Text, "Last name *", &[]),
            field("f3", FieldKind::Email, "Email address *", &[]),
            field("f4", FieldKind::Tel, "Mobile number", &[]),
            field("f5", FieldKind::Text, "Suburb", &[]),
            field("f6", FieldKind::Select, "State", &["ACT", "NSW", "VIC"]),
            field("f7", FieldKind::Text, "What is your notice period?", &[]),
            field("f8", FieldKind::Date, "Earliest start date", &[]),
            field("f9", FieldKind::Url, "LinkedIn profile", &[]),
        ];
        let facts = facts();
        let ctx = FillContext {
            facts: &facts,
            answers: &[],
            has_cover_letter: false,
            resume_file: "Test Person - Resume - Acme.docx",
            cover_letter_file: None,
            uploaded: &[],
        };
        let plan = plan(&fields, &ctx);
        assert_eq!(value_of(&plan, "f1").as_deref(), Some("Test"));
        assert_eq!(value_of(&plan, "f2").as_deref(), Some("Middle Person"));
        assert_eq!(value_of(&plan, "f3").as_deref(), Some("test@example.com"));
        assert_eq!(value_of(&plan, "f4").as_deref(), Some("0400 000 000"));
        assert_eq!(value_of(&plan, "f5").as_deref(), Some("Sydney"));
        assert_eq!(value_of(&plan, "f6").as_deref(), Some("NSW"));
        assert_eq!(value_of(&plan, "f7").as_deref(), Some("2 weeks"));
        assert_eq!(value_of(&plan, "f8").as_deref(), Some("2026-12-01"));
        // No LinkedIn in the profile: that one is left for later stages.
        assert_eq!(
            plan.unknown
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            ["f9"]
        );
    }

    #[test]
    fn filled_fields_are_left_alone() {
        let mut first = field("f1", FieldKind::Text, "First name", &[]);
        first.value = "Already".into();
        let facts = facts();
        let ctx = FillContext {
            facts: &facts,
            answers: &[],
            has_cover_letter: false,
            resume_file: "Test Person - Resume - Acme.docx",
            cover_letter_file: None,
            uploaded: &[],
        };
        let plan = plan(&[first], &ctx);
        assert!(plan.actions.is_empty() && plan.unknown.is_empty());
    }

    #[test]
    fn documents_go_in_the_right_upload_boxes() {
        let fields = vec![
            field(
                "f1",
                FieldKind::Radio,
                "Resumé",
                &["Upload a resumé", "Don't include a resumé"],
            ),
            field(
                "f2",
                FieldKind::Radio,
                "Cover letter",
                &[
                    "Upload a cover letter",
                    "Write a cover letter",
                    "Don't include a cover letter",
                ],
            ),
            field("f3", FieldKind::File, "Upload your CV *", &[]),
            field("f4", FieldKind::File, "Cover letter file", &[]),
        ];
        let facts = facts();
        let without = FillContext {
            facts: &facts,
            answers: &[],
            has_cover_letter: false,
            resume_file: "Test Person - Resume - Acme.docx",
            cover_letter_file: None,
            uploaded: &[],
        };
        let plan_without = plan(&fields, &without);
        assert_eq!(
            value_of(&plan_without, "f1").as_deref(),
            Some("Upload a resumé")
        );
        assert_eq!(
            value_of(&plan_without, "f2").as_deref(),
            Some("Don't include a cover letter")
        );
        assert!(plan_without.actions.contains(&Action::Upload {
            field_id: "f3".into(),
            label: "Upload your CV *".into(),
            doc: Doc::Resume
        }));
        assert!(plan_without.needs_cover_letter);

        let with = FillContext {
            facts: &facts,
            answers: &[],
            has_cover_letter: true,
            resume_file: "Test Person - Resume - Acme.docx",
            cover_letter_file: Some("Test Person - Cover letter - Acme.docx"),
            uploaded: &[],
        };
        let plan_with = plan(&fields, &with);
        assert_eq!(
            value_of(&plan_with, "f2").as_deref(),
            Some("Upload a cover letter")
        );
        assert!(!plan_with.needs_cover_letter);
        assert_eq!(plan_with.actions.len(), 4);
    }

    #[test]
    fn a_resume_the_site_preselected_is_swapped_for_yours() {
        let options = &[
            "Select a resumé",
            "Upload a resumé",
            "Don't include a resumé",
        ];
        let mut resume = field("f1", FieldKind::Radio, "Resumé", options);
        resume.value = "Select a resumé".into();
        let mut saved = field(
            "f2",
            FieldKind::Select,
            "Saved resumé",
            &["Old CV 2024.pdf"],
        );
        saved.value = "Old CV 2024.pdf".into();
        let facts = facts();
        let ctx = FillContext {
            facts: &facts,
            answers: &[],
            has_cover_letter: false,
            resume_file: "Test Person - Resume - Acme.docx",
            cover_letter_file: None,
            uploaded: &[],
        };
        let plan1 = plan(&[resume.clone(), saved], &ctx);
        assert_eq!(value_of(&plan1, "f1").as_deref(), Some("Upload a resumé"));
        assert!(plan1.unknown.is_empty(), "the old list is left alone");

        // No useful label: the options still say it's the resumé.
        let mut unlabelled = resume.clone();
        unlabelled.label = String::new();
        let plan2 = plan(&[unlabelled], &ctx);
        assert_eq!(value_of(&plan2, "f1").as_deref(), Some("Upload a resumé"));

        // Already on "Upload": nothing to change.
        resume.value = "Upload a resumé".into();
        assert!(plan(&[resume], &ctx).actions.is_empty());
    }

    #[test]
    fn seek_style_saved_resumes_get_an_upload_then_ours_is_picked() {
        // SEEK: the saved resumés are the options, plus an Upload button
        // (which the worker reports as a file field labelled "Resumé").
        // An older CareerPilot copy with the very same name, from another job.
        let old = "Test Person - Resume.pdf Added 20 days ago";
        let mut resume = field(
            "f1",
            FieldKind::Radio,
            "Resumé",
            &[old, "Don't include a resumé"],
        );
        resume.value = old.into();
        let upload_button = field("f2", FieldKind::File, "Resumé", &[]);
        let facts = facts();
        let mut ctx = FillContext {
            facts: &facts,
            answers: &[],
            has_cover_letter: false,
            resume_file: "Test Person - Resume.pdf",
            cover_letter_file: None,
            uploaded: &[],
        };

        // 1. Not uploaded in this run yet: upload it, even though an old copy
        //    with the same name is listed (and picked) already.
        let first = plan(&[resume.clone(), upload_button.clone()], &ctx);
        assert_eq!(
            first.actions,
            vec![Action::Upload {
                field_id: "f2".into(),
                label: "Resumé".into(),
                doc: Doc::Resume
            }]
        );
        assert!(first.unknown.is_empty());

        // 2. After the upload it's listed: pick the fresh copy, not the old
        //    one, and don't upload again.
        let ours = "Test Person - Resume.pdf Added just now";
        resume.options.push(ours.into());
        ctx.uploaded = &[Doc::Resume];
        let second = plan(&[resume.clone(), upload_button.clone()], &ctx);
        assert_eq!(value_of(&second, "f1").as_deref(), Some(ours));
        assert_eq!(second.actions.len(), 1);

        // SEEK's cover letter Upload button, with no letter approved: not
        // uploaded, and flagged so the runner asks you first.
        let mut letter_button = field("f3", FieldKind::File, "Cover letter", &[]);
        letter_button.required = false;
        let with_letter = plan(&[letter_button], &ctx);
        assert!(with_letter.actions.is_empty());
        assert!(with_letter.cover_letter_optional);
        assert!(!with_letter.needs_cover_letter);

        // 3. A site that renames the file: still only one upload per run.
        resume.options.pop();
        assert!(plan(&[resume, upload_button], &ctx).actions.is_empty());
    }

    #[test]
    fn answer_bank_matches_the_same_question_worded_differently() {
        let answers = vec![
            KnownAnswer {
                id: Some(Uuid::nil()),
                question: "Do you have a driver's licence?".into(),
                answer: "Yes".into(),
            },
            KnownAnswer {
                id: None,
                question: "Working With Children Check".into(),
                answer: "No".into(),
            },
        ];
        let fields = vec![
            field(
                "f1",
                FieldKind::Radio,
                "Do you have a current driver's licence?",
                &["Yes", "No"],
            ),
            field(
                "f2",
                FieldKind::Radio,
                "Do you hold a current Working With Children Check?",
                &["Yes", "No"],
            ),
            field(
                "f3",
                FieldKind::Radio,
                "Are you willing to travel interstate?",
                &["Yes", "No"],
            ),
        ];
        let facts = facts();
        let ctx = FillContext {
            facts: &facts,
            answers: &answers,
            has_cover_letter: false,
            resume_file: "Test Person - Resume - Acme.docx",
            cover_letter_file: None,
            uploaded: &[],
        };
        let plan = plan(&fields, &ctx);
        assert_eq!(value_of(&plan, "f1").as_deref(), Some("Yes"));
        // "hold a current" isn't in the saved question, so it's not close enough.
        assert_eq!(
            plan.unknown
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            ["f2", "f3"]
        );
    }

    #[test]
    fn options_match_loosely_but_never_guess() {
        let opts: Vec<String> = ["Yes, I do", "No, I don't"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(match_option("yes", &opts).as_deref(), Some("Yes, I do"));
        assert_eq!(match_option("No", &opts).as_deref(), Some("No, I don't"));
        let states: Vec<String> = ["New South Wales", "Victoria"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            match_option("new south wales", &states).as_deref(),
            Some("New South Wales")
        );
        assert_eq!(match_option("NSW", &states), None);
        let years: Vec<String> = ["1 year", "2 years", "More than 5 years"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(match_option("2", &years), None);
    }

    #[test]
    fn normalise_handles_accents_and_stars() {
        assert_eq!(normalise("Resumé file *"), "resume file");
        assert_eq!(normalise("E-mail (required)"), "e mail");
    }
}
