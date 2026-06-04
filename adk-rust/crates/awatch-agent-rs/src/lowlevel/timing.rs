use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static MONOTONIC_START: OnceLock<Instant> = OnceLock::new();

pub fn monotonic_ticks() -> u128 {
    MONOTONIC_START
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
}

pub fn high_precision_time_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_ticks_do_not_go_backwards() {
        let first = monotonic_ticks();
        let second = monotonic_ticks();

        assert!(second >= first);
    }

    #[test]
    fn high_precision_time_is_epoch_based() {
        assert!(high_precision_time_ns() > 1_000_000_000);
    }
}
