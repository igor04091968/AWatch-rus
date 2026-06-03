import unittest
from pathlib import Path
from unittest import mock

import proxmox.tsj_guardian_bot as MODULE


class DummyState:
    def __init__(self):
        self.pending_incident = None
        self.last_warning_signature = ""
        self.failure_streak_signature = ""
        self.failure_streak_count = 0
        self.failure_streak_first_ts = 0

    def save(self):
        pass


def make_bot(check_rc, check_out, heal_rc, heal_out):
    bot = object.__new__(MODULE.TSJGuardianBot)
    bot.state = DummyState()
    bot.retry_autoheal_sec = 300
    bot.incident_failure_quorum_checks = 1
    bot.exit_on_autoheal_success = True
    bot.fs_immediate_ai_on_critical = False
    bot.operator_timeout = 900
    bot.check_script = "/tmp/fake-check"
    bot.heal_script = "/tmp/fake-heal"
    bot._notifications = []
    bot._logs = []
    bot._notify = lambda msg: bot._notifications.append(msg)
    bot._log = lambda level, msg: bot._logs.append((level, msg))
    bot._sync_warning_state = lambda warnings: None
    bot._run_check_script_once = lambda timeout_sec=240: (check_rc, check_out, True)
    bot._run_shell = lambda cmd, timeout_sec=420: (heal_rc, heal_out)
    bot._background_aw_rus_failures = lambda failures: ([], [])
    bot._is_aw_rus_failure_line = lambda line: line.strip().lower().startswith("[fail] aw-rus:")
    bot._extract_aw_rus_failure_keys = lambda failures: []
    bot._perform_aw_rus_autoheal = lambda failures: (True, [], ["Проверка AW-Rus + DLP:"], [])
    return bot


def make_slo_bot(summary):
    bot = object.__new__(MODULE.TSJGuardianBot)
    bot.aw_rus_slo_enabled = True
    bot.aw_rus_slo_alert_window = "24h"
    bot.aw_rus_slo_min_samples = 4
    bot.aw_rus_slo_max_age_sec = 90
    bot.aw_rus_slo_summary_cmd = "cat /tmp/slo.json"
    bot._logs = []
    bot._log = lambda level, msg: bot._logs.append((level, msg))
    bot._run_shell = lambda cmd, timeout_sec=30: (0, MODULE.json.dumps(summary))
    bot._aw_rus_probe_should_run = lambda failures: True
    bot._aw_rus_dlp_probe = lambda: (_ for _ in ()).throw(AssertionError("raw probe should not run"))
    return bot


def make_slo_cycle_bot(summary):
    bot = make_bot(check_rc=0, check_out="", heal_rc=0, heal_out="")
    bot.aw_rus_slo_enabled = True
    bot.aw_rus_slo_alert_window = "24h"
    bot.aw_rus_slo_min_samples = 4
    bot.aw_rus_slo_max_age_sec = 90
    bot.aw_rus_slo_summary_cmd = "cat /tmp/slo.json"
    bot._run_shell = lambda cmd, timeout_sec=420: (0, MODULE.json.dumps(summary))
    bot._aw_rus_probe_should_run = lambda failures: True
    bot._aw_rus_dlp_probe = lambda: (_ for _ in ()).throw(AssertionError("raw probe should not run during SLO drill"))
    bot._background_aw_rus_failures = MODULE.TSJGuardianBot._background_aw_rus_failures.__get__(
        bot,
        MODULE.TSJGuardianBot,
    )
    bot._extract_aw_rus_failure_keys = MODULE.TSJGuardianBot._extract_aw_rus_failure_keys.__get__(
        bot,
        MODULE.TSJGuardianBot,
    )
    return bot


