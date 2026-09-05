use std::collections::HashMap;

use chrono::{DateTime, Utc};

pub const MAX_FAILURES: u32 = 5;
pub const BLOCK_DURATION_MINUTES: i64 = 15;

pub struct AttemptRecord {
    pub failures: u32,
    pub last_failure: DateTime<Utc>,
}

pub fn record_failure(records: &mut HashMap<String, AttemptRecord>, key: &str, now: DateTime<Utc>) {
    let record = records.entry(key.to_owned()).or_insert(AttemptRecord {
        failures: 0,
        last_failure: now,
    });
    if now - record.last_failure >= chrono::Duration::minutes(BLOCK_DURATION_MINUTES) {
        record.failures = 0;
    }
    record.failures += 1;
    record.last_failure = now;
}

pub fn record_success(records: &mut HashMap<String, AttemptRecord>, key: &str) {
    records.remove(key);
}

pub fn cleanup(records: &mut HashMap<String, AttemptRecord>, now: DateTime<Utc>) {
    records.retain(|_, record| {
        now - record.last_failure < chrono::Duration::minutes(BLOCK_DURATION_MINUTES)
    });
}

pub fn blocked_for(
    records: &HashMap<String, AttemptRecord>,
    key: &str,
    now: DateTime<Utc>,
) -> Option<chrono::Duration> {
    let record = records.get(key)?;
    let remaining = record.last_failure + chrono::Duration::minutes(BLOCK_DURATION_MINUTES) - now;
    (record.failures >= MAX_FAILURES && remaining > chrono::Duration::zero()).then_some(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 5, 12, 0, 0).unwrap()
    }

    #[test]
    fn case01_empty_store_is_not_blocked() {
        assert_eq!(blocked_for(&HashMap::new(), "a", now()), None);
    }
    #[test]
    fn case02_one_failure_is_not_blocked() {
        let mut records = HashMap::new();
        record_failure(&mut records, "a", now());
        assert_eq!(records["a"].failures, 1);
        assert_eq!(blocked_for(&records, "a", now()), None);
    }

    #[test]
    fn case03_four_failures_are_not_blocked() {
        let mut records = HashMap::new();
        for seconds in 0..4 {
            record_failure(
                &mut records,
                "a",
                now() + chrono::Duration::seconds(seconds),
            );
        }
        assert_eq!(records["a"].failures, 4);
        assert_eq!(
            records["a"].last_failure,
            now() + chrono::Duration::seconds(3)
        );
        assert_eq!(
            blocked_for(&records, "a", now() + chrono::Duration::seconds(3)),
            None
        );
    }

    fn five_failures() -> HashMap<String, AttemptRecord> {
        let mut records = HashMap::new();
        for _ in 0..5 {
            record_failure(&mut records, "a", now());
        }
        records
    }

    #[test]
    fn case04_five_failures_block_login() {
        assert_eq!(
            blocked_for(&five_failures(), "a", now()),
            Some(chrono::Duration::minutes(15))
        );
    }

    #[test]
    fn case05_still_blocked_after_fourteen_minutes_fifty_nine_seconds() {
        assert_eq!(
            blocked_for(
                &five_failures(),
                "a",
                now() + chrono::Duration::seconds(899)
            ),
            Some(chrono::Duration::seconds(1))
        );
    }

    #[test]
    fn case06_block_expires_at_exactly_fifteen_minutes() {
        assert_eq!(
            blocked_for(&five_failures(), "a", now() + chrono::Duration::minutes(15)),
            None
        );
    }

    #[test]
    fn case07_remaining_time_tracks_elapsed_time() {
        let records = five_failures();
        for (elapsed, remaining) in [(1, 899), (123, 777), (450, 450)] {
            assert_eq!(
                blocked_for(&records, "a", now() + chrono::Duration::seconds(elapsed)),
                Some(chrono::Duration::seconds(remaining))
            );
        }
        assert_eq!(
            blocked_for(
                &records,
                "a",
                now() + chrono::Duration::milliseconds(899_500)
            ),
            Some(chrono::Duration::milliseconds(500))
        );
    }

    #[test]
    fn case08_expired_failures_restart_at_one() {
        for minutes in [15, 16] {
            let mut records = HashMap::new();
            for _ in 0..4 {
                record_failure(&mut records, "a", now());
            }
            let later = now() + chrono::Duration::minutes(minutes);
            record_failure(&mut records, "a", later);
            assert_eq!(records["a"].failures, 1);
            assert_eq!(records["a"].last_failure, later);
            assert_eq!(blocked_for(&records, "a", later), None);
        }
    }

    #[test]
    fn case09_success_removes_failure_history() {
        let mut records = five_failures();
        record_success(&mut records, "a");
        assert!(!records.contains_key("a"));
        record_failure(&mut records, "a", now());
        assert_eq!(records["a"].failures, 1);
        assert_eq!(blocked_for(&records, "a", now()), None);
    }

    #[test]
    fn case10_keys_are_independent() {
        let mut records = five_failures();
        assert_eq!(blocked_for(&records, "b", now()), None);
        record_failure(&mut records, "b", now());
        assert_eq!(blocked_for(&records, "b", now()), None);
        record_success(&mut records, "b");
        assert!(blocked_for(&records, "a", now()).is_some());
    }

    #[test]
    fn case11_cleanup_removes_only_expired_records() {
        let mut records = five_failures();
        record_failure(&mut records, "old", now() - chrono::Duration::seconds(1));
        record_failure(&mut records, "fresh", now() + chrono::Duration::seconds(1));
        cleanup(&mut records, now() + chrono::Duration::minutes(15));
        assert_eq!(records.len(), 1);
        assert!(records.contains_key("fresh"));
    }
}
