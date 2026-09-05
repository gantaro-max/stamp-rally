use std::collections::HashMap;

use chrono::{DateTime, Utc};

pub const MAX_ATTEMPTS: u32 = 5;
pub const BLOCK_DURATION_MINUTES: i64 = 15;
pub const CLEANUP_THRESHOLD: usize = 1024;

pub struct AttemptRecord {
    pub attempts: u32,
    pub last_attempt: DateTime<Utc>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AttemptResult {
    Accepted,
    Blocked(chrono::Duration),
}

pub fn register_attempt(
    records: &mut HashMap<String, AttemptRecord>,
    key: &str,
    now: DateTime<Utc>,
) -> AttemptResult {
    if let Some(record) = records.get(key) {
        let remaining = record.last_attempt + chrono::Duration::minutes(BLOCK_DURATION_MINUTES) - now;
        if record.attempts >= MAX_ATTEMPTS && remaining > chrono::Duration::zero() {
            return AttemptResult::Blocked(remaining);
        }
    }

    let record = records.entry(key.to_owned()).or_insert(AttemptRecord {
        attempts: 0,
        last_attempt: now,
    });
    if now - record.last_attempt >= chrono::Duration::minutes(BLOCK_DURATION_MINUTES) {
        record.attempts = 0;
    }
    record.attempts += 1;
    record.last_attempt = now;
    AttemptResult::Accepted
}

pub fn record_success(records: &mut HashMap<String, AttemptRecord>, key: &str) {
    records.remove(key);
}

pub fn cleanup(records: &mut HashMap<String, AttemptRecord>, now: DateTime<Utc>) {
    if records.len() <= CLEANUP_THRESHOLD {
        return;
    }
    records.retain(|_, record| {
        now - record.last_attempt < chrono::Duration::minutes(BLOCK_DURATION_MINUTES)
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{TimeZone, Utc};

    use super::*;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 5, 12, 0, 0).unwrap()
    }

    #[test]
    fn case01_empty_store_accepts_an_attempt() {
        assert_eq!(register_attempt(&mut HashMap::new(), "a", now()), AttemptResult::Accepted);
    }

    #[test]
    fn case02_second_attempt_is_accepted() {
        let mut records = HashMap::new();
        register_attempt(&mut records, "a", now());
        assert_eq!(register_attempt(&mut records, "a", now()), AttemptResult::Accepted);
    }

    #[test]
    fn case03_fifth_attempt_is_accepted() {
        let mut records = HashMap::new();
        for _ in 0..4 { register_attempt(&mut records, "a", now()); }
        assert_eq!(register_attempt(&mut records, "a", now()), AttemptResult::Accepted);
    }

    fn five_attempts() -> HashMap<String, AttemptRecord> {
        let mut records = HashMap::new();
        for _ in 0..MAX_ATTEMPTS { register_attempt(&mut records, "a", now()); }
        records
    }

    #[test]
    fn case04_sixth_attempt_is_blocked() {
        let mut records = five_attempts();
        assert!(matches!(register_attempt(&mut records, "a", now()), AttemptResult::Blocked(_)));
    }

    #[test]
    fn case05_block_remains_at_fourteen_minutes_fifty_nine_seconds() {
        let mut records = five_attempts();
        assert_eq!(register_attempt(&mut records, "a", now() + chrono::Duration::seconds(899)), AttemptResult::Blocked(chrono::Duration::seconds(1)));
    }

    #[test]
    fn case06_block_expires_at_fifteen_minutes() {
        let mut records = five_attempts();
        assert_eq!(register_attempt(&mut records, "a", now() + chrono::Duration::minutes(15)), AttemptResult::Accepted);
        assert_eq!(records["a"].attempts, 1);
    }

    #[test]
    fn case07_block_reports_remaining_time() {
        let mut records = five_attempts();
        assert_eq!(register_attempt(&mut records, "a", now() + chrono::Duration::seconds(123)), AttemptResult::Blocked(chrono::Duration::seconds(777)));
    }

    #[test]
    fn case08_expired_attempts_restart_at_one() {
        let mut records = HashMap::new();
        for _ in 0..4 { register_attempt(&mut records, "a", now()); }
        let later = now() + chrono::Duration::minutes(15);
        assert_eq!(register_attempt(&mut records, "a", later), AttemptResult::Accepted);
        assert_eq!(records["a"].attempts, 1);
    }

    #[test]
    fn case09_success_clears_attempt_history() {
        let mut records = five_attempts();
        record_success(&mut records, "a");
        assert_eq!(register_attempt(&mut records, "a", now()), AttemptResult::Accepted);
        assert_eq!(records["a"].attempts, 1);
    }

    #[test]
    fn case10_keys_are_independent() {
        let mut records = five_attempts();
        assert_eq!(register_attempt(&mut records, "b", now()), AttemptResult::Accepted);
        assert!(matches!(register_attempt(&mut records, "a", now()), AttemptResult::Blocked(_)));
    }

    #[test]
    fn case11_cleanup_skips_store_at_or_below_threshold() {
        let mut records = HashMap::new();
        records.insert("old".to_owned(), AttemptRecord { attempts: 1, last_attempt: now() });
        cleanup(&mut records, now() + chrono::Duration::minutes(15));
        assert!(records.contains_key("old"));
    }

    #[test]
    fn case12_cleanup_removes_expired_entries_above_threshold() {
        let mut records = HashMap::new();
        for index in 0..=CLEANUP_THRESHOLD {
            records.insert(format!("old-{index}"), AttemptRecord { attempts: 1, last_attempt: now() });
        }
        records.insert("fresh".to_owned(), AttemptRecord { attempts: 1, last_attempt: now() + chrono::Duration::seconds(1) });
        cleanup(&mut records, now() + chrono::Duration::minutes(15));
        assert_eq!(records.len(), 1);
        assert!(records.contains_key("fresh"));
    }
}
