//! The daily scrape. A background task sleeps until the next run time,
//! starts a run (unless one is already going), and repeats.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::PgPool;

use super::{Trigger, execute_run, seek::SeekClient, start_run};

/// The next moment the clock in `tz` shows `at`, strictly after `now`.
pub fn next_run_after(now: DateTime<Utc>, at: NaiveTime, tz: Tz) -> DateTime<Utc> {
    let local_today = now.with_timezone(&tz).date_naive();
    for days in 0..=2 {
        let date = local_today + Duration::days(days);
        // `.earliest()` handles daylight-saving gaps and overlaps.
        if let Some(candidate) = tz.from_local_datetime(&date.and_time(at)).earliest() {
            let candidate = candidate.with_timezone(&Utc);
            if candidate > now {
                return candidate;
            }
        }
    }
    now + Duration::days(1)
}

pub fn spawn_daily(db: PgPool, client: SeekClient, at: NaiveTime, tz: Tz) {
    tokio::spawn(async move {
        loop {
            let next = next_run_after(Utc::now(), at, tz);
            tracing::info!(next_run = %next.with_timezone(&tz), "next scheduled scrape");
            let wait = (next - Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(wait).await;

            match start_run(&db, Trigger::Schedule).await {
                Ok(run) => execute_run(db.clone(), client.clone(), run.id).await,
                Err(error) => tracing::warn!(%error, "scheduled scrape skipped"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_run_is_today_or_tomorrow_in_local_time() {
        let tz: Tz = "Australia/Sydney".parse().unwrap();
        let eight = NaiveTime::from_hms_opt(8, 0, 0).unwrap();

        // 7:00 in Sydney (UTC+11 in November) -> 8:00 the same day.
        let now = tz
            .with_ymd_and_hms(2026, 11, 14, 7, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_after(now, eight, tz).with_timezone(&tz);
        assert_eq!(next.to_string(), "2026-11-14 08:00:00 AEDT");

        // 9:00 -> 8:00 tomorrow.
        let now = tz
            .with_ymd_and_hms(2026, 11, 14, 9, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        let next = next_run_after(now, eight, tz).with_timezone(&tz);
        assert_eq!(next.to_string(), "2026-11-15 08:00:00 AEDT");
    }
}
