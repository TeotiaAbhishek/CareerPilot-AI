use chrono::{DateTime, Duration, Utc};

/// Turn SEEK's relative label ("3d ago", "5h ago", "30+d ago") into an
/// approximate timestamp, counted back from `seen_at` (when we saw the label).
pub fn listed_at_from_label(label: &str, seen_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let label = label
        .trim()
        .trim_start_matches("Posted")
        .trim()
        .to_lowercase();
    let label = label.trim_end_matches("ago").trim().replace('+', "");

    if label == "just now" || label == "today" {
        return Some(seen_at);
    }

    let unit_start = label.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = label.split_at(unit_start);
    let n: i64 = number.parse().ok()?;

    let age = match unit.trim() {
        "m" | "min" | "mins" => Duration::minutes(n),
        "h" | "hr" | "hrs" => Duration::hours(n),
        "d" | "day" | "days" => Duration::days(n),
        _ => return None,
    };

    Some(seen_at - age)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap()
    }

    #[test]
    fn parses_seek_labels() {
        assert_eq!(
            listed_at_from_label("3d ago", at()),
            Some(at() - Duration::days(3))
        );
        assert_eq!(
            listed_at_from_label("Posted 5h ago", at()),
            Some(at() - Duration::hours(5))
        );
        assert_eq!(
            listed_at_from_label("45m ago", at()),
            Some(at() - Duration::minutes(45))
        );
        assert_eq!(
            listed_at_from_label("30+d ago", at()),
            Some(at() - Duration::days(30))
        );
    }

    #[test]
    fn unknown_labels_give_none() {
        assert_eq!(listed_at_from_label("yesterday-ish", at()), None);
        assert_eq!(listed_at_from_label("", at()), None);
    }
}
