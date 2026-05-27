import unittest
from pathlib import Path
from unittest import mock

import proxmox.tsj_guardian_bot as MODULE


class DummyState:
    def __init__(self):
        self.pending_incident = None
        self.last_warning_signature = ""

    def save(self):
        pass


def make_bot(check_rc, check_out, heal_rc, heal_out):
    bot = object.__new__(MODULE.TSJGuardianBot)
    bot.state = DummyState()
    bot.retry_autoheal_sec = 300
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


class TsjGuardianBotTests(unittest.TestCase):
    def test_new_incident_auto_resolved_before_notification_stays_silent(self):
        check_out = "2026-05-24 10:01:18 [FAIL] node_13: curl failed: http://10.10.10.13:5600/\n"
        heal_out = "2026-05-24 10:01:21 [OK] node_13: HTTP 200 OK: http://10.10.10.13:5600/api/0/info\n"
        bot = make_bot(check_rc=1, check_out=check_out, heal_rc=0, heal_out=heal_out)

        bot._handle_check_cycle()

        self.assertIsNone(bot.state.pending_incident)
        self.assertEqual(bot._notifications, [])
        self.assertTrue(any("auto-resolved before operator notification" in msg for _, msg in bot._logs))

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
            if "aw-worktime-sessions_SHARKON2025/events" in url:
                return response([{"timestamp": fresh_worktime, "data": {"active": True}}])
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
