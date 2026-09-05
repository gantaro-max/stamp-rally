use std::collections::HashMap;

use chrono::{DateTime, Utc};

pub struct AttemptRecord;

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

}
