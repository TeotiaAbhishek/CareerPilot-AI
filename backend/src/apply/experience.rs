//! Years of experience, worked out in code from the dates on your resume.
//!
//! Small local models are bad at date maths: given "Sep 2022 – Sep 2023" and
//! "How many years' experience do you have as a Data Consultant?", qwen3 often
//! says "No experience". So we count the months here and hand the model the
//! numbers. It still decides which roles count for questions about a tool
//! or another kind of role.
//!
//! One question code answers by itself: years of experience as the role
//! you're applying for ("...as a Data Consultant?" on a Data Consultant job).
//! You only apply to jobs scored as a match for your experience, so all of
//! your work experience counts, and the model doesn't get to say "No
//! experience" because the job title differs from yours.

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};

use super::browser::Field;
use super::fill::normalise;
use crate::documents::resume::{MasterResume, entry_line};

/// A month as a single number: year * 12 + (month - 1).
type Month = i32;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// "Sep 2022", "September 2022", "09/2022", "2022", "Present".
fn parse_month(text: &str, today: Month) -> Option<Month> {
    let t = text
        .trim()
        .trim_matches(|c: char| c == '(' || c == ')' || c == ',');
    let lower = t.to_lowercase();
    if ["present", "current", "now", "today", "ongoing"].contains(&lower.as_str()) {
        return Some(today);
    }
    let year = |s: &str| -> Option<i32> {
        let y: i32 = s.parse().ok()?;
        (1950..=2100).contains(&y).then_some(y)
    };
    let parts: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || c == '/' || c == '.')
        .filter(|p| !p.is_empty())
        .collect();
    match parts.as_slice() {
        [y] => year(y).map(|y| y * 12),
        [m, y] => {
            let y = year(y)?;
            let m = if let Ok(n) = m.parse::<i32>() {
                (1..=12).contains(&n).then_some(n - 1)?
            } else {
                let name: String = m.chars().take(3).collect();
                MONTHS.iter().position(|x| *x == name)? as i32
            };
            Some(y * 12 + m)
        }
        _ => None,
    }
}

/// The first "start – end" range in a line, as inclusive months. The line
/// can hold other parts split by "|", like "Data Analyst | Sep 2022 – Now".
fn parse_range(line: &str, today: Month) -> Option<(Month, Month)> {
    line.replace("**", "")
        .split('|')
        .find_map(|part| one_range(part, today))
}

fn one_range(part: &str, today: Month) -> Option<(Month, Month)> {
    for sep in ["–", "—", " - ", " to ", "-"] {
        if let Some((a, b)) = part.split_once(sep) {
            let start = parse_month(a, today)?;
            let end = parse_month(b, today)?;
            return (start <= end).then_some((start, end));
        }
    }
    None
}

fn years(months: usize) -> String {
    // Rounded down so we never overstate.
    format!("{:.1}", (months as f64 / 12.0 * 10.0).floor() / 10.0)
}

/// The months of every dated job, overlaps counted once.
fn worked_months(master: &MasterResume, now: Month) -> BTreeSet<Month> {
    master
        .experience
        .iter()
        .filter_map(|e| dates(e, now))
        .flat_map(|(start, end)| start..=end)
        .collect()
}

fn dates(entry: &crate::documents::resume::Entry, now: Month) -> Option<(Month, Month)> {
    std::iter::once(&entry.heading)
        .chain(&entry.details)
        .find_map(|d| parse_range(d, now))
}

fn month_of(today: NaiveDate) -> Month {
    today.year() * 12 + today.month0() as i32
}

/// Your total work experience in years (rounded down to one decimal), or
/// None when no job on the resume has dates.
pub fn total_years(master: &MasterResume, today: NaiveDate) -> Option<f64> {
    let months = worked_months(master, month_of(today)).len();
    (months > 0).then(|| (months as f64 / 12.0 * 10.0).floor() / 10.0)
}

/// Words in a job title that say nothing about the kind of work.
const TITLE_NOISE: [&str; 12] = [
    "senior",
    "junior",
    "lead",
    "graduate",
    "principal",
    "the",
    "and",
    "for",
    "with",
    "level",
    "entry",
    "mid",
];

/// "How many years' experience do you have as a Data Consultant?" on a Data
/// Consultant job, with options like "No experience", "1 year", "2 years":
/// the option for your total years, never more. None for anything else (a
/// tool, a different kind of role, a text box), which goes to the model.
pub fn years_answer(field: &Field, job_title: &str, total: f64) -> Option<String> {
    if !field.kind.is_choice() {
        return None;
    }
    let label = normalise(&field.label);
    if !(label.contains("experience") && label.contains("year")) {
        return None;
    }
    let title = normalise(job_title);
    let about_this_job = title
        .split_whitespace()
        .filter(|w| w.len() >= 3 && !TITLE_NOISE.contains(w))
        .any(|w| label.split_whitespace().any(|l| l == w));
    if !about_this_job {
        return None;
    }
    field
        .options
        .iter()
        .filter_map(|o| option_years(o).map(|y| (y, o)))
        .filter(|(y, _)| *y <= total)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, o)| o.clone())
}