class TsjGuardianBotTests(unittest.TestCase):
    def test_new_incident_auto_resolved_before_notification_stays_silent(self):
        check_out = "2026-05-24 10:01:18 [FAIL] node_13: curl failed: http://10.10.10.13:5600/\n"
        heal_out = "2026-05-24 10:01:21 [OK] node_13: HTTP 200 OK: http://10.10.10.13:5600/api/0/info\n"
        bot = make_bot(check_rc=1, check_out=check_out, heal_rc=0, heal_out=heal_out)

        bot._handle_check_cycle()

        self.assertIsNone(bot.state.pending_incident)
        self.assertEqual(bot._notifications, [])
        self.assertTrue(any("auto-resolved before operator notification" in msg for _, msg in bot._logs))

    def test_transient_failure_is_suppressed_until_quorum(self):
        check_out = "2026-05-24 10:01:18 [FAIL] node_13: curl failed: http://10.10.10.13:5600/\n"
        heal_out = "2026-05-24 10:01:40 [FAIL] node_13: curl failed: http://10.10.10.13:5600/\n"
        bot = make_bot(check_rc=1, check_out=check_out, heal_rc=1, heal_out=heal_out)
        bot.incident_failure_quorum_checks = 2

        bot._handle_check_cycle()

        self.assertIsNone(bot.state.pending_incident)
        self.assertEqual(bot.state.failure_streak_count, 1)
        self.assertEqual(bot._notifications, [])

        bot._handle_check_cycle()

        self.assertIsNotNone(bot.state.pending_incident)
        self.assertEqual(bot.state.failure_streak_count, 0)
        self.assertEqual(len(bot._notifications), 2)
        self.assertIn("Обнаружен инцидент", bot._notifications[0])

    def test_filesystem_critical_bypasses_failure_quorum(self):
        check_out = "2026-05-24 10:01:18 [FAIL] filesystem_usage: /var 96%\n"
        heal_out = "2026-05-24 10:01:40 [FAIL] filesystem_usage: /var 96%\n"
        bot = make_bot(check_rc=1, check_out=check_out, heal_rc=1, heal_out=heal_out)
        bot.incident_failure_quorum_checks = 3

        bot._handle_check_cycle()

        self.assertIsNotNone(bot.state.pending_incident)
        self.assertEqual(bot.state.failure_streak_count, 0)
        self.assertTrue(bot._notifications)

    def test_new_incident_notifies_only_after_autoheal_failure(self):
        check_out = "2026-05-24 10:01:18 [FAIL] node_13: curl failed: http://10.10.10.13:5600/\n"
        heal_out = "2026-05-24 10:01:40 [FAIL] node_13: curl failed: http://10.10.10.13:5600/\n"
        bot = make_bot(check_rc=1, check_out=check_out, heal_rc=1, heal_out=heal_out)

        bot._handle_check_cycle()

        self.assertIsNotNone(bot.state.pending_incident)
        self.assertEqual(len(bot._notifications), 2)
        self.assertIn("Обнаружен инцидент", bot._notifications[0])
        self.assertIn("Авто-лечение неуспешно", bot._notifications[1])

    def test_aw_only_failure_enters_background_incident_cycle(self):
        bot = make_bot(check_rc=0, check_out="", heal_rc=0, heal_out="")
        bot._background_aw_rus_failures = lambda failures: (
            ["[FAIL] aw-rus:watcher-window: watcher-window: STALE age=3600s end=2026-05-25T04:45:45Z"],
            ["watcher-window"],
        )
        bot._attempt_autoheal = lambda *args, **kwargs: False

        bot._handle_check_cycle()

        self.assertIsNotNone(bot.state.pending_incident)
        self.assertIn("aw-rus:watcher-window", "\n".join(bot.state.pending_incident.failures))
        self.assertEqual(len(bot._notifications), 2)
        self.assertIn("Обнаружен инцидент", bot._notifications[0])

    def test_aw_only_failure_auto_resolves_silently(self):
        bot = make_bot(check_rc=0, check_out="", heal_rc=0, heal_out="")
        bot._background_aw_rus_failures = lambda failures: (
            ["[FAIL] aw-rus:worktime: worktime(USER1): STALE active_seconds=0"],
            ["worktime"],
        )
        def fake_autoheal(*args, **kwargs):
            bot.state.pending_incident = None
            return True
        bot._attempt_autoheal = fake_autoheal

        bot._handle_check_cycle()

        self.assertIsNone(bot.state.pending_incident)
        self.assertEqual(bot._notifications, [])
        self.assertTrue(any("auto-resolved before operator notification" in msg for _, msg in bot._logs))

    def test_slo_budget_exhaustion_becomes_aw_failure(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": False},
            "windows": {
                "24h": {
                    "availability_percent": 99.96,
                    "samples": 10,
                    "bad_samples": 2,
                    "budget_remaining_seconds": -5,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_bot(summary)

        lines, failures = MODULE.TSJGuardianBot._aw_rus_slo_lines_and_failures(bot)
        rendered, keys = MODULE.TSJGuardianBot._background_aw_rus_failures(bot, [])

        self.assertEqual(failures, ["slo"])
        self.assertTrue(any("error budget exhausted" in line for line in lines))
        self.assertEqual(keys, ["slo"])
        self.assertIn("aw-rus:slo", rendered[0])

    def test_slo_recovered_budget_exhaustion_does_not_become_aw_failure(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": True},
            "windows": {
                "24h": {
                    "availability_percent": 99.96,
                    "samples": 10,
                    "bad_samples": 2,
                    "budget_remaining_seconds": -5,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_bot(summary)

        lines, failures = MODULE.TSJGuardianBot._aw_rus_slo_lines_and_failures(bot)
        rendered, keys = MODULE.TSJGuardianBot._background_aw_rus_failures(bot, [])

        self.assertEqual(failures, [])
        self.assertTrue(any("RECOVERED error budget exhausted" in line for line in lines))
        self.assertEqual(keys, [])
        self.assertEqual(rendered, [])

    def test_slo_status_line_marks_historical_burn_as_recovered_when_current_sample_ok(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": True},
            "windows": {
                "24h": {
                    "availability_percent": 98.43,
                    "samples": 100,
                    "bad_samples": 3,
                    "budget_remaining_seconds": -20,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_bot(summary)

        line = MODULE.TSJGuardianBot._aw_rus_slo_status_line(bot)

        self.assertIn("aw_rus_slo: recovered", line)
        self.assertIn("current_sample=OK", line)
        self.assertIn("budget_remaining_seconds=-20", line)

    def test_slo_status_line_marks_current_failure_as_fail(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": False},
            "windows": {
                "24h": {
                    "availability_percent": 98.43,
                    "samples": 100,
                    "bad_samples": 3,
                    "budget_remaining_seconds": -20,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_bot(summary)

        line = MODULE.TSJGuardianBot._aw_rus_slo_status_line(bot)

        self.assertIn("aw_rus_slo: fail", line)
        self.assertIn("current_sample=FAIL", line)

    def test_slo_warmup_does_not_alert_before_min_samples(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": False},
            "windows": {
                "24h": {
                    "availability_percent": 0.0,
                    "samples": 1,
                    "bad_samples": 1,
                    "budget_remaining_seconds": 10,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_bot(summary)

        lines, failures = MODULE.TSJGuardianBot._aw_rus_slo_lines_and_failures(bot)

        self.assertEqual(failures, [])
        self.assertTrue(any("WARMUP" in line for line in lines))

    def test_slo_failure_has_no_direct_autoheal_target(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": False},
            "windows": {
                "24h": {
                    "availability_percent": 99.96,
                    "samples": 10,
                    "bad_samples": 2,
                    "budget_remaining_seconds": -5,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_bot(summary)

        ok, report, _after_lines, after_failures = MODULE.TSJGuardianBot._perform_aw_rus_autoheal(bot, ["slo"])

        self.assertFalse(ok)
        self.assertEqual(after_failures, ["slo"])
        self.assertTrue(any("no direct autoheal target" in line for line in report))

    def test_slo_stale_autoheal_message_points_to_sampler(self):
        summary = {
            "generated_at_utc": "2026-05-30T10:00:00Z",
            "current_sample": {"ok": True},
            "windows": {"24h": {"samples": 10, "bad_samples": 0, "budget_remaining_seconds": 25, "status": "ok"}},
        }
        bot = make_slo_bot(summary)

        ok, report, after_lines, after_failures = MODULE.TSJGuardianBot._perform_aw_rus_autoheal(bot, ["slo"])

        self.assertFalse(ok)
        self.assertEqual(after_failures, ["slo"])
        self.assertTrue(any("SLO summary stale" in line for line in report))
        self.assertTrue(any("slo-summary: STALE" in line for line in after_lines))

    def test_slo_current_failure_drill_creates_incident_and_operator_notifications(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": False},
            "windows": {
                "24h": {
                    "availability_percent": 99.96,
                    "samples": 10,
                    "bad_samples": 2,
                    "budget_remaining_seconds": -5,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_cycle_bot(summary)

        bot._handle_check_cycle()

        self.assertIsNotNone(bot.state.pending_incident)
        self.assertIn("aw-rus:slo", "\n".join(bot.state.pending_incident.failures))
        self.assertEqual(len(bot._notifications), 2)
        self.assertIn("Обнаружен инцидент", bot._notifications[0])
        self.assertIn("Авто-лечение неуспешно", bot._notifications[1])

    def test_slo_recovered_historical_burn_drill_stays_silent(self):
        summary = {
            "generated_at_utc": MODULE.datetime.now(MODULE.timezone.utc).isoformat().replace("+00:00", "Z"),
            "current_sample": {"ok": True},
            "windows": {
                "24h": {
                    "availability_percent": 99.96,
                    "samples": 10,
                    "bad_samples": 2,
                    "budget_remaining_seconds": -5,
                    "status": "burning",
                }
            },
        }
        bot = make_slo_cycle_bot(summary)

        bot._handle_check_cycle()

        self.assertIsNone(bot.state.pending_incident)
        self.assertEqual(bot._notifications, [])
        self.assertTrue(any(msg == "Check OK" for _, msg in bot._logs))


class TelegramApiDocumentTests(unittest.TestCase):
    def test_send_document_rejects_empty_payload(self):
        api = MODULE.TelegramAPI("123:abc", proxy_url="")
        with self.assertRaisesRegex(RuntimeError, "Refusing to send empty document"):
            api.send_document(1, "planshet.ovpn", b"")

    @mock.patch("proxmox.tsj_guardian_bot.requests.post")
    def test_send_document_uses_requests_multipart(self, post_mock):
        response = mock.Mock()
        response.raise_for_status.return_value = None
        response.json.return_value = {"ok": True}
        post_mock.return_value = response

        api = MODULE.TelegramAPI("123:abc", proxy_url="http://127.0.0.1:11090")
        api.send_document(42, "planshet.ovpn", b"client\nremote x\n", caption="cfg")

        args, kwargs = post_mock.call_args
        self.assertEqual(args[0], "https://api.telegram.org/bot123:abc/sendDocument")
        self.assertEqual(kwargs["data"]["chat_id"], "42")
        self.assertEqual(kwargs["data"]["caption"], "cfg")
        self.assertEqual(kwargs["files"]["document"][0], "planshet.ovpn")
        self.assertEqual(kwargs["files"]["document"][1], b"client\nremote x\n")
        self.assertEqual(kwargs["proxies"], {"http": "http://127.0.0.1:11090", "https": "http://127.0.0.1:11090"})


class OpenVpnHelperTests(unittest.TestCase):
    def test_load_pfsense_readonly_env_uses_instance_override_path(self):
        env_text = """
PFSENSE_URL=https://10.10.10.1:8443
PFSENSE_API_KEY=test-key
VERIFY_SSL=false
"""
        with mock.patch.object(Path, "read_text", return_value=env_text) as read_mock:
            bot = object.__new__(MODULE.TSJGuardianBot)
            bot.pfsense_env_path = "/tmp/pfsense.env.readonly"
            base_url, api_key, verify_ssl = MODULE.TSJGuardianBot._load_pfsense_readonly_env(bot)

        self.assertEqual((base_url, api_key, verify_ssl), ("https://10.10.10.1:8443", "test-key", False))
        read_mock.assert_called_once_with(encoding="utf-8")

    def test_parse_openvpn_helper_result_returns_config_bytes(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        payload = MODULE.json.dumps(
            {
                "common_name": "planshet",
                "filename": "planshet.ovpn",
                "tunnel_network": "10.0.13.22/24",
                "cert_created": True,
                "csc_created": True,
                "config_b64": MODULE.base64.b64encode(
                    b"client\nremote 10.10.10.1 1194 udp\n<ca>\nX\n</ca>\n"
                ).decode("ascii"),
            }
        )

        filename, summary, config_bytes = MODULE.TSJGuardianBot._parse_openvpn_helper_result(bot, payload, "planshet")

        self.assertEqual(filename, "planshet.ovpn")
        self.assertIn("planshet", summary)
        self.assertIn("10.0.13.22/24", summary)
        self.assertIn("сертификат создан: да", summary)
        self.assertEqual(config_bytes, b"client\nremote 10.10.10.1 1194 udp\n<ca>\nX\n</ca>\n")

    def test_parse_openvpn_helper_result_rejects_invalid_config(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        payload = MODULE.json.dumps(
            {
                "filename": "bad.ovpn",
                "config_b64": MODULE.base64.b64encode(b"placeholder output").decode("ascii"),
            }
        )

        with self.assertRaisesRegex(RuntimeError, "missing markers"):
            MODULE.TSJGuardianBot._parse_openvpn_helper_result(bot, payload, "planshet")

    def test_load_pfsense_inventory_access_parses_root_ssh_values(self):
        inventory = """
### 2. pfSense

- SSH LAN: `10.10.10.1:2022`
- Web UI: `10.10.10.1:8443`
- Admin login: `admin`
- Admin password: `admin-secret`
- Root login: `root`
- Root password: `secret`

### 3. InfluxDB
"""
        with mock.patch.object(Path, "read_text", return_value=inventory):
            bot = object.__new__(MODULE.TSJGuardianBot)
            bot.pfsense_inventory_path = "/tmp/inventory.md"
            host, port, user, password = MODULE.TSJGuardianBot._load_pfsense_inventory_access(bot)

        self.assertEqual((host, port, user, password), ("10.10.10.1", 2022, "root", "secret"))

    def test_load_pfsense_web_access_parses_web_values(self):
        inventory = """
### 2. pfSense

- Web UI: `10.10.10.1:8443`
- Admin login: `admin`
- Admin password: `admin-secret`

### 3. InfluxDB
"""
        with mock.patch.object(Path, "read_text", return_value=inventory):
            bot = object.__new__(MODULE.TSJGuardianBot)
            bot.pfsense_inventory_path = "/tmp/inventory.md"
            base_url, user, password = MODULE.TSJGuardianBot._load_pfsense_web_access(bot)

        self.assertEqual((base_url, user, password), ("https://10.10.10.1:8443", "admin", "admin-secret"))

    def test_extract_pfsense_csrf_token(self):
        token = MODULE.TSJGuardianBot._extract_pfsense_csrf_token('var csrfMagicToken = "abc123";')
        self.assertEqual(token, "abc123")


class ProcessDecodeTests(unittest.TestCase):
    def test_decode_process_output_falls_back_to_cp1251(self):
        text = "sharkon2025\\Администратор"
        raw = text.encode("cp1251")

        decoded = MODULE.TSJGuardianBot._decode_process_output(raw)

        self.assertEqual(decoded, text)


class WorktimeCsvParseTests(unittest.TestCase):
    def test_parse_worktime_today_csv_uses_named_active_seconds_column(self):
        csv_text = (
            "user,user_id,active_seconds,active_hhmm\n"
            "user1,SHARKON2025\\\\user1,155,00:02\n"
            "администратор,SHARKON2025\\\\Администратор,9330,02:35\n"
        )

        rows = MODULE.TSJGuardianBot._parse_worktime_today_csv(csv_text)

        self.assertEqual(rows[0]["user"], "user1")
        self.assertEqual(rows[0]["active_seconds"], 155)
        self.assertEqual(rows[1]["user"], "администратор")
        self.assertEqual(rows[1]["active_seconds"], 9330)

    def test_decode_process_output_prefers_cp866_when_it_scores_better(self):
        text = "Подготовка модулей"
        raw = text.encode("cp866")

        decoded = MODULE.TSJGuardianBot._decode_process_output(raw)

        self.assertEqual(decoded, text)

    @mock.patch("proxmox.tsj_guardian_bot.requests.get")
    def test_fetch_worktime_today_csv_retries_once_after_timeout(self, get_mock):
        timeout_exc = MODULE.requests.exceptions.ReadTimeout("read timeout")
        response = mock.Mock()
        response.raise_for_status.return_value = None
        response.text = "user,user_id,active_seconds\nuser1,SHARKON2025\\\\user1,155\n"
        get_mock.side_effect = [timeout_exc, response]

        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.aw_rus_worktime_base = "http://10.10.10.13:5610"

        csv_text = MODULE.TSJGuardianBot._fetch_worktime_today_csv(bot, timeout_sec=3, attempts=2)

        self.assertIn("user1", csv_text)


class AwRusDlpProbeTests(unittest.TestCase):
    @mock.patch("proxmox.tsj_guardian_bot.requests.get")
    def test_dlp_endpoint_stale_is_downgraded_when_fileops_is_fresh(self, get_mock):
        real_datetime = MODULE.datetime
        now = real_datetime(2026, 5, 26, 19, 40, tzinfo=MODULE.timezone.utc)
        stale_endpoint = "2026-05-26T15:30:48.682Z"
        fresh_fileops = "2026-05-26T19:39:30.000Z"
        fresh_worktime = "2026-05-26T19:39:50.000Z"
        fresh_watcher = "2026-05-26T19:39:40.000Z"

        def response(payload):
            resp = mock.Mock()
            resp.raise_for_status.return_value = None
            resp.json.return_value = payload
            return resp

        def fake_get(url, timeout=20):
            if url.endswith("/buckets"):
                return response(
                    {
                        "aw-worktime-sessions_SHARKON2025": {"metadata": {"end": fresh_worktime}},
                        "aw-watcher-window_SHARKON2025": {"metadata": {"end": fresh_watcher}},
                        "aw-watcher-afk_SHARKON2025": {"metadata": {"end": fresh_watcher}},
                        "aw-dlp-endpoint-signals_SHARKON2025": {"metadata": {"end": stale_endpoint}},
                        "aw-file-operations_SHARKON2025": {"metadata": {"end": fresh_fileops}},
                        "aw-file-operations_10.10.10.13": {"metadata": {"end": fresh_fileops}},
                    }
                )
            if "aw-rus-collector-guard_SHARKON2025/events" in url:
                return response(
                    [
                        {
                            "timestamp": fresh_worktime,
                            "data": {"status": "ok", "mode": "shadow", "actions": [], "problems": []},
                        }
                    ]
                )
            if "aw-worktime-sessions_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_worktime, "data": {"active": True}}])
            if "aw-watcher-window_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {}}])
            if "aw-watcher-afk_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {}}])
            if "aw-dlp-endpoint-signals_SHARKON2025/events" in url:
                return response([{"timestamp": stale_endpoint, "data": {"signalType": "self_test"}}])
            if "aw-file-operations_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_fileops, "data": {}}])
            if "aw-file-operations_10.10.10.13/events" in url:
                return response([{"timestamp": fresh_fileops, "data": {}}])
            if url.endswith("/buckets/aw-watcher-window_SHARKON2025"):
                return response({"metadata": {"end": fresh_watcher}})
            if url.endswith("/buckets/aw-watcher-afk_SHARKON2025"):
                return response({"metadata": {"end": fresh_watcher}})
            if url.endswith("/buckets/aw-dlp-endpoint-signals_SHARKON2025"):
                return response({"metadata": {"end": stale_endpoint}})
            if url.endswith("/buckets/aw-file-operations_SHARKON2025"):
                return response({"metadata": {"end": fresh_fileops}})
            if url.endswith("/buckets/aw-file-operations_10.10.10.13"):
                return response({"metadata": {"end": fresh_fileops}})
            raise AssertionError(f"unexpected url {url}")

        get_mock.side_effect = fake_get

        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.aw_rus_api_base = "http://10.10.10.13:5600/api/0"
        bot.aw_rus_worktime_base = "http://10.10.10.13:5610"
        bot.aw_rus_host = "SHARKON2025"
        bot.aw_rus_stale_sec = 900
        bot.aw_rus_primary_user = "user1"
        bot._fetch_worktime_today_csv = lambda timeout_sec=20, attempts=2: (
            "user,user_id,active_seconds\n"
            "user1,SHARKON2025\\\\user1,120\n"
        )

        with mock.patch("proxmox.tsj_guardian_bot.datetime") as dt_mock:
            dt_mock.now.return_value = now
            dt_mock.fromisoformat.side_effect = lambda value: real_datetime.fromisoformat(value)
            lines, failures = MODULE.TSJGuardianBot._aw_rus_dlp_probe(bot)

        self.assertNotIn("dlp-endpoint", failures)
        self.assertTrue(any("file-operations fresh; endpoint collector degraded" in line for line in lines))

    @mock.patch("proxmox.tsj_guardian_bot.requests.get")
    def test_inactive_host_and_healthy_guard_downgrades_endpoint_stale_to_warn(self, get_mock):
        real_datetime = MODULE.datetime
        now = real_datetime(2026, 5, 30, 3, 20, tzinfo=MODULE.timezone.utc)
        stale_endpoint = "2026-05-30T02:40:00.000Z"
        fresh_guard = "2026-05-30T03:19:30.000Z"
        fresh_worktime = "2026-05-30T03:19:40.000Z"
        fresh_watcher = "2026-05-30T03:18:00.000Z"

        def response(payload):
            resp = mock.Mock()
            resp.raise_for_status.return_value = None
            resp.json.return_value = payload
            return resp

        def fake_get(url, timeout=20):
            if url.endswith("/buckets"):
                return response({"aw-worktime-sessions_SHARKON2025": {"metadata": {"end": fresh_worktime}}})
            if "aw-rus-collector-guard_SHARKON2025/events" in url:
                return response(
                    [
                        {
                            "timestamp": fresh_guard,
                            "data": {"status": "ok", "mode": "shadow", "actions": [], "problems": []},
                        }
                    ]
                )
            if "aw-worktime-sessions_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_worktime, "data": {"active": False}}])
            if "aw-watcher-window_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {}}])
            if "aw-watcher-afk_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {}}])
            if "aw-dlp-endpoint-signals_SHARKON2025/events" in url:
                return response([{"timestamp": stale_endpoint, "data": {"signalType": "self_test"}}])
            if "aw-file-operations_SHARKON2025/events" in url:
                return response([])
            if "aw-file-operations_10.10.10.13/events" in url:
                return response([])
            if url.endswith("/buckets/aw-file-operations_SHARKON2025"):
                return response({"metadata": {"end": stale_endpoint}})
            if url.endswith("/buckets/aw-file-operations_10.10.10.13"):
                return response({"metadata": {"end": stale_endpoint}})
            raise AssertionError(f"unexpected url {url}")

        get_mock.side_effect = fake_get

        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.aw_rus_api_base = "http://10.10.10.13:5600/api/0"
        bot.aw_rus_worktime_base = "http://10.10.10.13:5610"
        bot.aw_rus_host = "SHARKON2025"
        bot.aw_rus_stale_sec = 900
        bot.aw_rus_primary_user = "user1"
        bot._fetch_worktime_today_csv = lambda timeout_sec=20, attempts=2: (
            "user,user_id,active_seconds\n"
            "user1,SHARKON2025\\\\user1,0\n"
        )

        with mock.patch("proxmox.tsj_guardian_bot.datetime") as dt_mock:
            dt_mock.now.return_value = now
            dt_mock.fromisoformat.side_effect = lambda value: real_datetime.fromisoformat(value)
            lines, failures = MODULE.TSJGuardianBot._aw_rus_dlp_probe(bot)

        self.assertNotIn("dlp-endpoint", failures)
        self.assertTrue(any("collector-guard: OK" in line for line in lines))
        self.assertTrue(any("dlp-endpoint: WARN" in line and "guard healthy" in line for line in lines))

    @mock.patch("proxmox.tsj_guardian_bot.requests.get")
    def test_worktime_report_timeout_is_warn_when_session_bucket_is_fresh(self, get_mock):
        real_datetime = MODULE.datetime
        now = real_datetime(2026, 5, 30, 3, 20, tzinfo=MODULE.timezone.utc)
        fresh_guard = "2026-05-30T03:19:30.000Z"
        fresh_worktime = "2026-05-30T03:19:40.000Z"
        fresh_watcher = "2026-05-30T03:18:00.000Z"

        def response(payload):
            resp = mock.Mock()
            resp.raise_for_status.return_value = None
            resp.json.return_value = payload
            return resp

        def fake_get(url, timeout=20):
            if url.endswith("/buckets"):
                return response({"aw-worktime-sessions_SHARKON2025": {"metadata": {"end": fresh_worktime}}})
            if "aw-rus-collector-guard_SHARKON2025/events" in url:
                return response(
                    [
                        {
                            "timestamp": fresh_guard,
                            "data": {"status": "ok", "mode": "shadow", "actions": [], "problems": []},
                        }
                    ]
                )
            if "aw-worktime-sessions_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_worktime, "data": {"active": False}}])
            if "aw-watcher-window_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {}}])
            if "aw-watcher-afk_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {}}])
            if "aw-dlp-endpoint-signals_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_watcher, "data": {"signalType": "self_test"}}])
            if "aw-file-operations_SHARKON2025/events" in url:
                return response([])
            if "aw-file-operations_10.10.10.13/events" in url:
                return response([])
            if url.endswith("/buckets/aw-file-operations_SHARKON2025"):
                return response({"metadata": {"end": fresh_watcher}})
            if url.endswith("/buckets/aw-file-operations_10.10.10.13"):
                return response({"metadata": {"end": fresh_watcher}})
            raise AssertionError(f"unexpected url {url}")

        get_mock.side_effect = fake_get

        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.aw_rus_api_base = "http://10.10.10.13:5600/api/0"
        bot.aw_rus_worktime_base = "http://10.10.10.13:5610"
        bot.aw_rus_host = "SHARKON2025"
        bot.aw_rus_stale_sec = 900
        bot.aw_rus_primary_user = "user1"
        bot._fetch_worktime_today_csv = mock.Mock(side_effect=MODULE.requests.exceptions.ReadTimeout("read timeout"))

        with mock.patch("proxmox.tsj_guardian_bot.datetime") as dt_mock:
            dt_mock.now.return_value = now
            dt_mock.fromisoformat.side_effect = lambda value: real_datetime.fromisoformat(value)
            lines, failures = MODULE.TSJGuardianBot._aw_rus_dlp_probe(bot)

        self.assertNotIn("worktime", failures)
        self.assertTrue(any("worktime: WARN report unavailable" in line for line in lines))

    def test_endpoint_failure_uses_windows_heal_not_server_dlp_heal(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        calls = []

        def windows_heal(include_watchers, include_worktime, include_dlp=False):
            calls.append(("windows", include_watchers, include_worktime, include_dlp))
            return True, ["windows-ok"]

        def dlp_heal(targets):
            calls.append(("dlp", tuple(targets)))
            return True, ["dlp-ok"]

        bot._aw_rus_windows_collectors_heal = windows_heal
        bot._aw_rus_dlp_heal = dlp_heal
        bot._aw_rus_worktime_heal = lambda: (True, ["worktime-ok"])
        bot._aw_rus_dlp_probe = lambda: (["after"], [])

        with mock.patch("proxmox.tsj_guardian_bot.time.sleep"):
            ok, report, after_lines, after_failures = MODULE.TSJGuardianBot._perform_aw_rus_autoheal(
                bot,
                ["dlp-endpoint"],
            )

        self.assertTrue(ok)
        self.assertEqual(calls, [("windows", False, False, True)])
        self.assertIn("windows-ok", report)
        self.assertEqual(after_lines, ["after"])
        self.assertEqual(after_failures, [])

    def test_aw_autoheal_prefers_rust_plan_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        calls = []
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))

        def windows_heal(include_watchers, include_worktime, include_dlp=False):
            calls.append(("windows", include_watchers, include_worktime, include_dlp))
            return True, ["windows-ok"]

        def dlp_heal(targets):
            calls.append(("dlp", tuple(targets)))
            return True, ["dlp-ok"]

        bot._aw_rus_windows_collectors_heal = windows_heal
        bot._aw_rus_dlp_heal = dlp_heal
        bot._aw_rus_worktime_heal = lambda: (calls.append(("worktime",)) or (True, ["worktime-ok"]))
        bot._aw_rus_dlp_probe = lambda: (["after"], [])
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["payload"] = MODULE.json.loads(input_text)
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "failures": ["watcher-window", "dlp-fileops-server", "worktime"],
                            "slo_only": False,
                            "slo_stale": False,
                            "include_watchers": True,
                            "include_worktime": True,
                            "include_windows_dlp": False,
                            "server_dlp_failures": ["dlp-fileops-server"],
                            "run_windows_heal": True,
                            "run_server_dlp_heal": True,
                            "run_worktime_heal": True,
                            "sleep_after_seconds": 5,
                            "report_triggers": [
                                "- heal trigger: Windows session collectors degraded, starting remediation",
                                "- heal trigger: server-side DLP degraded, starting remediation",
                                "- heal trigger: worktime/watchers degraded, rebuilding server-side worktime views",
                            ],
                            "direct_autoheal_target": True,
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            with mock.patch("proxmox.tsj_guardian_bot.time.sleep") as sleep_mock:
                ok, report, after_lines, after_failures = MODULE.TSJGuardianBot._perform_aw_rus_autoheal(
                    bot,
                    ["watcher-window", "dlp-fileops-server", "worktime"],
                )

        self.assertTrue(ok)
        self.assertIn("--autoheal-plan-decision", captured["argv"])
        self.assertEqual(captured["payload"]["failures"], ["watcher-window", "dlp-fileops-server", "worktime"])
        self.assertEqual(
            calls,
            [
                ("windows", True, True, False),
                ("dlp", ("dlp-fileops-server",)),
                ("worktime",),
            ],
        )
        sleep_mock.assert_called_once_with(5)
        self.assertIn("windows-ok", report)
        self.assertIn("dlp-ok", report)
        self.assertIn("worktime-ok", report)
        self.assertEqual(after_lines, ["after"])
        self.assertEqual(after_failures, [])

    def test_slo_autoheal_prefers_rust_plan_backend(self):
        bot = make_slo_bot(
            {
                "generated_at_utc": "2026-05-30T10:00:00Z",
                "current_sample": {"ok": True},
                "windows": {"24h": {"samples": 10, "bad_samples": 0, "budget_remaining_seconds": 25, "status": "ok"}},
            }
        )
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                payload = MODULE.json.loads(input_text)
                if "--autoheal-plan-decision" not in argv:
                    raise AssertionError(f"unexpected argv {argv}")
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "failures": payload["failures"],
                            "slo_only": True,
                            "slo_stale": True,
                            "include_watchers": False,
                            "include_worktime": False,
                            "include_windows_dlp": False,
                            "server_dlp_failures": [],
                            "run_windows_heal": False,
                            "run_server_dlp_heal": False,
                            "run_worktime_heal": False,
                            "sleep_after_seconds": 0,
                            "report_triggers": ["from-rust-slo-trigger"],
                            "direct_autoheal_target": False,
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            ok, report, after_lines, after_failures = MODULE.TSJGuardianBot._perform_aw_rus_autoheal(bot, ["slo"])

        self.assertFalse(ok)
        self.assertEqual(report, ["from-rust-slo-trigger"])
        self.assertEqual(after_failures, ["slo"])
        self.assertTrue(any("slo-summary: STALE" in line for line in after_lines))


class CodexExecSafetyTests(unittest.TestCase):
    def test_codex_exec_does_not_forward_bearer_to_sudo(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.ai_exec_user = "igor"
        bot.ai_chat_workdir = "~"
        bot.ai_chat_sandbox = "workspace-write"
        bot.codex_model = "gpt-test"
        captured = {}

        def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
            captured["argv"] = argv
            captured["env_extra"] = env_extra or {}
            captured["cwd"] = cwd
            return MODULE.subprocess.CompletedProcess(argv, 1, "", "401 Unauthorized token_invalidated")

        bot._run_argv = fake_run_argv
        with mock.patch.dict(MODULE.os.environ, {"PFSENSE_MCP_BEARER": "secret-token"}, clear=False):
            rc, out, reply = MODULE.TSJGuardianBot._run_codex_exec_prompt(bot, "hello", timeout_sec=5)

        self.assertEqual(rc, 1)
        self.assertIn("401 Unauthorized", out)
        self.assertEqual(reply, "")
        self.assertEqual(captured["cwd"], "~")
        self.assertEqual(captured["env_extra"], {})
        self.assertNotIn("--preserve-env=PFSENSE_MCP_BEARER", captured["argv"])
        self.assertNotIn("secret-token", " ".join(captured["argv"]))

    def test_codex_auth_error_is_operator_safe(self):
        message = MODULE.TSJGuardianBot._summarize_exec_error(
            "HTTP error: 401 Unauthorized token_invalidated refresh_token_reused",
            1,
        )
        self.assertIn("повторная авторизация Codex", message)

    def test_detmir_status_line_prefers_rust_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            bot.detmir_state_file = Path(tmp) / "latest-state.json"
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                return MODULE.subprocess.CompletedProcess(argv, 0, "- detmir_auto: OK from-rust\n", "")

            bot._run_argv = fake_run_argv
            line = MODULE.TSJGuardianBot._detmir_auto_status_line(bot)

        self.assertEqual(line, "- detmir_auto: OK from-rust")
        self.assertEqual(captured["argv"][0], str(bin_path))
        self.assertEqual(captured["argv"][1], "--state")

    def test_aw_slo_status_line_prefers_rust_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot.aw_rus_slo_summary_cmd = "cat /tmp/slo.json"
        bot.aw_rus_slo_alert_window = "24h"
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    "- aw_rus_slo: recovered 24h current_sample=OK availability=99.00000% samples=4 budget_remaining_seconds=-1\n",
                    "",
                )

            bot._run_argv = fake_run_argv
            line = MODULE.TSJGuardianBot._aw_rus_slo_status_line(bot)

        self.assertIn("aw_rus_slo: recovered", line)
        self.assertIn("--aw-slo-status-line", captured["argv"])
        self.assertIn("--aw-slo-summary-command", captured["argv"])

    def test_status_text_prefers_rust_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot.infra_admin_root = "/opt/infra-admin"
        bot.detmir_state_file = Path("/var/lib/detmir-ai/latest-state.json")
        bot.state_file = "/opt/infra-admin/.state/tsj_guardian_state.json"
        bot.updates_rollback_file = Path("/opt/infra-admin/.state/rollback.json")
        bot.aw_rus_slo_summary_cmd = "cat /tmp/slo.json"
        bot.aw_rus_slo_alert_window = "24h"
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["timeout_sec"] = timeout_sec
                return MODULE.subprocess.CompletedProcess(argv, 0, "Статус: инцидентов нет.\n- from-rust\n", "")

            bot._run_argv = fake_run_argv
            text = MODULE.TSJGuardianBot._cmd_status(bot)

        self.assertIn("- from-rust", text)
        self.assertIn("--status-text", captured["argv"])
        self.assertIn("--bot-state", captured["argv"])
        self.assertIn("--rollback-file", captured["argv"])
        self.assertIn("--pfsense-status-command", captured["argv"])
        self.assertGreaterEqual(captured["timeout_sec"], 90)

    def test_status_text_falls_back_when_rust_backend_fails(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot.infra_admin_root = "/opt/infra-admin"
        bot.detmir_state_file = Path("/tmp/missing-detmir-state.json")
        bot.state_file = "/tmp/missing-bot-state.json"
        bot.updates_rollback_file = Path("/tmp/missing-rollback.json")
        bot.aw_rus_slo_summary_cmd = ""
        bot.aw_rus_slo_alert_window = "24h"
        bot.state = DummyState()
        bot.state.pending_pfsense_change = None
        bot.state.pending_openvpn_config = None
        bot.state.pending_proxmox_selection = None
        bot.state.pending_proxmox_restore = None
        bot.state.pending_update_install_confirm = False
        bot.state.pending_rollback_confirm = False
        bot.state.last_openvpn_expiry_signature = ""
        bot._pfsense_security_status_lines = lambda: "- pfsense_security: ok"
        bot._aw_rus_slo_status_line = lambda: "- aw_rus_slo: ok"
        bot._detmir_auto_status_line = lambda: "- detmir_auto: OK"
        bot._rollback_pending_count = lambda: 0
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            bot._run_argv = lambda *args, **kwargs: MODULE.subprocess.CompletedProcess(args[0], 1, "", "boom")
            text = MODULE.TSJGuardianBot._cmd_status(bot)

        self.assertIn("Статус: инцидентов нет.", text)
        self.assertIn("- detmir_auto: OK", text)
        self.assertTrue(any("status-text failed" in msg for _, msg in bot._logs))

    def test_suggestions_prefer_rust_decision_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["payload"] = MODULE.json.loads(input_text)
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps({"suggestions": ["from-rust"]}),
                    "",
                )

            bot._run_argv = fake_run_argv
            suggestions = MODULE.TSJGuardianBot._suggestions_from_failures(bot, ["[FAIL] x"])

        self.assertEqual(suggestions, ["from-rust"])
        self.assertIn("--incident-suggestions", captured["argv"])
        self.assertEqual(captured["payload"]["failures"], ["[FAIL] x"])

    def test_defer_transient_incident_prefers_rust_decision_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.incident_failure_quorum_checks = 2
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["payload"] = MODULE.json.loads(input_text)
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "defer": True,
                            "failure_streak_signature": "sig",
                            "failure_streak_count": 1,
                            "failure_streak_first_ts": 1000,
                            "reset_failure_streak": False,
                            "log_line": "Suppressing transient incident streak=1/2 failures=1",
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            defer = MODULE.TSJGuardianBot._defer_transient_new_incident(bot, ["[FAIL] x"])

        self.assertTrue(defer)
        self.assertIn("--incident-defer-decision", captured["argv"])
        self.assertEqual(bot.state.failure_streak_signature, "sig")
        self.assertEqual(bot.state.failure_streak_count, 1)
        self.assertTrue(any("Suppressing transient incident" in msg for _, msg in bot._logs))

    def test_timeout_escalation_prefers_rust_decision_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.state.pending_incident = MODULE.PendingIncident(
            incident_id="inc-1",
            created_ts=1,
            failures=["f1"],
            suggestions=[],
            last_autoheal_ts=0,
            autoheal_attempts=0,
            operator_acked=False,
            escalated_to_ai=False,
            fallback_executed=False,
        )
        bot.operator_timeout = 900
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot._notifications = []
        bot._notify = lambda msg: bot._notifications.append(msg)
        bot._escalate_to_ai = lambda: True
        bot._run_server_fallback = lambda: True
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["payload"] = MODULE.json.loads(input_text)
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "should_escalate": True,
                            "should_fallback": True,
                            "timed_out": True,
                            "operator_acked": False,
                            "age_seconds": 1000,
                            "reason": "operator_timeout_reached",
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            MODULE.TSJGuardianBot._evaluate_timeout_escalation(bot)

        self.assertIn("--escalation-decision", captured["argv"])
        self.assertTrue(bot.state.pending_incident.escalated_to_ai)
        self.assertTrue(bot.state.pending_incident.fallback_executed)
        self.assertIn("Оператор не ответил. Выполнена эскалация.", bot._notifications)
        self.assertIn("Сервер выполнил автономный fallback-план.", bot._notifications)

    def test_run_operator_action_prefers_rust_routing(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot._escalate_to_ai = lambda: True
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["payload"] = MODULE.json.loads(input_text)
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "requested_action": "techsupport",
                            "canonical_action": "support",
                            "handler": "ai_escalation",
                            "allowed": True,
                            "requires_confirmation": False,
                            "risk_level": "medium",
                            "reason": "allowed",
                            "message": None,
                            "state_update_hints": [],
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            result = MODULE.TSJGuardianBot._run_operator_action(bot, "techsupport")

        self.assertEqual(result, "/run support result=ok")
        self.assertIn("--operator-action-decision", captured["argv"])
        self.assertEqual(captured["payload"]["action"], "techsupport")
        self.assertTrue(any("Operator action routed by Rust" in msg for _, msg in bot._logs))

    def test_run_operator_action_uses_rust_confirmation_guard(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.state.pending_update_install_confirm = False
        bot.state.pending_rollback_confirm = False
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot._run_shell = lambda *args, **kwargs: (_ for _ in ()).throw(
            AssertionError("blocked action must not execute shell")
        )
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "requested_action": "updates-install-confirm",
                            "canonical_action": "updates-install-confirm",
                            "handler": "updates_install_apply",
                            "allowed": False,
                            "requires_confirmation": True,
                            "risk_level": "high",
                            "reason": "missing_update_install_confirmation",
                            "message": "Нет ожидающего запроса на установку. Сначала нажмите \"Установить критичные и важные обновления\".",
                            "state_update_hints": [],
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            result = MODULE.TSJGuardianBot._run_operator_action(bot, "updates-install-confirm")

        self.assertIn("Нет ожидающего запроса на установку", result)
        self.assertIn("Установить критичные и важные обновления", result)

    def test_pfsense_first_confirm_prefers_rust_confirmation_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.state.pending_pfsense_change = MODULE.PendingPfSenseChange(
            request_id="pf-1",
            created_ts=1000,
            operator_request="add firewall rule",
            stage="awaiting_first_confirm",
            confirm_code="123456",
            first_confirmed_ts=0,
        )
        bot.pfsense_change_confirm_ttl_sec = 900
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)
            captured = {}

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                captured["argv"] = argv
                captured["payload"] = MODULE.json.loads(input_text)
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "kind": "pfsense",
                            "action": "first_confirm",
                            "present": True,
                            "expired": False,
                            "allowed": True,
                            "clear_pending": False,
                            "next_stage": "awaiting_second_confirm",
                            "first_confirmed_ts": 1200,
                            "reason": "first_confirmed",
                            "message": None,
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            text = MODULE.TSJGuardianBot._confirm_pfsense_change_stage_one(bot)

        self.assertIn("--confirmation-decision", captured["argv"])
        self.assertEqual(captured["payload"]["kind"], "pfsense")
        self.assertEqual(captured["payload"]["action"], "first_confirm")
        self.assertEqual(bot.state.pending_pfsense_change.stage, "awaiting_second_confirm")
        self.assertEqual(bot.state.pending_pfsense_change.first_confirmed_ts, 1200)
        self.assertIn("/pfsense_apply 123456", text)

    def test_pfsense_apply_uses_rust_confirmation_guard_before_side_effect(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.state.pending_pfsense_change = MODULE.PendingPfSenseChange(
            request_id="pf-1",
            created_ts=1000,
            operator_request="add firewall rule",
            stage="awaiting_second_confirm",
            confirm_code="123456",
            first_confirmed_ts=1100,
        )
        bot.pfsense_change_confirm_ttl_sec = 900
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot._run_pfsense_change_codex_exec = lambda pending: (_ for _ in ()).throw(
            AssertionError("blocked confirmation must not execute pfSense side effect")
        )
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "kind": "pfsense",
                            "action": "apply",
                            "present": True,
                            "expired": False,
                            "allowed": False,
                            "clear_pending": False,
                            "reason": "wrong_code",
                            "message": "Неверный код второго подтверждения pfSense.",
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            result = MODULE.TSJGuardianBot._apply_pfsense_change(bot, "000000")

        self.assertEqual(result, "Неверный код второго подтверждения pfSense.")

    def test_openvpn_apply_uses_rust_confirmation_guard_before_prepare(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.state.pending_openvpn_config = MODULE.PendingOpenVpnConfig(
            request_id="ovpn-1",
            created_ts=1000,
            common_name="user1",
            stage="awaiting_second_confirm",
            confirm_code="123456",
            first_confirmed_ts=1100,
        )
        bot.openvpn_config_confirm_ttl_sec = 900
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        sent = []
        bot._send_text = lambda chat_id, text: sent.append((chat_id, text))
        bot._prepare_openvpn_config = lambda pending: (_ for _ in ()).throw(
            AssertionError("blocked confirmation must not prepare OpenVPN config")
        )
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "kind": "openvpn",
                            "action": "apply",
                            "present": True,
                            "expired": False,
                            "allowed": False,
                            "clear_pending": False,
                            "reason": "wrong_stage",
                            "message": "Второе подтверждение пока недоступно. Сначала выполните первый шаг подтверждения.",
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            MODULE.TSJGuardianBot._apply_openvpn_config(bot, 42, "123456")

        self.assertEqual(sent, [(42, "Второе подтверждение пока недоступно. Сначала выполните первый шаг подтверждения.")])

    def test_proxmox_restore_apply_uses_rust_confirmation_guard_before_snapshot_check(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.state = DummyState()
        bot.state.pending_proxmox_restore = MODULE.PendingProxmoxRestore(
            request_id="pm-1",
            created_ts=1000,
            kind="lxc",
            guest_id="200",
            guest_name="ct",
            node="pve",
            snapshot="tsj-guardian-manual",
            confirm_code="123456",
        )
        bot.proxmox_restore_confirm_ttl_sec = 900
        bot._logs = []
        bot._log = lambda level, msg: bot._logs.append((level, msg))
        bot._snapshot_exists = lambda *args, **kwargs: (_ for _ in ()).throw(
            AssertionError("blocked confirmation must not check snapshot")
        )
        with MODULE.tempfile.TemporaryDirectory() as tmp:
            bin_path = Path(tmp) / "tsj-guardian-status"
            bin_path.write_text("#!/bin/sh\n", encoding="utf-8")
            bot.tsj_guardian_status_bin = str(bin_path)

            def fake_run_argv(argv, timeout_sec=180, input_text="", env_extra=None, cwd=None):
                return MODULE.subprocess.CompletedProcess(
                    argv,
                    0,
                    MODULE.json.dumps(
                        {
                            "kind": "proxmox_restore",
                            "action": "apply",
                            "present": True,
                            "expired": False,
                            "allowed": False,
                            "clear_pending": False,
                            "reason": "wrong_code",
                            "message": "Неверный код подтверждения восстановления Proxmox.",
                        }
                    ),
                    "",
                )

            bot._run_argv = fake_run_argv
            result = MODULE.TSJGuardianBot._apply_proxmox_restore(bot, "000000")

        self.assertEqual(result, "Неверный код подтверждения восстановления Proxmox.")


class BotUiLabelTests(unittest.TestCase):
    def test_dlp_toggle_button_text_reflects_monitor_mode(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._aw_dlp_policy_request = lambda method, path, timeout_sec=5: {
            "policy": {
                "endpoint": {
                    "clipboard": [{"id": "c1", "enabled": True, "action": "alert"}],
                    "email": [{"id": "e1", "enabled": True, "action": "log"}],
                }
            }
        }

        label = MODULE.TSJGuardianBot._aw_dlp_toggle_button_text(bot)

        self.assertEqual(label, "DLP сейчас: наблюдение | включить блокировку")

    def test_menu_markup_uses_dynamic_dlp_label_and_human_dfir_label(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._aw_dlp_toggle_button_text = lambda current_mode=None: "DLP сейчас: блокировка | включить наблюдение"

        markup = MODULE.TSJGuardianBot._menu_markup(bot)

        self.assertIn(
            ["Проверка AW-Rus + DLP", "DLP сейчас: блокировка | включить наблюдение"],
            markup["keyboard"],
        )
        self.assertIn(["Форензика Windows логов"], markup["keyboard"])

    def test_hayabusa_usage_text_is_human_friendly(self):
        bot = object.__new__(MODULE.TSJGuardianBot)

        text = MODULE.TSJGuardianBot._aw_rus_hayabusa_usage_text(bot)

        self.assertIn("Форензика Windows логов:", text)
        self.assertIn("/aw_dfir /path/to/package.zip HOST [CASE_ID] [MODE]", text)

    def test_dlp_mode_command_refreshes_menu_markup(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.BTN_AW_DLP_CHECK = MODULE.TSJGuardianBot.BTN_AW_DLP_CHECK
        bot.BTN_DLP_MODE_TOGGLE = MODULE.TSJGuardianBot.BTN_DLP_MODE_TOGGLE
        bot.BTN_AW_DFIR = MODULE.TSJGuardianBot.BTN_AW_DFIR
        bot.BTN_AW_DFIR_LEGACY = MODULE.TSJGuardianBot.BTN_AW_DFIR_LEGACY
        bot.BTN_STATUS = MODULE.TSJGuardianBot.BTN_STATUS
        bot.BTN_CHECK = MODULE.TSJGuardianBot.BTN_CHECK
        bot.BTN_HEAL = MODULE.TSJGuardianBot.BTN_HEAL
        bot.BTN_ACK = MODULE.TSJGuardianBot.BTN_ACK
        bot.BTN_RESOLVE = MODULE.TSJGuardianBot.BTN_RESOLVE
        bot.BTN_HELP = MODULE.TSJGuardianBot.BTN_HELP
        bot.allowed_chats = {42}
        bot.state = DummyState()
        bot._expire_pending_pfsense_change_if_needed = lambda: None
        bot._expire_pending_openvpn_config_if_needed = lambda: None
        bot._expire_pending_proxmox_selection_if_needed = lambda: None
        bot._expire_pending_proxmox_restore_if_needed = lambda: None
        bot._log = lambda level, msg: None
        sent = {}
        bot._send_text = lambda chat_id, text, reply_markup=None: sent.update(
            {"chat_id": chat_id, "text": text, "reply_markup": reply_markup}
        )
        bot._send_text_with_menu = lambda chat_id, text: sent.update(
            {"chat_id": chat_id, "text": text, "reply_markup": bot._menu_markup()}
        )
        bot._send_menu = lambda chat_id, text: sent.update({"chat_id": chat_id, "text": text})
        bot._run_operator_action = lambda action: f"ran:{action}"
        bot._menu_markup = lambda: {"keyboard": [["Проверка AW-Rus + DLP", "DLP сейчас: наблюдение | включить блокировку"]]}
        bot._is_dlp_toggle_button = lambda text: text.startswith("DLP")
        bot._is_dfir_button = lambda text: text in {MODULE.TSJGuardianBot.BTN_AW_DFIR, MODULE.TSJGuardianBot.BTN_AW_DFIR_LEGACY}

        update = {
            "message": {
                "chat": {"id": 42},
                "text": "/dlp_mode",
            }
        }

        MODULE.TSJGuardianBot._process_message(bot, update)

        self.assertEqual(sent["chat_id"], 42)
        self.assertEqual(sent["text"], "ran:dlp-mode")
        self.assertEqual(
            sent["reply_markup"],
            {"keyboard": [["Проверка AW-Rus + DLP", "DLP сейчас: наблюдение | включить блокировку"]]},
        )

    def test_status_command_refreshes_menu_markup(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.BTN_STATUS = MODULE.TSJGuardianBot.BTN_STATUS
        bot.allowed_chats = {42}
        bot.state = DummyState()
        bot._expire_pending_pfsense_change_if_needed = lambda: None
        bot._expire_pending_openvpn_config_if_needed = lambda: None
        bot._expire_pending_proxmox_selection_if_needed = lambda: None
        bot._expire_pending_proxmox_restore_if_needed = lambda: None
        bot._log = lambda level, msg: None
        bot._cmd_status = lambda: "ok"
        sent = {}
        bot._send_text = lambda chat_id, text, reply_markup=None: sent.update(
            {"chat_id": chat_id, "text": text, "reply_markup": reply_markup}
        )
        bot._send_text_with_menu = lambda chat_id, text: sent.update(
            {"chat_id": chat_id, "text": text, "reply_markup": bot._menu_markup()}
        )
        bot._menu_markup = lambda: {"keyboard": [["Статус", "DLP сейчас: наблюдение | включить блокировку"]]}

        update = {"message": {"chat": {"id": 42}, "text": "Статус"}}
        MODULE.TSJGuardianBot._process_message(bot, update)

        self.assertEqual(sent["text"], "ok")
        self.assertEqual(
            sent["reply_markup"],
            {"keyboard": [["Статус", "DLP сейчас: наблюдение | включить блокировку"]]},
        )


class DlpPolicyToggleTests(unittest.TestCase):
    def test_aw_dlp_policy_mode_text_prefers_rust_decision_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot._aw_dlp_policy_request = lambda method, path, timeout_sec=15: {
            "name": "active",
            "policyId": 7,
            "version": 3,
            "policy": {"endpoint": {"clipboard": [{"id": "c1", "action": "alert"}]}},
        }
        captured = {}

        def fake_decision(policy, target_mode=""):
            captured["policy"] = policy
            captured["target_mode"] = target_mode
            return {
                "current_mode": "monitor",
                "groups": [{"name": "endpoint.clipboard", "blocked": 0, "total": 1}],
            }

        bot._aw_dlp_policy_decision = fake_decision

        text = MODULE.TSJGuardianBot._aw_dlp_policy_mode_text(bot)

        self.assertEqual(captured["target_mode"], "")
        self.assertIn("- mode: monitor", text)
        self.assertIn("- endpoint.clipboard: block=0/1", text)

    def test_aw_dlp_policy_toggle_text_prefers_rust_decision_backend(self):
        bot = object.__new__(MODULE.TSJGuardianBot)
        bot.aw_dlp_policy_actor = "tester"
        active_policy = {
            "endpoint": {"clipboard": [{"id": "c1", "enabled": True, "action": "alert"}]},
        }
        updated_policy = {
            "endpoint": {"clipboard": [{"id": "c1", "enabled": True, "action": "block"}]},
            "_tsj_meta": {"dlp_mode": "enforce"},
        }
        requests_seen = []

        def fake_request(method, path, payload=None, timeout_sec=20):
            requests_seen.append((method, path, payload))
            if method == "GET":
                return {"name": "active", "policyId": 7, "version": 3, "policy": active_policy}
            return {"item": {"current_version": 4}}

        bot._aw_dlp_policy_request = fake_request
        bot._aw_dlp_policy_decision = lambda policy, target_mode="": {
            "current_mode": "monitor",
            "target_mode": "enforce",
            "changed_count": 1,
            "changed_rules": ["endpoint.clipboard:c1 alert->block"],
            "updated_policy": updated_policy,
        }
        bot._aw_rus_windows_sync_dlp_policy = lambda policy: (True, ["- windows-policy-sync: OK"])
        bot._aw_dlp_toggle_button_text = lambda current_mode=None: "DLP сейчас: блокировка | включить наблюдение"

        text = MODULE.TSJGuardianBot._aw_dlp_policy_toggle_text(bot)

        self.assertEqual(requests_seen[1][0], "PUT")
        self.assertEqual(requests_seen[1][1], "/dlp/policies/7")
        self.assertEqual(requests_seen[1][2]["policy"], updated_policy)
        self.assertIn("- mode: monitor -> enforce", text)
        self.assertIn("- changed_rules: 1", text)
        self.assertIn("endpoint.clipboard:c1 alert->block", text)

    def test_aw_dlp_mode_from_policy_detects_monitor_and_enforce(self):
        monitor_policy = {
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": True, "action": "alert"}],
                "usb": [{"id": "u1", "enabled": True, "action": "log"}],
            }
        }
        enforce_policy = {
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": True, "action": "block"}],
                "usb": [{"id": "u1", "enabled": True, "action": "block"}],
            }
        }

        self.assertEqual(MODULE.TSJGuardianBot._aw_dlp_mode_from_policy(monitor_policy), "monitor")
        self.assertEqual(MODULE.TSJGuardianBot._aw_dlp_mode_from_policy(enforce_policy), "enforce")

    def test_aw_dlp_policy_for_mode_promotes_only_toggleable_channels(self):
        policy = {
            "rules": [{"id": "web1", "enabled": True, "action": "alert"}],
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": True, "action": "alert"}],
                "usb": [{"id": "u1", "enabled": True, "action": "log"}],
                "print": [{"id": "p1", "enabled": True, "action": "alert"}],
                "email": [{"id": "e1", "enabled": True, "action": "alert"}],
            },
        }

        updated, changed_count, changed_rules = MODULE.TSJGuardianBot._aw_dlp_policy_for_mode(policy, "enforce")

        self.assertEqual(updated["rules"][0]["action"], "alert")
        self.assertEqual(updated["endpoint"]["clipboard"][0]["action"], "block")
        self.assertEqual(updated["endpoint"]["usb"][0]["action"], "block")
        self.assertEqual(updated["endpoint"]["print"][0]["action"], "block")
        self.assertEqual(updated["endpoint"]["email"][0]["action"], "block")
        self.assertEqual(changed_count, 4)
        self.assertTrue(any("endpoint.clipboard:c1 alert->block" == item for item in changed_rules))

    def test_aw_dlp_policy_for_mode_demotes_block_to_alert(self):
        policy = {
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": True, "action": "block"}],
                "usb": [{"id": "u1", "enabled": True, "action": "block"}],
            }
        }

        updated, changed_count, _ = MODULE.TSJGuardianBot._aw_dlp_policy_for_mode(policy, "monitor")

        self.assertEqual(updated["endpoint"]["clipboard"][0]["action"], "alert")
        self.assertEqual(updated["endpoint"]["usb"][0]["action"], "alert")
        self.assertEqual(changed_count, 2)


if __name__ == "__main__":
    unittest.main()
