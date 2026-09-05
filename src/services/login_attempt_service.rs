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
}