/// The fewest years an option stands for: "No experience" is -1 (only when
/// nothing else fits), "Less than 1 year" 0, "2 years" or "2-3 years" or
/// "More than 2 years" 2. None for options that aren't about years.
fn option_years(option: &str) -> Option<f64> {
    let o = option.to_lowercase();
    if o.contains("no experience") || o.trim() == "none" {
        return Some(-1.0);
    }
    if o.contains("less than") || o.contains("under") {
        return Some(0.0);
    }
    if !o.contains("year") {
        return None;
    }
    o.split(|c: char| !c.is_ascii_digit() && c != '.')
        .find(|p| !p.is_empty())
        .and_then(|n| n.parse().ok())
}

/// One line per dated job, plus the total without counting overlaps twice.
/// Empty when no job on the resume has dates we can read.
pub fn experience_block(master: &MasterResume, today: NaiveDate) -> String {
    let now = month_of(today);
    let mut out = String::new();
    for entry in &master.experience {
        let Some((start, end)) = dates(entry, now) else {
            continue;
        };
        let months = (end - start + 1) as usize;
        out.push_str(&format!(
            "[{}] {}: {} years\n",
            entry.id,
            entry_line(entry),
            years(months)
        ));
    }
    if out.is_empty() {
        return out;
    }
    format!(
        "YEARS PER ROLE (worked out from the resume dates)\n{out}Total work experience: {} years\n",
        years(worked_months(master, now).len())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESUME: &str = "# Jane Citizen

## Experience

### Acme Bank
**Data Analyst** | Sep 2022 – Sep 2023
- Built SQL reports.

### Beta Corp
**BI Analyst** | Jan 2024 – Present
- Power BI dashboards.

### Side Gig
**Tutor** | 2023 - 2024
- Taught maths.

### No dates here
**Volunteer**
- Helped out.
";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()
    }

    #[test]
    fn months_and_ranges_parse() {
        let now = 2026 * 12;
        assert_eq!(parse_month("Sep 2022", now), Some(2022 * 12 + 8));
        assert_eq!(parse_month("September 2022", now), Some(2022 * 12 + 8));
        assert_eq!(parse_month("09/2022", now), Some(2022 * 12 + 8));
        assert_eq!(parse_month("Present", now), Some(now));
        assert_eq!(parse_month("Mumbai", now), None);
        let sep_2022_to_sep_2023 = Some((2022 * 12 + 8, 2023 * 12 + 8));
        assert_eq!(
            parse_range("Sep 2022 – Sep 2023", now),
            sep_2022_to_sep_2023
        );
        assert_eq!(
            parse_range("**Data Analyst** | Sep 2022 – Sep 2023", now),
            sep_2022_to_sep_2023
        );
        assert_eq!(parse_range("Mumbai, India", now), None);
    }

    #[test]
    fn each_dated_role_gets_its_years_and_overlaps_count_once() {
        let master = MasterResume::parse(RESUME);
        let block = experience_block(&master, today());
        assert!(block.contains("Data Analyst"), "{block}");
        assert!(block.contains(": 1.0 years"), "{block}");
        // Jan 2024 to Jan 2026 inclusive is 25 months.
        assert!(block.contains(": 2.0 years"), "{block}");
        assert!(!block.contains("Volunteer"), "{block}");
        // Sep 2022..Sep 2023 is 13 months and Jan 2024..Jan 2026 is 25. The
        // tutor job (Jan 2023..Jan 2024) only adds Oct to Dec 2023: 41 in all.
        assert!(
            block.contains("Total work experience: 3.4 years"),
            "{block}"
        );
    }

    fn choice(label: &str, options: &[&str]) -> Field {
        serde_json::from_value(serde_json::json!({
            "id": "f1", "kind": "radio", "label": label, "required": true,
            "options": options, "value": ""
        }))
        .unwrap()
    }

    const SEEK_YEARS: [&str; 6] = [
        "No experience",
        "Less than 1 year",
        "1 year",
        "2 years",
        "3 years",
        "More than 5 years",
    ];

    #[test]
    fn years_as_the_job_title_use_your_total() {
        let q = choice(
            "How many years' experience do you have as a Data Consultant?",
            &SEEK_YEARS,
        );
        assert_eq!(
            years_answer(&q, "Senior Data Consultant", 1.7).as_deref(),
            Some("1 year")
        );
        assert_eq!(
            years_answer(&q, "Data Consultant", 0.5).as_deref(),
            Some("Less than 1 year")
        );
        assert_eq!(
            years_answer(&q, "Data Consultant", 9.0).as_deref(),
            Some("More than 5 years")
        );
    }

    #[test]
    fn other_years_questions_go_to_the_model() {
        let sql = choice(
            "How many years' experience do you have with SQL?",
            &SEEK_YEARS,
        );
        assert_eq!(years_answer(&sql, "Data Consultant", 1.7), None);
        let nurse = choice(
            "How many years' experience do you have as a nurse?",
            &SEEK_YEARS,
        );
        assert_eq!(years_answer(&nurse, "Data Consultant", 1.7), None);
        let yes_no = choice(
            "Do you have experience as a Data Consultant?",
            &["Yes", "No"],
        );
        assert_eq!(years_answer(&yes_no, "Data Consultant", 1.7), None);
    }

    #[test]
    fn total_years_counts_overlaps_once() {
        let master = MasterResume::parse(RESUME);
        assert_eq!(total_years(&master, today()), Some(3.4));
    }

    #[test]
    fn no_dates_means_no_block() {
        let master = MasterResume::parse(
            "# Jane\n\n## Experience\n\n### Acme\n**Analyst**\n- Did things.\n",
        );
        assert_eq!(experience_block(&master, today()), "");
        assert_eq!(total_years(&master, today()), None);
    }
}
