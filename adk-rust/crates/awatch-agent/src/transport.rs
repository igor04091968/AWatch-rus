use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;

use crate::config::AgentConfig;
use crate::envelope::TelemetryEnvelope;
use crate::metrics::AgentMetrics;
use crate::spool::{FlushSummary, LocalSpool};

pub fn send_envelope(config: &AgentConfig, envelope: &TelemetryEnvelope) -> Result<()> {
    let client = Client::builder()
        .timeout(config.request_timeout())
        .build()
        .context("build agent telemetry HTTP client")?;
    client
        .post(&config.server_url)
        .json(envelope)
        .send()
        .and_then(|response| response.error_for_status())
        .map(|_| ())
        .map_err(|err| anyhow!("agent telemetry POST failed: {err}"))
}

pub fn flush_with_retry(
    config: &AgentConfig,
    spool: &LocalSpool,
    metrics: &mut AgentMetrics,
) -> Result<FlushSummary> {
    let mut attempt = 0_u32;
    loop {
        let summary = spool.process_pending(config.retry_max_attempts, |envelope| {
            send_envelope(config, envelope)
        })?;
        metrics.retry_count = metrics
            .retry_count
            .saturating_add(u64::try_from(summary.retried).unwrap_or(u64::MAX));
        if summary.retried == 0 || attempt + 1 >= config.retry_max_attempts {
            return Ok(summary);
        }
        let backoff = exponential_backoff(config.retry_base_backoff_ms, attempt);
        thread::sleep(backoff);
        attempt += 1;
    }
}

pub fn exponential_backoff(base_ms: u64, attempt: u32) -> Duration {
    let factor = 1_u64.checked_shl(attempt.min(10)).unwrap_or(1024);
    Duration::from_millis(base_ms.saturating_mul(factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_exponential_and_bounded() {
        assert_eq!(exponential_backoff(100, 0), Duration::from_millis(100));
        assert_eq!(exponential_backoff(100, 3), Duration::from_millis(800));
        assert_eq!(exponential_backoff(100, 99), Duration::from_millis(102400));
    }
}
