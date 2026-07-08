use chrono::{DateTime, Local, Utc};
use cron::Schedule as CronSchedule;
use std::str::FromStr;

pub use cron::Schedule;

/// Parse a cron expression into a [`cron::Schedule`].
///
/// Supports both standard 5-field (`min hour day month weekday`) and
/// 6-field (`sec min hour day month weekday`) expressions. Five-field
/// expressions are normalized by prepending a `0` second field.
///
/// # Errors
///
/// Returns an error string if `schedule` is not a valid cron expression.
pub fn parse_schedule(schedule: &str) -> Result<CronSchedule, String> {
    let trimmed = schedule.trim();
    let field_count = trimmed.split_whitespace().count();

    let normalized = if field_count == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    };

    CronSchedule::from_str(&normalized)
        .map_err(|e| format!("invalid cron schedule '{}': {}", schedule, e))
}

/// Validate that `schedule` is a parsable cron expression.
///
/// # Errors
///
/// Returns an error string if the schedule is invalid.
pub fn validate_schedule(schedule: &str) -> Result<(), String> {
    parse_schedule(schedule)?;
    Ok(())
}

/// Compute the next fire time of `schedule` strictly after `base`.
///
/// The `tz` argument accepts `"local"` or `"utc"` case-insensitively.
/// For `"local"`, `base` is converted to the system local timezone, the next
/// fire time is computed in local time, and then converted back to UTC.
pub fn next_fire_after(base: DateTime<Utc>, schedule: &str, tz: &str) -> Option<DateTime<Utc>> {
    let schedule = parse_schedule(schedule).ok()?;

    match tz.to_ascii_lowercase().as_str() {
        "local" => {
            let local_base = base.with_timezone(&Local);
            schedule
                .after(&local_base)
                .next()
                .map(|local_next| local_next.with_timezone(&Utc))
        }
        _ => schedule.after(&base).next(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    #[test]
    fn parses_valid_five_field_cron() {
        let schedule = parse_schedule("0 0 * * *");
        assert!(schedule.is_ok(), "expected daily at midnight to parse");
    }

    #[test]
    fn next_fire_after_is_strictly_after_base() {
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let next = next_fire_after(base, "0 * * * *", "utc").expect("expected a next fire time");
        assert!(next > base, "next fire time must be strictly after base");
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn invalid_cron_returns_error() {
        let result = parse_schedule("not a cron expression");
        assert!(result.is_err());
    }

    #[test]
    fn validate_schedule_accepts_valid_and_rejects_invalid() {
        assert!(validate_schedule("*/5 * * * *").is_ok());
        assert!(validate_schedule("bad schedule").is_err());
    }

    #[test]
    fn local_and_utc_do_not_panic() {
        let base = Utc.with_ymd_and_hms(2026, 6, 15, 23, 30, 0).unwrap();

        let next_utc = next_fire_after(base, "0 0 * * *", "utc");
        let next_local = next_fire_after(base, "0 0 * * *", "local");
        let next_utc_upper = next_fire_after(base, "0 0 * * *", "UTC");

        assert!(next_utc.is_some());
        assert!(next_local.is_some());
        assert!(next_utc_upper.is_some());

        // Local interpretation should also land strictly after the base.
        assert!(next_local.unwrap() > base);
    }
}
