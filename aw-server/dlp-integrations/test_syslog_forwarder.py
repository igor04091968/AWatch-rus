#!/usr/bin/env python3
import importlib.util
import json
import sys
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("syslog_forwarder.py")
SPEC = importlib.util.spec_from_file_location("syslog_forwarder", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def test_iter_new_incidents_skips_timed_out_bucket(monkeypatch):
    def fake_http_json(url, timeout=15):
        if url.endswith("/buckets/"):
            return {"aw-dlp-incidents_SHARKON2025": {}}
        raise TimeoutError("timed out")

    monkeypatch.setattr(MODULE, "http_json", fake_http_json)

    incidents, max_ids = MODULE.iter_new_incidents(
        aw_base="http://127.0.0.1:5600/api/0",
        state={"last_ids": {"aw-dlp-incidents_SHARKON2025": 42}},
        per_bucket_limit=300,
    )

    assert incidents == []
    assert max_ids == {}


def test_main_skips_aw_api_timeout_without_overwriting_state(monkeypatch, tmp_path):
    state_path = tmp_path / "syslog-forwarder-state.json"
    original_state = {"last_ids": {"aw-dlp-incidents_SHARKON2025": 99}}
    state_path.write_text(json.dumps(original_state), encoding="utf-8")

    monkeypatch.setattr(
        MODULE,
        "load_yaml",
        lambda path: {
            "aw_api_base": "http://127.0.0.1:5600/api/0",
            "state_path": str(state_path),
        },
    )
    monkeypatch.setattr(
        MODULE,
        "iter_new_incidents",
        lambda aw_base, state, per_bucket_limit: (_ for _ in ()).throw(TimeoutError("timed out")),
    )
    monkeypatch.setattr(
        MODULE,
        "save_json",
        lambda path, payload: (_ for _ in ()).throw(AssertionError("state should not be saved on AW API timeout")),
    )

    MODULE.main()

    assert json.loads(state_path.read_text(encoding="utf-8")) == original_state
