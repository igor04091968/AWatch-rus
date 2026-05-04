#!/usr/bin/env python3
"""
ActivityWatch Prometheus Exporter
Собирает метрики из ActivityWatch API и экспонирует их в формате Prometheus.
"""

import time
import logging
import requests
from prometheus_client import start_http_server, Gauge, Counter, Histogram, Info
from datetime import datetime, timedelta

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Configuration
AW_SERVER_HOST = "10.10.10.13"
AW_SERVER_PORT = 5600
AW_SERVER_SCHEME = "http"
AW_API_BASE = f"{AW_SERVER_SCHEME}://{AW_SERVER_HOST}:{AW_SERVER_PORT}/api/0"
EXPORTER_PORT = 9398

# Metrics
aw_buckets_total = Gauge('aw_buckets_total', 'Total number of ActivityWatch buckets')
aw_events_total = Counter('aw_events_total', 'Total number of ActivityWatch events', ['bucket', 'event_type'])
aw_events_last_timestamp = Gauge('aw_events_last_timestamp', 'Timestamp of last event in bucket', ['bucket'])
aw_bucket_events_count = Gauge('aw_bucket_events_count', 'Number of events in bucket', ['bucket'])
aw_collector_status = Info('aw_collector_status', 'Status of ActivityWatch collectors')
aw_server_info = Info('aw_server_info', 'ActivityWatch server information')

class ActivityWatchExporter:
    def __init__(self, api_base):
        self.api_base = api_base
        self.session = requests.Session()
        self.session.headers.update({'Accept': 'application/json'})
        self.bucket_cache = {}
        
    def get_buckets(self):
        """Get all buckets from ActivityWatch API."""
        try:
            response = self.session.get(f"{self.api_base}/buckets", timeout=10)
            response.raise_for_status()
            return response.json()
        except Exception as e:
            logger.error(f"Failed to get buckets: {e}")
            return {}
    
    def get_bucket_events(self, bucket_id, limit=1):
        """Get events from a specific bucket."""
        try:
            response = self.session.get(
                f"{self.api_base}/buckets/{bucket_id}/events",
                params={'limit': limit},
                timeout=10
            )
            response.raise_for_status()
            return response.json()
        except Exception as e:
            logger.error(f"Failed to get events for {bucket_id}: {e}")
            return []
    
    def get_bucket_info(self, bucket_id):
        """Get detailed info about a bucket."""
        try:
            response = self.session.get(f"{self.api_base}/buckets/{bucket_id}", timeout=10)
            response.raise_for_status()
            return response.json()
        except Exception as e:
            logger.error(f"Failed to get info for {bucket_id}: {e}")
            return {}
    
    def collect_metrics(self):
        """Collect metrics from ActivityWatch."""
        buckets = self.get_buckets()
        
        # Update bucket count
        aw_buckets_total.set(len(buckets))
        
        # Server info
        aw_server_info.info({
            'host': AW_SERVER_HOST,
            'port': AW_SERVER_PORT,
            'scheme': AW_SERVER_SCHEME,
            'api_base': self.api_base
        })
        
        # Collector status
        collectors = {}
        for bucket_id, bucket_data in buckets.items():
            client = bucket_data.get('client', 'unknown')
            hostname = bucket_data.get('hostname', 'unknown')
            bucket_type = bucket_data.get('type', 'unknown')
            
            # Count events
            events = self.get_bucket_events(bucket_id, limit=1000)
            event_count = len(events)
            aw_bucket_events_count.labels(bucket=bucket_id).set(event_count)
            
            # Last event timestamp
            if events:
                last_event = events[0]
                timestamp = last_event.get('timestamp', 0)
                try:
                    # Convert to Unix timestamp if needed
                    if isinstance(timestamp, str):
                        dt = datetime.fromisoformat(timestamp.replace('Z', '+00:00'))
                        unix_ts = dt.timestamp()
                    else:
                        unix_ts = timestamp
                    aw_events_last_timestamp.labels(bucket=bucket_id).set(unix_ts)
                except:
                    pass
            
            # Collector status
            collector_key = f"{hostname}_{client}"
            collectors[collector_key] = {
                'status': 'active',
                'bucket': bucket_id,
                'type': bucket_type,
                'events': event_count
            }
        
        aw_collector_status.info(collectors)

def main():
    exporter = ActivityWatchExporter(AW_API_BASE)
    
    # Initial collection
    exporter.collect_metrics()
    
    # Start HTTP server
    start_http_server(EXPORTER_PORT)
    logger.info(f"ActivityWatch exporter started on port {EXPORTER_PORT}")
    logger.info(f"Scraping ActivityWatch API at {AW_API_BASE}")
    
    # Collect metrics every 30 seconds
    while True:
        time.sleep(30)
        exporter.collect_metrics()

if __name__ == '__main__':
    main()
