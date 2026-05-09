#!/usr/bin/env python3
"""
ActivityWatch Prometheus Exporter
Собирает метрики из ActivityWatch API и экспонирует их в формате Prometheus.
"""

import logging
import os
import time
from datetime import datetime

import requests
from prometheus_client import Counter, Gauge, Info, start_http_server

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Configuration
AW_SERVER_HOST = os.getenv("AW_SERVER_HOST", "10.10.10.13")
AW_SERVER_PORT = int(os.getenv("AW_SERVER_PORT", "5600"))
AW_SERVER_SCHEME = os.getenv("AW_SERVER_SCHEME", "http")
AW_API_BASE = os.getenv(
    "AW_API_BASE",
    f"{AW_SERVER_SCHEME}://{AW_SERVER_HOST}:{AW_SERVER_PORT}/api/0",
)
EXPORTER_PORT = int(os.getenv("EXPORTER_PORT", "9398"))
SCRAPE_INTERVAL_SECONDS = int(os.getenv("SCRAPE_INTERVAL_SECONDS", "30"))

# Metrics
aw_up = Gauge("aw_up", "ActivityWatch API availability: 1 if the last scrape succeeded, 0 otherwise")
aw_buckets_total = Gauge("aw_buckets_total", "Total number of ActivityWatch buckets")
aw_events_total = Counter("aw_events_total", "Total number of ActivityWatch events observed", ["bucket", "event_type"])
aw_events_last_timestamp = Gauge("aw_events_last_timestamp", "Timestamp of last event in bucket", ["bucket"])
aw_bucket_events_count = Gauge("aw_bucket_events_count", "Number of events sampled from bucket", ["bucket"])
aw_collector_status = Gauge(
    "aw_collector_status",
    "ActivityWatch bucket collector status: 1 if bucket was observed during the last scrape",
    ["bucket", "client", "hostname", "type"],
)
aw_server_info = Info("aw_server", "ActivityWatch server information")


class ActivityWatchExporter:
    def __init__(self, api_base):
        self.api_base = api_base.rstrip("/")
        self.session = requests.Session()
        self.session.headers.update({"Accept": "application/json"})
        self.bucket_event_counts = {}

    def get_buckets(self):
        """Get all buckets from ActivityWatch API."""
        response = self.session.get(f"{self.api_base}/buckets", timeout=10)
        response.raise_for_status()
        return response.json()

    def get_bucket_events(self, bucket_id, limit=1000):
        """Get events from a specific bucket."""
        response = self.session.get(
            f"{self.api_base}/buckets/{bucket_id}/events",
            params={"limit": limit},
            timeout=10,
        )
        response.raise_for_status()
        return response.json()

    @staticmethod
    def event_type(event):
        data = event.get("data") or {}
        return str(data.get("app") or data.get("title") or event.get("$schema") or "unknown")

    @staticmethod
    def event_timestamp(event):
        timestamp = event.get("timestamp", 0)
        if isinstance(timestamp, str):
            return datetime.fromisoformat(timestamp.replace("Z", "+00:00")).timestamp()
        return float(timestamp or 0)

    def collect_metrics(self):
        """Collect metrics from ActivityWatch."""
        try:
            buckets = self.get_buckets()
            aw_up.set(1)
        except Exception as exc:
            logger.error("Failed to get buckets: %s", exc)
            aw_up.set(0)
            return

        aw_buckets_total.set(len(buckets))
        aw_server_info.info(
            {
                "host": AW_SERVER_HOST,
                "port": str(AW_SERVER_PORT),
                "scheme": AW_SERVER_SCHEME,
                "api_base": self.api_base,
            }
        )

        aw_collector_status.clear()
        for bucket_id, bucket_data in buckets.items():
            client = str(bucket_data.get("client", "unknown"))
            hostname = str(bucket_data.get("hostname", "unknown"))
            bucket_type = str(bucket_data.get("type", "unknown"))

            try:
                events = self.get_bucket_events(bucket_id)
            except Exception as exc:
                logger.error("Failed to get events for %s: %s", bucket_id, exc)
                events = []

            event_count = len(events)
            aw_bucket_events_count.labels(bucket=bucket_id).set(event_count)
            aw_collector_status.labels(bucket=bucket_id, client=client, hostname=hostname, type=bucket_type).set(1)

            previous_count = self.bucket_event_counts.get(bucket_id)
            if previous_count is not None and event_count > previous_count:
                for event in events[: event_count - previous_count]:
                    aw_events_total.labels(bucket=bucket_id, event_type=self.event_type(event)).inc()
            self.bucket_event_counts[bucket_id] = event_count

            if events:
                try:
                    aw_events_last_timestamp.labels(bucket=bucket_id).set(self.event_timestamp(events[0]))
                except Exception as exc:
                    logger.warning("Failed to parse last event timestamp for %s: %s", bucket_id, exc)


def main():
    exporter = ActivityWatchExporter(AW_API_BASE)
    exporter.collect_metrics()

    start_http_server(EXPORTER_PORT)
    logger.info("ActivityWatch exporter started on port %s", EXPORTER_PORT)
    logger.info("Scraping ActivityWatch API at %s", AW_API_BASE)

    while True:
        time.sleep(SCRAPE_INTERVAL_SECONDS)
        exporter.collect_metrics()


if __name__ == "__main__":
    main()
