#!/usr/bin/env python3
import importlib.util
import os
import sys
from pathlib import Path


MODULE_PATH = Path(__file__).parent / "dlp-compliance" / "report_generator.py"


def load_module(name: str):
    spec = importlib.util.spec_from_file_location(name, MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


MODULE = load_module("aw_dlp_report_generator")


def test_build_aw_api_base_accepts_root_and_api_urls():
    assert MODULE.build_aw_api_base("http://127.0.0.1:5600") == "http://127.0.0.1:5600/api/0"
    assert MODULE.build_aw_api_base("http://127.0.0.1:5600/") == "http://127.0.0.1:5600/api/0"
    assert MODULE.build_aw_api_base("http://127.0.0.1:5600/api/0") == "http://127.0.0.1:5600/api/0"


def test_aw_dlp_api_base_env_takes_precedence(monkeypatch):
    monkeypatch.setenv("AW_SERVER_URL", "http://127.0.0.1:5600")
    monkeypatch.setenv("AW_DLP_AW_API_BASE", "http://127.0.0.1:5600/api/0")
    module = load_module("aw_dlp_report_generator_env_precedence")
    assert module.AW_API_BASE == "http://127.0.0.1:5600/api/0"


def test_aw_server_url_is_normalized_when_dlp_api_base_is_missing(monkeypatch):
    monkeypatch.setenv("AW_SERVER_URL", "http://127.0.0.1:5600")
    monkeypatch.delenv("AW_DLP_AW_API_BASE", raising=False)
    module = load_module("aw_dlp_report_generator_server_url_fallback")
    assert module.AW_API_BASE == "http://127.0.0.1:5600/api/0"
