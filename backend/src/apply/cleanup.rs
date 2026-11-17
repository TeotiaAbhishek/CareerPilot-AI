//! SEEK keeps every resumé you upload, and caps how many you can keep. This
//! tidies up the ones CareerPilot uploaded ("Jane Citizen - Resume.pdf"):
//! it lists them, keeps the newest 3, and deletes the older ones you confirm.
//! Resumés you uploaded yourself, with other names, are never touched.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::{AppError, AppResult},
    profile,
};

/// SEEK's profile page, where saved resumés are listed.
pub const SEEK_PROFILE_URL: &str = "https://www.seek.com.au/profile/me";
/// How many of CareerPilot's resumés stay on SEEK.
pub const KEEP: usize = 3;

#[derive(Deserialize)]
pub struct ScanParams {
    /// Use the worker's practice profile page instead of SEEK.
    #[serde(default)]
    practice: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Listed {
    pub index: usize,
    pub name: String,
    pub added: String,
    /// Suggested for deletion (older than the newest 3). You confirm.
    pub delete: bool,
}

#[derive(Serialize)]
pub struct Scan {
    pub tab: String,
    pub resumes: Vec<Listed>,
}

/// File names CareerPilot gives resumés: "Jane Citizen - Resume.pdf", and
/// before F6b "Jane Citizen - Resume - Acme.docx".
fn prefixes(full_name: &str) -> Vec<String> {
    vec![format!("{} - Resume", full_name.trim())]
}

/// "Added just now" -> 0, "Added 3 days ago" -> 4320 (minutes). None when
/// it can't tell, and then it's never suggested for deletion.
pub fn age_minutes(added: &str) -> Option<i64> {
    let text = added.to_lowercase();
    if text.contains("just now") || text.contains("today") || text.contains("seconds") {
        return Some(0);
    }
    if text.contains("yesterday") {
        return Some(24 * 60);
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    for pair in words.windows(2) {
        let n: i64 = match pair[0] {
            "a" | "an" | "one" => 1,
            other => match other.parse() {
                Ok(n) => n,
                Err(_) => continue,
            },
        };
        let unit = pair[1].trim_end_matches(['s', ',', '.']);
        let minutes = match unit {
            "minute" | "min" => 1,
            "hour" | "hr" => 60,
            "day" => 24 * 60,
            "week" => 7 * 24 * 60,
            "month" => 30 * 24 * 60,
            "year" => 365 * 24 * 60,
            _ => continue,
        };
        return Some(n * minutes);
    }
    None
}

/// Keep the newest `KEEP`; suggest the rest. Unknown ages are kept.
pub fn suggest(found: Vec<(usize, String, String)>) -> Vec<Listed> {
    let mut known: Vec<(i64, usize)> = found
        .iter()
        .filter_map(|(i, _, added)| age_minutes(added).map(|age| (age, *i)))
        .collect();
    known.sort();
    let doomed: Vec<usize> = known.iter().skip(KEEP).map(|(_, i)| *i).collect();
    found
        .into_iter()
        .map(|(index, name, added)| Listed {
            delete: doomed.contains(&index),
            index,
            name,
            added,
        })
        .collect()
}

/// Open your SEEK profile in the agent's browser (in view) and list the
/// resumés CareerPilot uploaded.
pub async fn scan(
    State(state): State<AppState>,
    Query(params): Query<ScanParams>,
) -> AppResult<Json<Scan>> {
    let facts = profile::load_facts(&state.db).await?;
    if facts.full_name.trim().is_empty() {
        return Err(AppError::Conflict(
            "Add your name on the Profile page first: CareerPilot's resumés are named after it."
                .into(),
        ));
    }
    let browser = state.applier.browser();
    let url = if params.practice {
        // The practice page names its files after you.
        reqwest::Url::parse_with_params(
            &format!("{}/practice/seek-resumes", browser.base_url()),
            &[("name", facts.full_name.trim())],
        )
        .map_err(|e| AppError::Unavailable(e.to_string()))?
        .to_string()
    } else {
        SEEK_PROFILE_URL.to_string()
    };
    let unavailable = |e: crate::apply::browser::BrowserError| AppError::Unavailable(e.to_string());
    let tab = browser.open(&url, true).await.map_err(unavailable)?;
    let found = browser
        .saved_resumes(&tab, &prefixes(&facts.full_name))
        .await
        .map_err(unavailable)?;
    let resumes = suggest(
        found
            .into_iter()
            .map(|r| (r.index, r.name, r.added))
            .collect(),
    );
    Ok(Json(Scan { tab, resumes }))
}

#[derive(Deserialize)]
pub struct DeleteInput {
    tab: String,
    /// The ones you ticked, as listed by scan.
    resumes: Vec<Pick>,
}

#[derive(Deserialize)]
pub struct Pick {
    index: usize,
    name: String,
}

#[derive(Serialize)]
pub struct Deleted {
    pub deleted: Vec<String>,
    /// Couldn't be deleted safely: delete these yourself on SEEK.
    pub failed: Vec<String>,
}

/// Delete the resumés you confirmed, in the tab scan opened. Highest index
/// first, so the positions of the others don't shift.
pub async fn delete(
    State(state): State<AppState>,
    Json(mut input): Json<DeleteInput>,
) -> AppResult<Json<Deleted>> {
    let facts = profile::load_facts(&state.db).await?;
    let prefixes = prefixes(&facts.full_name);
    let browser = state.applier.browser();
    input.resumes.sort_by_key(|r| std::cmp::Reverse(r.index));
    let mut result = Deleted {
        deleted: vec![],
        failed: vec![],
    };
    for pick in input.resumes {
        // Only CareerPilot's own files, whatever the request says.
        if !prefixes.iter().any(|p| pick.name.starts_with(p.as_str())) {
            result.failed.push(pick.name);
            continue;
        }
        match browser
            .delete_resume(&input.tab, &prefixes, pick.index, &pick.name)
            .await
        {
            Ok(true) => result.deleted.push(pick.name),
            Ok(false) => result.failed.push(pick.name),
            Err(e) => return Err(AppError::Unavailable(e.to_string())),
        }
    }
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_from_seek_wording() {
        assert_eq!(age_minutes("Added just now"), Some(0));
        assert_eq!(age_minutes("Added 5 minutes ago"), Some(5));
        assert_eq!(age_minutes("Added an hour ago"), Some(60));
        assert_eq!(age_minutes("Added 3 days ago"), Some(3 * 24 * 60));
        assert_eq!(age_minutes("Added 2 months ago"), Some(60 * 24 * 60));
        assert_eq!(age_minutes("PDF, 120 KB"), None);
    }

    #[test]
    fn the_newest_three_stay_and_unknown_ages_are_never_suggested() {
        let found = vec![
            (
                0,
                "J - Resume - Acme.docx".into(),
                "Added 30 days ago".into(),
            ),
            (1, "J - Resume.pdf".into(), "Added 20 days ago".into()),
            (2, "J - Resume.pdf".into(), "Added just now".into()),
            (3, "J - Resume.pdf".into(), "Added 5 days ago".into()),
            (4, "J - Resume.pdf".into(), "Added 10 days ago".into()),
            (5, "J - Resume.pdf".into(), "".into()),
        ];
        let listed = suggest(found);
        let doomed: Vec<usize> = listed
            .iter()
            .filter(|l| l.delete)
            .map(|l| l.index)
            .collect();
        assert_eq!(doomed, vec![0, 1]);
    }
}
