use std::collections::HashMap;

use chrono::{DateTime, Utc};

pub struct AttemptRecord {
    pub failures: u32,
    pub last_failure: DateTime<Utc>,
}

pub fn record_failure(records: &mut HashMap<String, AttemptRecord>, key: &str, now: DateTime<Utc>) {
    let record = records.entry(key.to_owned()).or_insert(AttemptRecord {
        failures: 0,
        last_failure: now,
    });
    record.failures += 1;
    record.last_failure = now;
}

pub fn blocked_for(
    _records: &HashMap<String, AttemptRecord>,
    _key: &str,
    _now: DateTime<Utc>,
) -> Option<chrono::Duration> {
    None
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
            record_failure(&mut records, "a", now() + chrono::Duration::seconds(seconds));
        }
        assert_eq!(records["a"].failures, 4);
        assert_eq!(records["a"].last_failure, now() + chrono::Duration::seconds(3));
        assert_eq!(blocked_for(&records, "a", now() + chrono::Duration::seconds(3)), None);
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
        assert_eq!(blocked_for(&five_failures(), "a", now()), Some(chrono::Duration::minutes(15)));
    }

}
