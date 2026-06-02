use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use detmir_core::parse_utc_rfc3339;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const DEFAULT_GET_ATTEMPTS: usize = 6;

#[derive(Debug, Clone)]
pub struct ActivityWatchClient {
    api_base: String,
    client: Client,
}

impl ActivityWatchClient {
    pub fn new(api_base: impl Into<String>, timeout: Duration) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .no_proxy()
            .build()
            .context("failed to build ActivityWatch HTTP client")?;
        Ok(Self {
            api_base: api_base.into().trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        let mut last_error = None;
        for attempt in 0..DEFAULT_GET_ATTEMPTS {
            let result = self
                .client
                .get(&url)
                .send()
                .with_context(|| format!("ActivityWatch request failed: {url}"))
                .and_then(|response| {
                    response.error_for_status().with_context(|| {
                        format!("ActivityWatch returned non-success status: {url}")
                    })
                })
                .and_then(|response| {
                    response
                        .json()
                        .with_context(|| format!("failed to parse ActivityWatch JSON: {url}"))
                });

            match result {
                Ok(value) => return Ok(value),
                Err(err) => last_error = Some(err),
            }

            if attempt + 1 < DEFAULT_GET_ATTEMPTS {
                std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("ActivityWatch request failed: {url}")))
    }

    pub fn latest_event(&self, bucket_id: &str) -> Result<Option<AwEvent>> {
        let path = format!("/buckets/{bucket_id}/events?limit=1");
        let mut events: Vec<AwEvent> = self.get_json(&path)?;
        events.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
        Ok(events.into_iter().next())
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.api_base, path)
        } else {
            format!("{}/{}", self.api_base, path)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AwEvent {
    pub timestamp: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub data: serde_json::Value,
}

impl AwEvent {
    pub fn timestamp_utc(&self) -> Result<DateTime<Utc>> {
        parse_utc_rfc3339(&self.timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_paths_without_double_slashes() {
        let client =
            ActivityWatchClient::new("http://127.0.0.1:5600/api/0/", Duration::from_secs(1))
                .unwrap();
        assert_eq!(
            client.url("/buckets/x/events?limit=1"),
            "http://127.0.0.1:5600/api/0/buckets/x/events?limit=1"
        );
    }

    #[test]
    fn parses_event_timestamp() {
        let event = AwEvent {
            timestamp: "2026-05-31T10:20:30Z".to_string(),
            duration: 0.0,
            data: serde_json::json!({}),
        };
        assert_eq!(event.timestamp_utc().unwrap().timestamp(), 1_780_222_830);
    }
}
