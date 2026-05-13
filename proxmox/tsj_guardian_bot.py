#!/usr/bin/env python3
import hashlib
import json
import os
import re
import secrets
import shlex
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import traceback
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from urllib.parse import urlencode
from urllib.request import Request, build_opener, ProxyHandler

import requests


def env_bool(name: str, default: bool) -> bool:
    raw = os.getenv(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


def env_int(name: str, default: int) -> int:
    raw = os.getenv(name)
    if raw is None:
        return default
    try:
        return int(raw.strip())
    except ValueError:
        return default


def mask_proxy_url(url: str) -> str:
    if "@" in url and "://" in url:
        scheme, rest = url.split("://", 1)
        auth_host = rest.split("@", 1)
        if len(auth_host) == 2:
            return f"{scheme}://***:***@{auth_host[1]}"
    return url


@dataclass
class PendingIncident:
    incident_id: str
    created_ts: int
    failures: List[str]
    suggestions: List[str]
    last_autoheal_ts: int
    autoheal_attempts: int
    operator_acked: bool
    escalated_to_ai: bool
    fallback_executed: bool


@dataclass
class PendingPfSenseChange:
    request_id: str
    created_ts: int
    operator_request: str
    stage: str
    confirm_code: str
    first_confirmed_ts: int


@dataclass
class PendingOpenVpnConfig:
    request_id: str
    created_ts: int
    common_name: str
    stage: str
    confirm_code: str
    first_confirmed_ts: int


@dataclass
class PendingProxmoxSelection:
    mode: str
    created_ts: int


@dataclass
class PendingProxmoxRestore:
    request_id: str
    created_ts: int
    kind: str
    guest_id: str
    guest_name: str
    node: str
    snapshot: str
    confirm_code: str


class GuardianState:
    def __init__(self, path: str):
        self.path = path
        self.last_update_id = 0
        self.last_operator_message_ts = 0
        self.pending_incident: Optional[PendingIncident] = None
        self.pending_pfsense_change: Optional[PendingPfSenseChange] = None
        self.pending_openvpn_config: Optional[PendingOpenVpnConfig] = None
        self.pending_proxmox_selection: Optional[PendingProxmoxSelection] = None
        self.pending_proxmox_restore: Optional[PendingProxmoxRestore] = None
        self.pending_update_install_confirm = False
        self.pending_rollback_confirm = False
        self.last_warning_signature = ""
        self.last_openvpn_expiry_signature = ""
        self.ai_chat_intro_variant = -1

    def load(self) -> None:
        if not os.path.exists(self.path):
            return
        with open(self.path, "r", encoding="utf-8") as f:
            raw = json.load(f)
        self.last_update_id = int(raw.get("last_update_id", 0))
        self.last_operator_message_ts = int(raw.get("last_operator_message_ts", 0))
        self.pending_update_install_confirm = bool(raw.get("pending_update_install_confirm", False))
        self.pending_rollback_confirm = bool(raw.get("pending_rollback_confirm", False))
        self.last_warning_signature = str(raw.get("last_warning_signature", ""))
        self.last_openvpn_expiry_signature = str(raw.get("last_openvpn_expiry_signature", ""))
        self.ai_chat_intro_variant = int(raw.get("ai_chat_intro_variant", -1))
        pi = raw.get("pending_incident")
        if pi:
            self.pending_incident = PendingIncident(**pi)
        ppc = raw.get("pending_pfsense_change")
        if ppc:
            self.pending_pfsense_change = PendingPfSenseChange(**ppc)
        povpn = raw.get("pending_openvpn_config")
        if povpn:
            self.pending_openvpn_config = PendingOpenVpnConfig(**povpn)
        pps = raw.get("pending_proxmox_selection")
        if pps:
            self.pending_proxmox_selection = PendingProxmoxSelection(**pps)
        ppr = raw.get("pending_proxmox_restore")
        if ppr:
            self.pending_proxmox_restore = PendingProxmoxRestore(**ppr)

    def save(self) -> None:
        os.makedirs(os.path.dirname(self.path), exist_ok=True)
        payload = {
            "last_update_id": self.last_update_id,
            "last_operator_message_ts": self.last_operator_message_ts,
            "pending_incident": asdict(self.pending_incident) if self.pending_incident else None,
            "pending_pfsense_change": asdict(self.pending_pfsense_change) if self.pending_pfsense_change else None,
            "pending_openvpn_config": asdict(self.pending_openvpn_config) if self.pending_openvpn_config else None,
            "pending_proxmox_selection": asdict(self.pending_proxmox_selection) if self.pending_proxmox_selection else None,
            "pending_proxmox_restore": asdict(self.pending_proxmox_restore) if self.pending_proxmox_restore else None,
            "pending_update_install_confirm": self.pending_update_install_confirm,
            "pending_rollback_confirm": self.pending_rollback_confirm,
            "last_warning_signature": self.last_warning_signature,
            "last_openvpn_expiry_signature": self.last_openvpn_expiry_signature,
            "ai_chat_intro_variant": self.ai_chat_intro_variant,
        }
        tmp = self.path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(payload, f, ensure_ascii=False, indent=2)
        os.replace(tmp, self.path)


class TelegramAPI:
    def __init__(self, token: str, timeout_sec: int = 20, proxy_url: str = ""):
        self.base = f"https://api.telegram.org/bot{token}"
        self.timeout_sec = timeout_sec
        self.proxy_url = proxy_url.strip()
        if self.proxy_url:
            self.opener = build_opener(
                ProxyHandler({"http": self.proxy_url, "https": self.proxy_url})
            )
        else:
            self.opener = build_opener()

    def _should_retry(self, exc: Exception) -> bool:
        text = str(exc).lower()
        return any(
            marker in text
            for marker in (
                "503 service unavailable",
                "tunnel connection failed",
                "connection refused",
                "connection reset",
                "timed out",
                "temporary failure",
                "name or service not known",
                "network is unreachable",
            )
        )

    def _call(self, method: str, params: Dict) -> Dict:
        url = f"{self.base}/{method}"
        data = urlencode(params).encode("utf-8")
        req = Request(url, data=data, method="POST")
        last_exc: Optional[Exception] = None
        attempts = 6 if self.proxy_url else 3
        for attempt in range(1, attempts + 1):
            try:
                with self.opener.open(req, timeout=self.timeout_sec + 5) as resp:
                    body = resp.read().decode("utf-8")
                break
            except Exception as exc:
                last_exc = exc
                if attempt >= attempts or not self._should_retry(exc):
                    raise
                print(
                    f"{time.strftime('%Y-%m-%d %H:%M:%S')} [WARN] Telegram API {method} attempt {attempt}/{attempts} failed: {exc}",
                    flush=True,
                )
                time.sleep(min(attempt * 2, 10))
        else:
            raise last_exc if last_exc else RuntimeError(f"Telegram API call failed: {method}")
        parsed = json.loads(body)
        if not parsed.get("ok"):
            raise RuntimeError(f"Telegram API error on {method}: {parsed}")
        return parsed

    def get_updates(self, offset: int, timeout: int = 15) -> List[Dict]:
        r = self._call("getUpdates", {"offset": offset, "timeout": timeout})
        return r.get("result", [])

    def send_message(self, chat_id: int, text: str, reply_markup: Optional[Dict] = None) -> None:
        payload = {"chat_id": chat_id, "text": text}
        if reply_markup is not None:
            payload["reply_markup"] = json.dumps(reply_markup, ensure_ascii=False)
        self._call("sendMessage", payload)

    def send_document(self, chat_id: int, filename: str, content: bytes, caption: str = "") -> None:
        boundary = f"----codex{secrets.token_hex(12)}"
        parts: List[bytes] = []

        def add_field(name: str, value: str) -> None:
            parts.append(
                (
                    f"--{boundary}\r\n"
                    f'Content-Disposition: form-data; name="{name}"\r\n\r\n'
                    f"{value}\r\n"
                ).encode("utf-8")
            )

        add_field("chat_id", str(chat_id))
        if caption:
            add_field("caption", caption)

        parts.append(
            (
                f"--{boundary}\r\n"
                f'Content-Disposition: form-data; name="document"; filename="{filename}"\r\n'
                "Content-Type: application/octet-stream\r\n\r\n"
            ).encode("utf-8")
        )
        parts.append(content)
        parts.append(f"\r\n--{boundary}--\r\n".encode("utf-8"))

        req = Request(
            f"{self.base}/sendDocument",
            data=b"".join(parts),
            method="POST",
            headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
        )
        with self.opener.open(req, timeout=self.timeout_sec + 10) as resp:
            body = resp.read().decode("utf-8")
        parsed = json.loads(body)
        if not parsed.get("ok"):
            raise RuntimeError(f"Telegram API error on sendDocument: {parsed}")

    def send_long_message(
        self,
        chat_id: int,
        text: str,
        reply_markup: Optional[Dict] = None,
        limit: int = 3500,
    ) -> None:
        chunks: List[str] = []
        remaining = (text or "").strip()
        if not remaining:
            self.send_message(chat_id, "-", reply_markup=reply_markup)
            return

        while len(remaining) > limit:
            split_at = remaining.rfind("\n", 0, limit)
            if split_at <= 0:
                split_at = limit
            chunks.append(remaining[:split_at].strip())
            remaining = remaining[split_at:].strip()
        if remaining:
            chunks.append(remaining)

        for idx, chunk in enumerate(chunks):
            self.send_message(chat_id, chunk, reply_markup=reply_markup if idx == 0 else None)


class TSJGuardianBot:
    BTN_STATUS = "Статус"
    BTN_CHECK = "Диагностика"
    BTN_HEAL = "Лечение"
    BTN_ACK = "Подтвердить инцидент"
    BTN_RESOLVE = "Закрыть инцидент"
    BTN_AI = "Эскалация в тех.поддержку"
    BTN_FALLBACK = "Server Fallback"
    BTN_HELP = "Помощь"
    BTN_UPD_CHECK = "Проверить критичные и важные обновления"
    BTN_UPD_INSTALL = "Установить критичные и важные обновления"
    BTN_UPD_INSTALL_CONFIRM = "Подтвердить установку обновлений"
    BTN_UPD_ROLLBACK_CONFIRM = "Подтвердить откат узла"
    BTN_PM_SNAPSHOT = "Создать Proxmox снапшот"
    BTN_PM_RESTORE = "Восстановить из снапшота"
    BTN_AI_CHAT = "Тех.поддержка"
    BTN_OVPN_CERTS = "OpenVPN сертификаты"
    BTN_OVPN_EXPIRING = "Истекающие OpenVPN cert"
    BTN_OVPN_CONFIG = "Новый OpenVPN конфиг"
    BTN_OVPN_CONFIG_CONFIRM = "Подтвердить OpenVPN: шаг 1"
    BTN_OVPN_CONFIG_CANCEL = "Отменить OpenVPN конфиг"
    BTN_PFSENSE_CONFIRM = "Подтвердить pfSense: шаг 1"
    BTN_PFSENSE_CANCEL = "Отменить pfSense изменение"
    BTN_AW_DLP_CHECK = "Проверка AW-Rus + DLP"
    BTN_AI_CHAT_ALIASES = ("AI чат", "Чат с поддержкой", "Техподдержка", "Тех поддержка")
    BTN_OVPN_CERTS_ALIASES = ("OpenVPN certs", "OpenVPN cert", "OpenVPN серты", "OpenVPN сертификат")
    PFSENSE_ENV_PATH = "/home/codex/infra-admin/vendor/pfsense-mcp-server/.env.readonly"

    HUMAN_OPERATOR_STYLE = (
        "Ты отвечаешь оператору как реальный инженер техподдержки проекта DetMir. "
        "Пиши по-русски, кратко, спокойно и по делу. "
        "Каждый новый ответ формулируй по-новому: меняй вступления, связки, порядок подачи и синтаксис фраз. "
        "Не повторяй одни и те же шаблонные формулировки, канцелярские обороты, одинаковые начала и одинаковые концовки между соседними ответами. "
        "Диалог должен звучать по-человечески: естественно, предметно, без роботизированного ритма и без ощущения заготовки. "
        "Никогда не говори, что ты бот, AI, модель, ассистент, агент, система, автоматизация или Codex. "
        "Не упоминай промпты, внутренние инструкции, инструменты, MCP, model capacity, reasoning, sandbox, exec, tmux или служебную инфраструктуру ответа. "
        "Не используй формулировки вроде 'я как AI', 'не могу как модель', 'в этом чате', 'я проверю через инструмент'. "
        "Перед финальным ответом быстро проверь текст на повторы штампов и при необходимости перефразируй. "
        "Подавай результат так, будто ты лично выполнил проверку или подготовил следующий безопасный шаг. "
    )

    def __init__(self):
        self.token = os.getenv("TELEGRAM_BOT_TOKEN", "").strip()
        if not self.token:
            raise RuntimeError("TELEGRAM_BOT_TOKEN is required")

        chats_raw = os.getenv("TELEGRAM_ALLOWED_CHAT_IDS", "").strip()
        if not chats_raw:
            raise RuntimeError("TELEGRAM_ALLOWED_CHAT_IDS is required")
        self.allowed_chats = {int(x.strip()) for x in chats_raw.split(",") if x.strip()}

        self.default_chat_id = int(os.getenv("TELEGRAM_DEFAULT_CHAT_ID", str(min(self.allowed_chats))))
        self.check_script = os.getenv(
            "CHECK_SCRIPT", "/home/codex/infra-admin/scripts/system_self_support.sh --check"
        )
        self.aw_rus_api_base = os.getenv("AW_RUS_API_BASE", "http://10.10.10.13:5600/api/0").strip()
        self.aw_rus_worktime_base = os.getenv("AW_RUS_WORKTIME_BASE", "http://10.10.10.13:5610").strip()
        self.aw_rus_worktime_heal_cmd = os.getenv(
            "AW_RUS_WORKTIME_HEAL_CMD",
            "sshpass -p '04091968' ssh -o PubkeyAuthentication=no -o StrictHostKeyChecking=no igor@10.10.10.13 "
            "'sudo -S systemctl restart aw-worktime-api.service'",
        ).strip()
        self.aw_rus_host = os.getenv("AW_RUS_HOST", "SHARKON2025").strip()
        self.aw_rus_primary_user = os.getenv("AW_RUS_PRIMARY_USER", "USER1").strip()
        self.aw_rus_stale_sec = max(60, env_int("AW_RUS_STALE_SEC", 900))
        self.heal_script = os.getenv(
            "HEAL_SCRIPT", "/home/codex/infra-admin/scripts/system_self_support.sh --heal"
        )
        self.state_file = os.getenv(
            "STATE_FILE", "/home/codex/infra-admin/.state/tsj_guardian_state.json"
        )
        self.log_file = os.getenv(
            "LOG_FILE", "/home/codex/infra-admin/logs/tsj_guardian_bot.log"
        )
        self.heartbeat_file = os.getenv(
            "HEARTBEAT_FILE", "/home/codex/infra-admin/.state/tsj_guardian_heartbeat"
        )
        self.check_interval = env_int("CHECK_INTERVAL_SEC", 60)
        self.operator_timeout = env_int("OPERATOR_TIMEOUT_SEC", 900)  # 15 min
        self.retry_autoheal_sec = env_int("RETRY_AUTORECOVERY_EVERY_SEC", 300)
        self.exit_on_autoheal_success = env_bool("EXIT_ON_AUTORECOVERY_SUCCESS", True)

        self.ai_escalation_mode = os.getenv("AI_ESCALATION_MODE", "codex_exec").strip().lower() or "codex_exec"
        self.ai_exec_user = os.getenv("AI_EXEC_USER", os.getenv("TMUX_USER", "codex")).strip() or "codex"
        self.codex_model = os.getenv("CODEX_MODEL", "gpt-5.3-codex").strip() or "gpt-5.3-codex"
        fallback_models_raw = os.getenv("CODEX_FALLBACK_MODELS", "gpt-5.4-mini").strip()
        self.codex_fallback_models = [
            model.strip()
            for model in fallback_models_raw.split(",")
            if model.strip() and model.strip() != self.codex_model
        ]
        self.tmux_session = os.getenv("TMUX_SESSION", "ai")
        self.tmux_user = os.getenv("TMUX_USER", self.ai_exec_user).strip() or self.ai_exec_user
        self.tmux_create_if_missing = env_bool("TMUX_CREATE_IF_MISSING", False)
        self.tmux_start_cmd = os.getenv("TMUX_START_COMMAND", "codex")
        self.enable_ai_escalation = env_bool("ENABLE_AI_ESCALATION", True)
        self.fs_immediate_ai_on_critical = env_bool("FS_IMMEDIATE_AI_ON_CRITICAL", True)
        self.ai_chat_enabled = env_bool("AI_CHAT_ENABLED", True)
        self.ai_chat_timeout_sec = env_int("AI_CHAT_TIMEOUT_SEC", 1800)
        self.ai_chat_workdir = os.getenv("AI_CHAT_WORKDIR", "/home/codex/infra-admin").strip()
        self.ai_chat_sandbox = os.getenv("AI_CHAT_SANDBOX", "workspace-write").strip() or "workspace-write"
        self.openvpn_cert_check_timeout_sec = max(30, env_int("OPENVPN_CERT_CHECK_TIMEOUT_SEC", 240))
        self.openvpn_expiry_warn_timeout_sec = max(30, env_int("OPENVPN_EXPIRY_WARN_TIMEOUT_SEC", 120))
        self.pfsense_change_control_enabled = env_bool("PFSENSE_CHANGE_CONTROL_ENABLED", True)
        self.pfsense_change_confirm_ttl_sec = env_int("PFSENSE_CHANGE_CONFIRM_TTL_SEC", 900)
        self.openvpn_config_enabled = env_bool("OPENVPN_CONFIG_ENABLED", True)
        self.openvpn_config_confirm_ttl_sec = env_int("OPENVPN_CONFIG_CONFIRM_TTL_SEC", 900)
        self.openvpn_expiry_warn_enabled = env_bool("OPENVPN_EXPIRY_WARN_ENABLED", True)
        self.openvpn_expiry_warn_days = env_int("OPENVPN_EXPIRY_WARN_DAYS", 30)
        self.openvpn_expiry_warn_interval_sec = max(300, env_int("OPENVPN_EXPIRY_WARN_INTERVAL_SEC", 21600))
        self.telegram_proxy_url = (
            os.getenv("TELEGRAM_PROXY_URL", "").strip()
            or os.getenv("HTTPS_PROXY", "").strip()
            or os.getenv("HTTP_PROXY", "").strip()
        )

        # If AI path is down ("my death"), server must continue autonomously.
        self.enable_server_fallback = env_bool("ENABLE_SERVER_FALLBACK", True)
        self.server_fallback_commands = [
            x.strip() for x in os.getenv(
                "SERVER_FALLBACK_COMMANDS",
                "/home/codex/infra-admin/scripts/system_self_support.sh --heal"
            ).split(";;") if x.strip()
        ]
        self.updates_script = os.getenv(
            "UPDATES_SCRIPT",
            "/usr/bin/python3 /home/codex/infra-admin/scripts/proxmox_lxc_critical_updates.py",
        )
        self.updates_status_file = Path(
            os.getenv(
                "UPDATES_STATUS_FILE",
                "/home/codex/infra-admin/.state/proxmox_lxc_critical_updates.json",
            )
        )
        self.updates_rollback_file = Path(
            os.getenv(
                "UPDATES_ROLLBACK_FILE",
                "/home/codex/infra-admin/.state/proxmox_lxc_pending_rollback.json",
            )
        )
        self.proxmox_selection_ttl_sec = env_int("PROXMOX_SELECTION_TTL_SEC", 900)
        self.proxmox_restore_confirm_ttl_sec = env_int("PROXMOX_RESTORE_CONFIRM_TTL_SEC", 900)
        self.proxmox_manual_snapshot_name = (
            os.getenv("PROXMOX_MANUAL_SNAPSHOT_NAME", "tsj-guardian-manual").strip()
            or "tsj-guardian-manual"
        )
        self.pct_bin = os.getenv("PCT_BIN", "/usr/sbin/pct").strip() or "/usr/sbin/pct"
        self.qm_bin = os.getenv("QM_BIN", "/usr/sbin/qm").strip() or "/usr/sbin/qm"
        self.pvesh_bin = os.getenv("PVESH_BIN", "/usr/bin/pvesh").strip() or "/usr/bin/pvesh"

        self.api = TelegramAPI(self.token, proxy_url=self.telegram_proxy_url)
        self.state = GuardianState(self.state_file)
        self.state.load()
        self.next_openvpn_expiry_warn_ts = time.time() + self.openvpn_expiry_warn_interval_sec
        self._openvpn_cert_check_lock = threading.Lock()
        self._openvpn_cert_check_running = False
        self._openvpn_expiry_check_lock = threading.Lock()
        self._openvpn_expiry_check_running = False
        self._check_cycle_lock = threading.Lock()
        self._check_cycle_running = False
        self._updates_action_lock = threading.Lock()
        self._updates_action_running = ""

        os.makedirs(os.path.dirname(self.log_file), exist_ok=True)
        os.makedirs(os.path.dirname(self.heartbeat_file), exist_ok=True)
        self._log("INFO", "TSJ guardian bot initialized")
        if self.telegram_proxy_url:
            self._log("INFO", f"Telegram proxy enabled: {mask_proxy_url(self.telegram_proxy_url)}")
        else:
            self._log("WARN", "Telegram proxy is not configured")

    def _log(self, level: str, message: str) -> None:
        line = f"{time.strftime('%Y-%m-%d %H:%M:%S')} [{level}] {message}"
        print(line, flush=True)
        with open(self.log_file, "a", encoding="utf-8") as f:
            f.write(line + "\n")

    def _touch_heartbeat(self) -> None:
        with open(self.heartbeat_file, "w", encoding="utf-8") as f:
            f.write(str(int(time.time())))

    def _notify(self, text: str) -> None:
        try:
            self.api.send_message(self.default_chat_id, text)
        except Exception as exc:
            self._log("ERROR", f"Failed to send Telegram message: {exc}")

    def _menu_markup(self) -> Dict:
        return {
            "keyboard": [
                [self.BTN_STATUS, self.BTN_CHECK, self.BTN_HEAL],
                [self.BTN_AW_DLP_CHECK],
                [self.BTN_ACK, self.BTN_RESOLVE],
                [self.BTN_AI, self.BTN_FALLBACK],
                [self.BTN_AI_CHAT],
                [self.BTN_OVPN_CERTS, self.BTN_OVPN_EXPIRING],
                [self.BTN_OVPN_CONFIG],
                [self.BTN_OVPN_CONFIG_CONFIRM, self.BTN_OVPN_CONFIG_CANCEL],
                [self.BTN_PFSENSE_CONFIRM, self.BTN_PFSENSE_CANCEL],
                [self.BTN_UPD_CHECK, self.BTN_UPD_INSTALL],
                [self.BTN_UPD_INSTALL_CONFIRM, self.BTN_UPD_ROLLBACK_CONFIRM],
                [self.BTN_PM_SNAPSHOT, self.BTN_PM_RESTORE],
                [self.BTN_HELP],
            ],
            "resize_keyboard": True,
            "one_time_keyboard": False,
        }

    def _send_menu(self, chat_id: int, text: str) -> None:
        self.api.send_long_message(chat_id, text, reply_markup=self._menu_markup())

    def _send_text(self, chat_id: int, text: str) -> None:
        self.api.send_long_message(chat_id, text)

    def _local_time_text(self) -> str:
        return datetime.now().astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")

    def _run_check_script_once(self, timeout_sec: int = 240) -> Tuple[Optional[int], str, bool]:
        with self._check_cycle_lock:
            if self._check_cycle_running:
                return None, "check already running", False
            self._check_cycle_running = True

        try:
            rc, out = self._run_shell(self.check_script, timeout_sec=timeout_sec)
            return rc, out, True
        finally:
            with self._check_cycle_lock:
                self._check_cycle_running = False

    def _next_ai_chat_intro_text(self) -> str:
        variants = [
            "На связи. Напишите сообщение обычным текстом, разберу вопрос и отвечу здесь.",
            "Можно писать прямо сюда без команд. Опишите проблему своими словами, дальше подхвачу.",
            "Диалог открыт. Отправьте следующий вопрос текстом, продолжим здесь же.",
            "Готов продолжать в этом чате. Просто напишите, что именно нужно проверить или поправить.",
            "Пишите сразу по сути. Сообщение можно отправить обычным текстом, отвечу в этой переписке.",
            "Связь открыта. Опишите задачу как есть, дальше разберу и дам ответ здесь.",
            "Продолжаем здесь. Напишите проблему или вопрос обычным сообщением, подключусь по месту.",
            "Можно без дополнительных команд. Просто отправьте сообщение, и я отвечу по ситуации.",
        ]
        next_idx = (self.state.ai_chat_intro_variant + 1) % len(variants)
        self.state.ai_chat_intro_variant = next_idx
        self.state.save()
        return variants[next_idx]

    def _updates_action_description(self, action: str) -> Tuple[str, str]:
        if action == "updates-check":
            return ("проверку критичных и важных обновлений", "до 10-15 минут")
        if action == "updates-install-confirm":
            return ("установку критичных и важных обновлений", "до 30-90 минут")
        if action == "updates-rollback-confirm":
            return ("откат после неуспешного обновления", "до 15-30 минут")
        return ("операцию обновлений", "несколько минут")

    def _updates_progress_text(self, action: str, started_at_ts: float) -> str:
        description, _ = self._updates_action_description(action)
        elapsed_sec = max(1, int(time.time() - started_at_ts))
        elapsed_min = elapsed_sec // 60
        parts = [
            f"Проверка всё ещё выполняется: {description}.",
            f"Прошло примерно {elapsed_min} мин." if elapsed_min else "Прошло меньше минуты.",
        ]
        if action == "updates-check":
            summary = self._updates_summary_text()
            if summary and "Нет данных проверки" not in summary:
                parts.append("Последний сохранённый результат:")
                parts.append(summary)
        return "\n".join(parts)

    def _start_updates_action_async(self, chat_id: int, action: str) -> None:
        action = action.strip().lower()
        with self._updates_action_lock:
            if self._updates_action_running:
                current_desc, _ = self._updates_action_description(self._updates_action_running)
                self._send_text(
                    chat_id,
                    "Операция обновлений уже выполняется.\n"
                    f"Сейчас идёт {current_desc}. Дождитесь результата.",
                )
                return
            self._updates_action_running = action

        description, eta = self._updates_action_description(action)
        started_at = self._local_time_text()
        started_at_ts = time.time()
        ack_text = (
            "Запрос принят.\n"
            f"Старт: {started_at}\n"
            f"Начинаю {description}. Это может занять {eta}.\n"
            "Результат пришлю отдельным сообщением."
        )
        if action == "updates-check":
            summary = self._updates_summary_text()
            if summary and "Нет данных проверки" not in summary:
                ack_text += f"\n\nПоследний сохранённый результат:\n{summary}"
        try:
            self._send_text(chat_id, ack_text)
        except Exception as exc:
            self._log("ERROR", f"Failed to deliver async updates ack ({action}): {exc}")
        self._log("ACTION", f"Starting async updates action={action} chat_id={chat_id}")

        def progress_notifier() -> None:
            time.sleep(90)
            with self._updates_action_lock:
                still_running = self._updates_action_running == action
            if not still_running:
                return
            try:
                self._send_text(chat_id, self._updates_progress_text(action, started_at_ts))
            except Exception as exc:
                self._log("ERROR", f"Failed to deliver async updates progress ({action}): {exc}")

        def worker() -> None:
            try:
                result = self._run_operator_action(action)
                finished_at = self._local_time_text()
                self._log("INFO", f"Updates action completed ({action}): {result[:1200]}")
                self._send_text(chat_id, f"Завершено: {finished_at}\n{result}")
            except Exception as exc:
                self._log("ERROR", f"Updates action handler failed ({action}): {exc}\n{traceback.format_exc()}")
                if isinstance(exc, subprocess.TimeoutExpired):
                    msg = f"Операция обновлений превысила лимит времени: {exc.timeout} сек."
                else:
                    msg = f"Не удалось выполнить операцию обновлений ({action}): {exc}"
                self._log("ERROR", f"Updates action delivery/result error ({action}): {msg}")
                self._send_text(chat_id, msg)
            finally:
                with self._updates_action_lock:
                    self._updates_action_running = ""

        threading.Thread(target=progress_notifier, name=f"updates-progress-{action}", daemon=True).start()
        threading.Thread(target=worker, name=f"updates-{action}", daemon=True).start()

    def _start_openvpn_cert_check_async(self, chat_id: int, search_term: str = "") -> None:
        with self._openvpn_cert_check_lock:
            if self._openvpn_cert_check_running:
                self._send_text(
                    chat_id,
                    "Проверка OpenVPN сертификатов уже выполняется. Дождитесь текущего результата.",
                )
                return
            self._openvpn_cert_check_running = True

        self._send_text(chat_id, "Запрос принят. Проверяю OpenVPN сертификаты, это может занять до 3-4 минут.")
        self._log("ACTION", f"Starting async OpenVPN cert check (chat_id={chat_id}, filter={search_term!r})")

        def worker() -> None:
            try:
                result = self._run_openvpn_cert_check_codex_exec(search_term)
                self._send_text(chat_id, result)
            except Exception as exc:
                self._log("ERROR", f"OpenVPN cert check handler failed: {exc}")
                if isinstance(exc, subprocess.TimeoutExpired):
                    msg = f"Проверка OpenVPN сертификатов превысила лимит {self.openvpn_cert_check_timeout_sec} сек."
                else:
                    msg = f"Не удалось выполнить проверку OpenVPN сертификатов: {exc}"
                self._send_text(chat_id, msg)
            finally:
                with self._openvpn_cert_check_lock:
                    self._openvpn_cert_check_running = False

        threading.Thread(target=worker, name="ovpn-cert-check", daemon=True).start()

    def _start_openvpn_expiry_check_async(self, chat_id: int) -> None:
        with self._openvpn_expiry_check_lock:
            if self._openvpn_expiry_check_running:
                self._send_text(
                    chat_id,
                    "Проверка истекающих OpenVPN сертификатов уже выполняется. Дождитесь текущего результата.",
                )
                return
            self._openvpn_expiry_check_running = True

        self._send_text(chat_id, "Запрос принят. Проверяю истекающие OpenVPN сертификаты, это может занять до 1-2 минут.")
        self._log("ACTION", f"Starting async OpenVPN expiry check (chat_id={chat_id})")

        def worker() -> None:
            try:
                _, report = self._run_openvpn_expiry_codex_exec(self.openvpn_expiry_warn_days, expiring_only=True)
                self._send_text(chat_id, report)
            except Exception as exc:
                self._log("ERROR", f"OpenVPN expiry check handler failed: {exc}")
                if isinstance(exc, subprocess.TimeoutExpired):
                    msg = (
                        "Проверка истекающих OpenVPN сертификатов превысила лимит "
                        f"{self.openvpn_expiry_warn_timeout_sec} сек."
                    )
                else:
                    msg = f"Не удалось получить список истекающих OpenVPN сертификатов: {exc}"
                self._send_text(chat_id, msg)
            finally:
                with self._openvpn_expiry_check_lock:
                    self._openvpn_expiry_check_running = False

        threading.Thread(target=worker, name="ovpn-expiry-check", daemon=True).start()

    @staticmethod
    def _normalize_button_text(text: str) -> str:
        return re.sub(r"\s+", " ", (text or "").strip().lower())

    def _button_matches(self, text: str, primary: str, aliases: Tuple[str, ...] = ()) -> bool:
        normalized = self._normalize_button_text(text)
        candidates = [primary, *aliases]
        return any(normalized == self._normalize_button_text(candidate) for candidate in candidates)

    def _run_shell(self, cmd: str, timeout_sec: int = 180) -> Tuple[int, str]:
        p = subprocess.run(
            ["bash", "-lc", cmd],
            text=True,
            capture_output=True,
            timeout=timeout_sec,
            check=False,
        )
        out = (p.stdout or "") + (("\n" + p.stderr) if p.stderr else "")
        return p.returncode, out.strip()

    def _get_proxmox_guest_lock(self, kind: str, guest_id: str) -> str:
        if kind == "qemu":
            cmd = f"{shlex.quote(self.qm_bin)} config {shlex.quote(guest_id)}"
        else:
            cmd = f"{shlex.quote(self.pct_bin)} config {shlex.quote(guest_id)}"
        rc, out = self._run_shell(cmd, timeout_sec=60)
        if rc != 0:
            return ""
        for line in out.splitlines():
            stripped = line.strip()
            if stripped.startswith("lock:"):
                return stripped.split(":", 1)[1].strip()
        return ""

    def _has_live_proxmox_locking_process(self, kind: str, guest_id: str) -> bool:
        guest_pat = re.escape(str(guest_id))
        base_pat = (
            r"(pct|qm|vzdump)"
            r".*("
            r"snapshot|rollback|restore|backup|mount|delsnapshot|listsnapshot"
            r").*\b" + guest_pat + r"\b"
            r"|"
            r"(pct|qm|vzdump)"
            r".*\b" + guest_pat + r"\b.*("
            r"snapshot|rollback|restore|backup|mount|delsnapshot|listsnapshot"
            r")"
        )
        if kind == "lxc":
            base_pat += r"|lxc-usernsexec.*(/var/lib/lxc/" + guest_pat + r"/rootfs|/run/lxc/)"
        rc, out = self._run_shell(f"pgrep -af {shlex.quote(base_pat)}", timeout_sec=30)
        if rc != 0:
            return False
        lines = [line.strip() for line in out.splitlines() if line.strip()]
        return bool(lines)

    def _clear_stale_proxmox_lock_if_safe(self, kind: str, guest_id: str) -> Tuple[bool, str]:
        lock = self._get_proxmox_guest_lock(kind, guest_id)
        if not lock:
            return False, ""
        if self._has_live_proxmox_locking_process(kind, guest_id):
            return False, f"guest {kind}:{guest_id} lock={lock}, live locking process detected"
        unlock_bin = self.qm_bin if kind == "qemu" else self.pct_bin
        rc, out = self._run_shell(
            f"{shlex.quote(unlock_bin)} unlock {shlex.quote(guest_id)}",
            timeout_sec=60,
        )
        if rc != 0:
            raise RuntimeError(
                f"Не удалось снять stale lock `{lock}` с {kind}:{guest_id}: {out[-2000:]}"
            )
        self._log("WARN", f"Cleared stale Proxmox lock for {kind}:{guest_id}: {lock}")
        return True, lock

    def _run_ai_user_shell(self, cmd: str, timeout_sec: int = 180) -> Tuple[int, str]:
        if self.ai_exec_user:
            cmd = f"sudo -u {shlex.quote(self.ai_exec_user)} bash -lc {shlex.quote(cmd)}"
        return self._run_shell(cmd, timeout_sec=timeout_sec)

    def _run_codex_exec_prompt(self, prompt: str, timeout_sec: int, model: Optional[str] = None) -> Tuple[int, str, str]:
        with tempfile.NamedTemporaryFile("w+", encoding="utf-8", delete=False) as tmp:
            tmp_path = tmp.name
        os.chmod(tmp_path, 0o666)
        selected_model = (model or self.codex_model).strip() or self.codex_model

        cmd = (
            f"cd {shlex.quote(self.ai_chat_workdir)} && "
            f"PFSENSE_MCP_BEARER={shlex.quote(os.getenv('PFSENSE_MCP_BEARER', ''))} "
            f"codex exec --ephemeral --skip-git-repo-check "
            f"--model {shlex.quote(selected_model)} "
            f"-C {shlex.quote(self.ai_chat_workdir)} "
            f"-s {shlex.quote(self.ai_chat_sandbox)} "
            f"--color never -o {shlex.quote(tmp_path)} "
            f"{shlex.quote(prompt)}"
        )
        try:
            rc, out = self._run_ai_user_shell(cmd, timeout_sec=timeout_sec)
            try:
                with open(tmp_path, "r", encoding="utf-8") as f:
                    reply = f.read().strip()
            except FileNotFoundError:
                reply = ""
            return rc, out, reply
        finally:
            try:
                os.unlink(tmp_path)
            except FileNotFoundError:
                pass

    @staticmethod
    def _is_model_capacity_error(output: str) -> bool:
        lowered = (output or "").lower()
        return "selected model is at capacity" in lowered

    def _sanitize_operator_reply(self, text: str) -> str:
        cleaned = (text or "").strip()
        if not cleaned:
            return cleaned

        replacements = (
            (r"\bAI\b", ""),
            (r"\bCodex\b", ""),
            (r"\bMCP\b", ""),
            (r"\bTelegram-бот[аеуыом]*\b", "поддержке"),
            (r"\bбот[аеуыом]*\b", ""),
            (r"\bассистент[а-я]*\b", "инженер"),
            (r"\bмодель[а-я]*\b", ""),
            (r"\bавтоматизац[а-я]*\b", ""),
            (r"\bнейросет[а-я]*\b", ""),
            (r"\bискусственн(?:ый|ого|ому|ым|ом)? интеллект[а-я]*\b", ""),
            (r"\bя как AI\b", "я"),
            (r"\bя как ассистент\b", "я"),
            (r"\bв этом чате\b", "здесь"),
        )
        for pattern, replacement in replacements:
            cleaned = re.sub(pattern, replacement, cleaned, flags=re.IGNORECASE)

        cleaned = re.sub(r"[ \t]{2,}", " ", cleaned)
        cleaned = re.sub(r" ?\n ?", "\n", cleaned)
        cleaned = re.sub(r"\n{3,}", "\n\n", cleaned)
        return cleaned.strip()

    @staticmethod
    def _summarize_exec_error(output: str, rc: int) -> str:
        lowered = (output or "").lower()
        if "403 forbidden" in lowered or "unable to load site" in lowered:
            return "Сервис ответов временно недоступен. Повторите запрос чуть позже."
        if "selected model is at capacity" in lowered:
            return "Сервис ответов перегружен. Повторите запрос чуть позже."
        if "transport channel closed" in lowered or "unexpectedcontenttype" in lowered:
            return "Сервис ответов временно недоступен из-за сетевой ошибки. Повторите запрос чуть позже."
        return f"Обработка запроса завершилась с ошибкой.\nrc={rc}"

    def _run_ai_chat_codex_exec(self, operator_text: str) -> str:
        clean_text = operator_text.strip()
        if not clean_text:
            return "Пустой запрос."

        prompt = (
            f"{self.HUMAN_OPERATOR_STYLE}"
            "При работе с pfSense используй локальный MCP server `pfsense-enhanced`, если он доступен. "
            "pfSense разрешено использовать только в режиме чтения по умолчанию. "
            "Никогда не выполняй pfSense write-операции в этом пути: firewall rules, aliases, NAT, interfaces, routes, VPN, access lists, apply/reload config. "
            "Если оператор просит такое изменение, не выполняй его и явно направь в double-confirm flow для pfSense. "
            "Никогда не выполняй потенциально блокирующие изменения: firewall rules, NAT, interfaces, routes, VPN, access lists "
            "без прямой явной команды оператора и отдельного предупреждения о риске. "
            "Если запрос требует изменений, которые могут отрезать доступ, сначала предложи безопасный план и попроси явное подтверждение. "
            "Если можно ответить, проверить или сделать безопасную диагностику самостоятельно, делай это. "
            "Сообщение оператора:\n"
            f"{clean_text}"
        )

        self._log(
            "ACTION",
            f"Routing Telegram message to codex exec as {self.ai_exec_user} "
            f"(model={self.codex_model}): {clean_text[:200]}"
        )
        rc, out, reply = self._run_codex_exec_prompt(
            prompt,
            timeout_sec=self.ai_chat_timeout_sec,
            model=self.codex_model,
        )
        if not reply and rc != 0 and self._is_model_capacity_error(out):
            for fallback_model in self.codex_fallback_models:
                self._log(
                    "WARN",
                    f"Model {self.codex_model} is at capacity; retrying with fallback {fallback_model}"
                )
                rc, out, reply = self._run_codex_exec_prompt(
                    prompt,
                    timeout_sec=self.ai_chat_timeout_sec,
                    model=fallback_model,
                )
                if reply or rc == 0 or not self._is_model_capacity_error(out):
                    break
        if reply:
            if rc != 0:
                self._log("WARN", f"codex exec via {self.ai_exec_user} returned rc={rc} but produced final message")
            return self._sanitize_operator_reply(reply)
        if rc != 0:
            self._log("ERROR", f"codex exec via {self.ai_exec_user} failed rc={rc}: {out[-2000:]}")
            return self._summarize_exec_error(out, rc)
        return self._sanitize_operator_reply(out[-1500:]) or "Не удалось сформировать ответ."

    def _new_confirm_code(self) -> str:
        return "".join(secrets.choice("0123456789") for _ in range(6))

    def _pending_pfsense_change_expired(self) -> bool:
        ppc = self.state.pending_pfsense_change
        if not ppc:
            return False
        return int(time.time()) - ppc.created_ts > self.pfsense_change_confirm_ttl_sec

    def _expire_pending_pfsense_change_if_needed(self) -> None:
        if self._pending_pfsense_change_expired():
            self.state.pending_pfsense_change = None
            self.state.save()

    def _looks_like_pfsense_write_request(self, text: str) -> bool:
        lowered = text.lower()
        target_terms = (
            "pfsense", "pfsense", "firewall", "фаервол", "правил", "rule", "nat",
            "порт", "port forward", "alias", "алиас", "vpn", "маршрут", "route",
            "интерфейс", "interface", "gateway", "шлюз",
        )
        action_terms = (
            "add", "create", "update", "change", "modify", "edit", "delete", "remove",
            "apply", "open", "close", "allow", "deny", "block", "unblock", "enable", "disable",
            "добав", "созда", "измени", "обнов", "удали", "примени", "разреш", "запрет",
            "открой", "закрой", "включ", "выключ", "блок", "разблок",
        )
        return any(term in lowered for term in target_terms) and any(term in lowered for term in action_terms)

    def _start_pfsense_change_flow(self, operator_text: str) -> str:
        request_id = time.strftime("%Y%m%d-%H%M%S")
        self.state.pending_pfsense_change = PendingPfSenseChange(
            request_id=request_id,
            created_ts=int(time.time()),
            operator_request=operator_text.strip(),
            stage="awaiting_first_confirm",
            confirm_code=self._new_confirm_code(),
            first_confirmed_ts=0,
        )
        self.state.save()
        return (
            "Запрос на изменение pfSense принят, но не выполнен.\n"
            f"- request_id: {request_id}\n"
            f"- запрос: {operator_text.strip()}\n"
            "- Это потенциально опасное изменение, поэтому бот не будет выполнять его сразу.\n"
            f"- Для первого подтверждения нажмите \"{self.BTN_PFSENSE_CONFIRM}\" или отправьте `/pfsense_confirm`.\n"
            f"- Для отмены нажмите \"{self.BTN_PFSENSE_CANCEL}\" или отправьте `/pfsense_cancel`."
        )

    def _confirm_pfsense_change_stage_one(self) -> str:
        self._expire_pending_pfsense_change_if_needed()
        ppc = self.state.pending_pfsense_change
        if not ppc:
            return "Нет ожидающего изменения pfSense."
        if ppc.stage != "awaiting_first_confirm":
            return (
                "Первое подтверждение уже принято.\n"
                f"Для второго подтверждения отправьте `/pfsense_apply {ppc.confirm_code}`."
            )

        ppc.stage = "awaiting_second_confirm"
        ppc.first_confirmed_ts = int(time.time())
        self.state.save()
        return (
            "Первое подтверждение принято.\n"
            f"- request_id: {ppc.request_id}\n"
            f"- запрос: {ppc.operator_request}\n"
            "- Второе подтверждение должно быть отдельным действием.\n"
            f"- Для выполнения отправьте: `/pfsense_apply {ppc.confirm_code}`\n"
            f"- Код подтверждения действует {self.pfsense_change_confirm_ttl_sec} секунд с момента создания запроса."
        )

    def _cancel_pfsense_change(self) -> str:
        self.state.pending_pfsense_change = None
        self.state.save()
        return "Ожидающее изменение pfSense отменено."

    def _pending_openvpn_config_expired(self) -> bool:
        povpn = self.state.pending_openvpn_config
        if not povpn:
            return False
        return int(time.time()) - povpn.created_ts > self.openvpn_config_confirm_ttl_sec

    def _expire_pending_openvpn_config_if_needed(self) -> None:
        if self._pending_openvpn_config_expired():
            self.state.pending_openvpn_config = None
            self.state.save()

    def _sanitize_filename(self, value: str, suffix: str) -> str:
        cleaned = "".join(ch if ch.isalnum() or ch in ("-", "_", ".") else "_" for ch in value.strip())
        cleaned = cleaned.strip("._") or "openvpn-client"
        if not cleaned.endswith(suffix):
            cleaned += suffix
        return cleaned

    def _run_openvpn_cert_check_codex_exec(self, search_term: str = "") -> str:
        now, rows, considered = self._collect_openvpn_user_cert_rows(search_term=search_term)
        flt = search_term.strip().lower()
        rows_simple: List[Tuple[datetime, str]] = [(dt, line) for dt, line, _ in rows]
        rows_simple.sort(key=lambda x: x[0])
        lines = [x[1] for x in rows_simple]
        if not lines:
            if flt:
                return f"OpenVPN user-сертификаты по фильтру `{search_term.strip()}` не найдены."
            if considered == 0:
                return "OpenVPN user-сертификаты не найдены."
            return "OpenVPN user-сертификаты есть, но не удалось разобрать данные по срокам."

        expired_cnt = sum(1 for dt, _ in rows_simple if dt < now)
        # Count expiring in <=30 days explicitly for readable summary.
        expiring30_cnt = sum(1 for dt, _ in rows_simple if 0 <= (dt - now).days <= 30)
        header = (
            f"OpenVPN user-сертификаты: {len(lines)} шт."
            f"{f' (фильтр: {search_term.strip()})' if flt else ''}\n"
            f"- просрочено: {expired_cnt}\n"
            f"- истекает <=30 дн.: {expiring30_cnt}\n"
        )
        return header + "\n" + "\n".join(lines[:80])

    def _load_pfsense_readonly_env(self) -> Tuple[str, str, bool]:
        env_raw = Path(self.PFSENSE_ENV_PATH).read_text(encoding="utf-8")
        env_map: Dict[str, str] = {}
        for line in env_raw.splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            env_map[key.strip()] = value.strip()

        base_url = (env_map.get("PFSENSE_URL", "") or "").strip()
        api_key = (env_map.get("PFSENSE_API_KEY", "") or "").strip()
        verify_ssl = (env_map.get("VERIFY_SSL", "false") or "").strip().lower() in {"1", "true", "yes", "on"}
        if not base_url or not api_key:
            raise RuntimeError("Не найдены PFSENSE_URL/PFSENSE_API_KEY в .env.readonly")
        return base_url, api_key, verify_ssl

    def _collect_openvpn_user_cert_rows(
        self,
        search_term: str = "",
        warn_days: Optional[int] = None,
        expiring_only: bool = False,
    ) -> Tuple[datetime, List[Tuple[datetime, str, str]], int]:
        base_url, api_key, verify_ssl = self._load_pfsense_readonly_env()

        requests.packages.urllib3.disable_warnings()
        web = requests.Session()
        web.verify = verify_ssl
        web.trust_env = False
        headers = {"X-API-Key": api_key}

        certs_resp = web.get(f"{base_url}/api/v2/system/certificates", headers=headers, timeout=25)
        certs_resp.raise_for_status()
        certs = certs_resp.json().get("data") or []

        ovpn_resp = web.get(f"{base_url}/api/v2/vpn/openvpn/servers", headers=headers, timeout=25)
        ovpn_resp.raise_for_status()
        ovpn_servers = ovpn_resp.json().get("data") or []
        ovpn_carefs = {str(x.get("caref", "")) for x in ovpn_servers if x.get("caref")}

        now = datetime.now(timezone.utc)
        flt = search_term.strip().lower()
        limit_days = 30 if warn_days is None else max(0, int(warn_days))
        rows: List[Tuple[datetime, str, str]] = []
        considered = 0

        for cert in certs:
            cert_type = str(cert.get("type", "")).strip().lower()
            caref = str(cert.get("caref", "")).strip()
            descr = str(cert.get("descr", "")).strip()
            refid = str(cert.get("refid", "")).strip()
            crt = str(cert.get("crt", "")).strip()
            if cert_type != "user":
                continue
            if ovpn_carefs and caref not in ovpn_carefs:
                continue
            if not crt:
                continue

            with tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False) as tmp:
                tmp.write(crt)
                tmp_path = tmp.name
            try:
                parsed = ssl._ssl._test_decode_cert(tmp_path)
            finally:
                try:
                    os.unlink(tmp_path)
                except FileNotFoundError:
                    pass

            cn = ""
            for rdn in parsed.get("subject", []):
                for item in rdn:
                    if len(item) == 2 and item[0] == "commonName":
                        cn = item[1]
                        break
                if cn:
                    break
            not_after_raw = (parsed.get("notAfter", "") or "").strip()
            if not not_after_raw:
                continue
            not_after = datetime.strptime(not_after_raw, "%b %d %H:%M:%S %Y %Z").replace(tzinfo=timezone.utc)
            days_left = (not_after - now).days
            considered += 1

            search_blob = " ".join([cn, descr, refid]).lower()
            if flt and flt not in search_blob:
                continue
            if expiring_only and days_left > limit_days:
                continue

            if days_left < 0:
                state = f"ПРОСРОЧЕН {-days_left} дн."
            elif days_left <= limit_days:
                state = f"истекает через {days_left} дн."
            else:
                state = f"OK, {days_left} дн."
            name = cn or descr or refid or "unknown"
            line = (
                f"- {name}: {state}, до {not_after.strftime('%Y-%m-%d %H:%M UTC')} "
                f"(descr={descr or '-'}, refid={refid or '-'})"
            )
            signature_line = "|".join(
                [
                    name,
                    descr or "-",
                    refid or "-",
                    not_after.strftime("%Y-%m-%dT%H:%M:%SZ"),
                    str(days_left),
                ]
            )
            rows.append((not_after, line, signature_line))

        return now, rows, considered

    def _run_openvpn_expiry_codex_exec(
        self,
        warn_days: int,
        expiring_only: bool = True,
        timeout_sec: Optional[int] = None,
    ) -> Tuple[str, str]:
        del timeout_sec
        _, rows, considered = self._collect_openvpn_user_cert_rows(
            warn_days=warn_days,
            expiring_only=expiring_only,
        )
        rows.sort(key=lambda x: x[0])
        report_lines = [x[1] for x in rows]
        signature_payload = "\n".join(x[2] for x in rows)
        signature = f"sha256:{hashlib.sha256(signature_payload.encode('utf-8')).hexdigest()}"

        if not report_lines:
            if considered == 0:
                report = "OpenVPN user-сертификаты не найдены."
            else:
                report = (
                    f"Просроченных и истекающих в ближайшие {warn_days} дней "
                    "OpenVPN пользовательских сертификатов не найдено."
                )
            return signature, report

        report = (
            f"Просроченные и истекающие в ближайшие {warn_days} дней "
            f"OpenVPN user-сертификаты: {len(report_lines)} шт.\n\n"
            + "\n".join(report_lines[:80])
        )
        return signature, report

    def _sync_openvpn_expiry_warning(self) -> None:
        if not self.openvpn_expiry_warn_enabled:
            return
        try:
            signature, report = self._run_openvpn_expiry_codex_exec(
                self.openvpn_expiry_warn_days,
                expiring_only=True,
                timeout_sec=self.openvpn_expiry_warn_timeout_sec,
            )
        except Exception as exc:
            self._log("ERROR", f"OpenVPN expiry warning check failed: {exc}")
            return

        normalized_signature = signature or report
        if not report or "нет" in report.lower() and "истека" in report.lower():
            if self.state.last_openvpn_expiry_signature:
                self.state.last_openvpn_expiry_signature = ""
                self.state.save()
            return

        if normalized_signature == self.state.last_openvpn_expiry_signature:
            return

        self.state.last_openvpn_expiry_signature = normalized_signature
        self.state.save()
        self._notify(f"Предупреждение по OpenVPN сертификатам:\n\n{report}")

    def _start_openvpn_config_flow(self, common_name: str) -> str:
        cn = common_name.strip()
        if not cn:
            return "Укажите common name пользователя: `/openvpn_config USERNAME`."
        request_id = time.strftime("%Y%m%d-%H%M%S")
        self.state.pending_openvpn_config = PendingOpenVpnConfig(
            request_id=request_id,
            created_ts=int(time.time()),
            common_name=cn,
            stage="awaiting_first_confirm",
            confirm_code=self._new_confirm_code(),
            first_confirmed_ts=0,
        )
        self.state.save()
        return (
            "Запрос на новый OpenVPN конфиг принят, но не выполнен.\n"
            f"- request_id: {request_id}\n"
            f"- common_name: {cn}\n"
            "- Бот может сгенерировать/обновить пользовательский сертификат и экспортировать новый `.ovpn`, "
            "поэтому требуется двойное подтверждение.\n"
            f"- Для первого подтверждения нажмите \"{self.BTN_OVPN_CONFIG_CONFIRM}\" или отправьте `/openvpn_config_confirm`.\n"
            f"- Для отмены нажмите \"{self.BTN_OVPN_CONFIG_CANCEL}\" или отправьте `/openvpn_config_cancel`."
        )

    def _confirm_openvpn_config_stage_one(self) -> str:
        self._expire_pending_openvpn_config_if_needed()
        povpn = self.state.pending_openvpn_config
        if not povpn:
            return "Нет ожидающего запроса на OpenVPN конфиг."
        if povpn.stage != "awaiting_first_confirm":
            return (
                "Первое подтверждение уже принято.\n"
                f"Для второго подтверждения отправьте `/openvpn_config_apply {povpn.confirm_code}`."
            )
        povpn.stage = "awaiting_second_confirm"
        povpn.first_confirmed_ts = int(time.time())
        self.state.save()
        return (
            "Первое подтверждение OpenVPN-конфига принято.\n"
            f"- request_id: {povpn.request_id}\n"
            f"- common_name: {povpn.common_name}\n"
            f"- Для второго подтверждения отправьте: `/openvpn_config_apply {povpn.confirm_code}`\n"
            f"- Код действует {self.openvpn_config_confirm_ttl_sec} секунд с момента создания запроса."
        )

    def _cancel_openvpn_config(self) -> str:
        self.state.pending_openvpn_config = None
        self.state.save()
        return "Ожидающий запрос на OpenVPN конфиг отменён."

    def _pending_proxmox_selection_expired(self) -> bool:
        pending = self.state.pending_proxmox_selection
        if not pending:
            return False
        return int(time.time()) - pending.created_ts > self.proxmox_selection_ttl_sec

    def _expire_pending_proxmox_selection_if_needed(self) -> None:
        if self._pending_proxmox_selection_expired():
            self.state.pending_proxmox_selection = None
            self.state.save()

    def _pending_proxmox_restore_expired(self) -> bool:
        pending = self.state.pending_proxmox_restore
        if not pending:
            return False
        return int(time.time()) - pending.created_ts > self.proxmox_restore_confirm_ttl_sec

    def _expire_pending_proxmox_restore_if_needed(self) -> None:
        if self._pending_proxmox_restore_expired():
            self.state.pending_proxmox_restore = None
            self.state.save()

    def _discover_proxmox_targets(self) -> List[Dict]:
        rc, out = self._run_shell(
            f"{shlex.quote(self.pvesh_bin)} get /cluster/resources --type vm --output-format json",
            timeout_sec=60,
        )
        if rc != 0:
            raise RuntimeError(f"Не удалось получить список виртуальных узлов Proxmox: {out[-2000:]}")

        payload = json.loads(out)
        targets: List[Dict] = []
        for item in payload:
            kind = str(item.get("type", "")).strip().lower()
            guest_id = str(item.get("vmid", "")).strip()
            if kind not in {"lxc", "qemu"} or not guest_id:
                continue
            targets.append(
                {
                    "kind": kind,
                    "id": guest_id,
                    "name": str(item.get("name", "")).strip(),
                    "node": str(item.get("node", "")).strip(),
                    "status": str(item.get("status", "")).strip(),
                }
            )
        return sorted(targets, key=lambda item: (item["kind"], int(item["id"])))

    @staticmethod
    def _proxmox_target_label(target: Dict) -> str:
        kind_label = "CT" if target.get("kind") == "lxc" else "VM"
        guest_name = target.get("name") or "-"
        node = target.get("node") or "-"
        status = target.get("status") or "-"
        return f"{kind_label} {target.get('id')}: {guest_name} (node={node}, status={status})"

    def _proxmox_target_prompt(self, mode: str) -> str:
        targets = self._discover_proxmox_targets()
        if not targets:
            return "В Proxmox не найдено ни одного виртуального узла."

        action = (
            f"Для создания снапшота отправьте ID или имя нужного сервера.\n"
            f"Будет использован snapshot `{self.proxmox_manual_snapshot_name}` с заменой предыдущего."
            if mode == "snapshot"
            else f"Для восстановления отправьте ID или имя нужного сервера.\n"
                 f"Будет использован snapshot `{self.proxmox_manual_snapshot_name}`."
        )
        lines = [
            action,
            "Можно указывать в формате `200`, `lxc:200`, `qemu:100` или по имени.",
            "",
            "Доступные узлы:",
        ]
        lines.extend(f"- {self._proxmox_target_label(target)}" for target in targets[:60])
        return "\n".join(lines)

    def _resolve_proxmox_target(self, selector: str) -> Dict:
        raw = (selector or "").strip()
        if not raw:
            raise RuntimeError("Пустой идентификатор узла.")

        targets = self._discover_proxmox_targets()
        normalized = raw.lower()
        prefixed = re.match(r"^(lxc|ct|qemu|vm)\s*:\s*(\d+)$", normalized)
        if prefixed:
            kind = "lxc" if prefixed.group(1) in {"lxc", "ct"} else "qemu"
            guest_id = prefixed.group(2)
            for target in targets:
                if target["kind"] == kind and target["id"] == guest_id:
                    return target
            raise RuntimeError(f"Узел {kind}:{guest_id} не найден.")

        if normalized.isdigit():
            matches = [target for target in targets if target["id"] == normalized]
            if len(matches) == 1:
                return matches[0]
            if len(matches) > 1:
                raise RuntimeError(f"ID {normalized} неоднозначен. Укажите `lxc:{normalized}` или `qemu:{normalized}`.")

        exact_name = [target for target in targets if (target.get("name") or "").lower() == normalized]
        if len(exact_name) == 1:
            return exact_name[0]
        if len(exact_name) > 1:
            raise RuntimeError(f"Имя `{raw}` неоднозначно. Укажите ID узла.")

        partial = [target for target in targets if normalized in (target.get("name") or "").lower()]
        if len(partial) == 1:
            return partial[0]
        if len(partial) > 1:
            labels = ", ".join(self._proxmox_target_label(target) for target in partial[:5])
            raise RuntimeError(f"Найдено несколько узлов по `{raw}`: {labels}")
        raise RuntimeError(f"Узел `{raw}` не найден.")

    def _list_snapshot_names(self, kind: str, guest_id: str) -> List[str]:
        if kind == "qemu":
            rc, out = self._run_shell(
                f"{shlex.quote(self.qm_bin)} listsnapshot {shlex.quote(guest_id)}",
                timeout_sec=60,
            )
        else:
            rc, out = self._run_shell(
                f"{shlex.quote(self.pct_bin)} listsnapshot {shlex.quote(guest_id)}",
                timeout_sec=60,
            )
        if rc != 0:
            return []
        names: List[str] = []
        for line in out.splitlines():
            stripped = line.strip()
            if not stripped:
                continue
            # Support both ASCII (`->) and Unicode tree prefixes (├─, └─) in Proxmox output.
            cleaned = re.sub(r"^[`|+>\\s├└─-]+", "", stripped)
            if not cleaned:
                continue
            parts = cleaned.split()
            if not parts:
                continue
            candidate = parts[0].strip()
            if candidate and candidate.lower() not in {"name", "current", "root", "snapshot"}:
                names.append(candidate)
        return list(dict.fromkeys(names))

    def _snapshot_exists(self, kind: str, guest_id: str, snapshot: str) -> bool:
        return snapshot in self._list_snapshot_names(kind, guest_id)

    def _delete_snapshot_if_exists(self, kind: str, guest_id: str, snapshot: str) -> bool:
        if not self._snapshot_exists(kind, guest_id, snapshot):
            return False
        if kind == "qemu":
            cmd = (
                f"{shlex.quote(self.qm_bin)} delsnapshot {shlex.quote(guest_id)} "
                f"{shlex.quote(snapshot)} --force 1"
            )
        else:
            cmd = f"{shlex.quote(self.pct_bin)} delsnapshot {shlex.quote(guest_id)} {shlex.quote(snapshot)}"
        rc, out = self._run_shell(cmd, timeout_sec=300)
        if rc != 0:
            raise RuntimeError(f"Не удалось удалить предыдущий snapshot `{snapshot}`: {out[-2000:]}")
        return True

    def _create_manual_proxmox_snapshot(self, selector: str) -> str:
        target = self._resolve_proxmox_target(selector)
        snapshot = self.proxmox_manual_snapshot_name
        description = (
            "TSJ Guardian manual snapshot "
            + time.strftime("%Y-%m-%d %H:%M:%S")
        )
        cleared_lock, cleared_lock_name = self._clear_stale_proxmox_lock_if_safe(target["kind"], target["id"])
        deleted_old_snapshot = self._delete_snapshot_if_exists(target["kind"], target["id"], snapshot)
        if target["kind"] == "qemu":
            cmd = (
                f"{shlex.quote(self.qm_bin)} snapshot {shlex.quote(target['id'])} {shlex.quote(snapshot)} "
                f"--description {shlex.quote(description)} --vmstate 0"
            )
        else:
            cmd = (
                f"{shlex.quote(self.pct_bin)} snapshot {shlex.quote(target['id'])} {shlex.quote(snapshot)} "
                f"--description {shlex.quote(description)}"
            )
        rc, out = self._run_shell(cmd, timeout_sec=600)
        if rc != 0:
            lower_out = out.lower()
            if "locked" in lower_out:
                retried, retried_lock_name = self._clear_stale_proxmox_lock_if_safe(target["kind"], target["id"])
                if retried:
                    cleared_lock = True
                    cleared_lock_name = retried_lock_name or cleared_lock_name
                    rc, out = self._run_shell(cmd, timeout_sec=600)
                    lower_out = out.lower() if rc != 0 else ""
            # If proxmox still reports existing snapshot with same name, force one more delete+retry.
            if rc != 0 and ("already exists" in lower_out or "exists" in lower_out):
                deleted_old_snapshot = self._delete_snapshot_if_exists(target["kind"], target["id"], snapshot) or deleted_old_snapshot
                rc, out = self._run_shell(cmd, timeout_sec=600)
            if rc != 0:
                raise RuntimeError(f"Создание snapshot не удалось: {out[-2000:]}")
        message = (
            "Snapshot создан.\n"
            f"- узел: {self._proxmox_target_label(target)}\n"
            f"- guest_id: {target['id']} ({target['kind']})\n"
            f"- snapshot: {snapshot}\n"
            f"- deleted_old_snapshot: {'yes' if deleted_old_snapshot else 'no'}"
        )
        if cleared_lock:
            message += f"\n- stale lock `{cleared_lock_name}` был автоматически снят, так как живых mount/backup/snapshot-процессов не найдено."
        return message

    def _start_proxmox_restore_flow(self, selector: str) -> str:
        target = self._resolve_proxmox_target(selector)
        snapshot = self.proxmox_manual_snapshot_name
        if not self._snapshot_exists(target["kind"], target["id"], snapshot):
            raise RuntimeError(
                f"Для узла {self._proxmox_target_label(target)} не найден snapshot `{snapshot}`."
            )
        request_id = time.strftime("%Y%m%d-%H%M%S")
        self.state.pending_proxmox_restore = PendingProxmoxRestore(
            request_id=request_id,
            created_ts=int(time.time()),
            kind=target["kind"],
            guest_id=target["id"],
            guest_name=target.get("name", ""),
            node=target.get("node", ""),
            snapshot=snapshot,
            confirm_code=self._new_confirm_code(),
        )
        self.state.save()
        return (
            "Запрос на восстановление из snapshot принят, но ещё не выполнен.\n"
            f"- request_id: {request_id}\n"
            f"- узел: {self._proxmox_target_label(target)}\n"
            f"- snapshot: {snapshot}\n"
            "- Это опасная операция: текущее состояние узла будет заменено состоянием из snapshot.\n"
            f"- Для выполнения отправьте: `/proxmox_restore_apply {self.state.pending_proxmox_restore.confirm_code}`\n"
            "- Для отмены отправьте: `/proxmox_restore_cancel`."
        )

    def _cancel_proxmox_selection(self) -> str:
        self.state.pending_proxmox_selection = None
        self.state.save()
        return "Выбор узла Proxmox отменён."

    def _cancel_proxmox_restore(self) -> str:
        self.state.pending_proxmox_restore = None
        self.state.save()
        return "Ожидающее восстановление Proxmox отменено."

    def _apply_proxmox_restore(self, code: str) -> str:
        self._expire_pending_proxmox_restore_if_needed()
        pending = self.state.pending_proxmox_restore
        if not pending:
            return "Нет ожидающего восстановления Proxmox."
        if code.strip() != pending.confirm_code:
            return "Неверный код подтверждения восстановления Proxmox."
        if not self._snapshot_exists(pending.kind, pending.guest_id, pending.snapshot):
            self.state.pending_proxmox_restore = None
            self.state.save()
            return f"Snapshot `{pending.snapshot}` больше не найден, восстановление отменено."

        if pending.kind == "qemu":
            cmd = (
                f"{shlex.quote(self.qm_bin)} rollback {shlex.quote(pending.guest_id)} "
                f"{shlex.quote(pending.snapshot)} --start 1"
            )
        else:
            cmd = (
                f"{shlex.quote(self.pct_bin)} rollback {shlex.quote(pending.guest_id)} "
                f"{shlex.quote(pending.snapshot)} --start 1"
            )
        rc, out = self._run_shell(cmd, timeout_sec=1800)
        self.state.pending_proxmox_restore = None
        self.state.save()
        if rc != 0:
            return (
                "Восстановление из snapshot завершилось с ошибкой.\n"
                f"- узел: {pending.kind}:{pending.guest_id} {pending.guest_name or ''}\n"
                f"- snapshot: {pending.snapshot}\n"
                f"{out[-1800:]}"
            )
        return (
            "Восстановление из snapshot выполнено.\n"
            f"- узел: {pending.kind}:{pending.guest_id} {pending.guest_name or ''}\n"
            f"- snapshot: {pending.snapshot}\n"
            f"{out[-1500:]}"
        )

    def _extract_tag_block(self, text: str, begin: str, end: str) -> str:
        if begin not in text or end not in text:
            return ""
        return text.split(begin, 1)[1].split(end, 1)[0].strip()

    def _extract_openvpn_filename(self, text: str) -> str:
        marker = "OVPN_FILENAME:"
        if marker not in text:
            return ""
        line = text.split(marker, 1)[1].splitlines()[0].strip()
        return self._sanitize_filename(line or "openvpn-client", ".ovpn")

    def _run_openvpn_config_codex_exec(self, povpn: PendingOpenVpnConfig) -> Tuple[str, str, str]:
        prompt = (
            f"{self.HUMAN_OPERATOR_STYLE}"
            "Этот запуск имеет отдельное двойное подтверждение оператора на выпуск нового OpenVPN client config для одного пользователя. "
            "Разрешено выполнить только действия, необходимые для одного common name, и не более. "
            "Нужно: проверить пользовательский OpenVPN certificate, при необходимости сгенерировать или обновить его, "
            "затем экспортировать свежий OpenVPN client config для этого common name. "
            "Используй локальный MCP `pfsense-enhanced`, а если он read-only, разрешено использовать прямой pfSense API/SSH доступ из локального окружения только для этой подтверждённой операции. "
            "Верни ответ СТРОГО в формате без markdown:\n"
            "OVPN_FILENAME: <filename>.ovpn\n"
            "OVPN_SUMMARY_BEGIN\n"
            "<короткий отчёт>\n"
            "OVPN_SUMMARY_END\n"
            "OVPN_CONFIG_BEGIN\n"
            "<raw ovpn config>\n"
            "OVPN_CONFIG_END\n"
            f"common_name: {povpn.common_name}"
        )
        rc, out, reply = self._run_codex_exec_prompt(prompt, timeout_sec=self.ai_chat_timeout_sec)
        payload = reply or out
        summary = self._extract_tag_block(payload, "OVPN_SUMMARY_BEGIN", "OVPN_SUMMARY_END")
        config = self._extract_tag_block(payload, "OVPN_CONFIG_BEGIN", "OVPN_CONFIG_END")
        filename = self._extract_openvpn_filename(payload) or self._sanitize_filename(povpn.common_name, ".ovpn")
        if rc != 0 and not config:
            self._log("ERROR", f"OpenVPN config generation failed rc={rc}: {out[-2000:]}")
            raise RuntimeError(f"OpenVPN config generation failed rc={rc}")
        if not config:
            self._log("ERROR", f"OpenVPN config generation produced no config payload: {payload[-2000:]}")
            raise RuntimeError("OpenVPN config payload not found in response")
        return filename, self._sanitize_operator_reply(summary or f"Новый OpenVPN конфиг подготовлен для {povpn.common_name}."), config

    def _apply_openvpn_config(self, chat_id: int, code: str) -> None:
        self._expire_pending_openvpn_config_if_needed()
        povpn = self.state.pending_openvpn_config
        if not povpn:
            self._send_text(chat_id, "Нет ожидающего запроса на OpenVPN конфиг.")
            return
        if povpn.stage != "awaiting_second_confirm":
            self._send_text(chat_id, "Второе подтверждение пока недоступно. Сначала выполните первый шаг подтверждения.")
            return
        if code.strip() != povpn.confirm_code:
            self._send_text(chat_id, "Неверный код второго подтверждения OpenVPN-конфига.")
            return

        try:
            filename, summary, config = self._run_openvpn_config_codex_exec(povpn)
            self.api.send_document(
                chat_id,
                filename,
                config.encode("utf-8"),
                caption=summary[:900],
            )
            self._send_text(chat_id, f"OpenVPN конфиг отправлен как файл `{filename}`.")
        except Exception as exc:
            self._log("ERROR", f"Failed to deliver OpenVPN config: {exc}")
            self._send_text(chat_id, f"Не удалось подготовить или отправить OpenVPN конфиг: {exc}")
        finally:
            self.state.pending_openvpn_config = None
            self.state.save()

    def _run_pfsense_change_codex_exec(self, ppc: PendingPfSenseChange) -> str:
        prompt = (
            f"{self.HUMAN_OPERATOR_STYLE}"
            "Этот запуск имеет ОТДЕЛЬНОЕ ДВОЙНОЕ ПОДТВЕРЖДЕНИЕ оператора на выполнение одного изменения pfSense. "
            "Разрешено выполнить только тот конкретный запрос, который приведён ниже, и не более одного логически связанного изменения. "
            "Перед изменением кратко проверь текущий объект или правило. После изменения кратко проверь результат. "
            "Если локальный MCP `pfsense-enhanced` доступен только в read-only режиме, разрешено использовать прямой pfSense API/SSH доступ из локального окружения только для этого подтверждённого изменения. "
            "Никогда не выполняй дополнительные изменения сверх указанного запроса. "
            "Если запрос неоднозначен или опаснее, чем описано, не выполняй его и верни причину. "
            "Подтверждённый запрос оператора:\n"
            f"{ppc.operator_request}"
        )
        self._log("ACTION", f"Running approved pfSense change via codex exec as {self.ai_exec_user}: {ppc.request_id}")
        rc, out, reply = self._run_codex_exec_prompt(prompt, timeout_sec=self.ai_chat_timeout_sec)
        if reply:
            if rc != 0:
                self._log("WARN", f"Approved pfSense codex exec returned rc={rc} but produced final message")
            return self._sanitize_operator_reply(reply)
        self._log("ERROR", f"Approved pfSense codex exec failed rc={rc}: {out[-2000:]}")
        return self._sanitize_operator_reply(self._summarize_exec_error(out, rc))

    def _apply_pfsense_change(self, code: str) -> str:
        self._expire_pending_pfsense_change_if_needed()
        ppc = self.state.pending_pfsense_change
        if not ppc:
            return "Нет ожидающего изменения pfSense."
        if ppc.stage != "awaiting_second_confirm":
            return "Второе подтверждение пока недоступно. Сначала выполните первый шаг подтверждения."
        if code.strip() != ppc.confirm_code:
            return "Неверный код второго подтверждения pfSense."

        result = self._run_pfsense_change_codex_exec(ppc)
        self.state.pending_pfsense_change = None
        self.state.save()
        return f"Результат подтверждённого изменения pfSense:\n{result}"

    def _read_json(self, path: Path) -> Dict:
        if not path.exists():
            return {}
        try:
            with path.open("r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return {}

    def _updates_summary_text(self) -> str:
        payload = self._read_json(self.updates_status_file)
        if not payload:
            return "Нет данных проверки критичных и важных обновлений."
        lines = [
            "Проверка обновлений: "
            f"critical_total={payload.get('critical_total', 0)}, "
            f"important_total={payload.get('important_total', 0)}, "
            f"unsupported_total={payload.get('unsupported_total', 0)}",
        ]
        for node in payload.get("nodes", []):
            label = node.get("ctid") or node.get("vmid") or node.get("id")
            lines.append(
                f"- {str(node.get('kind', 'guest')).upper()} {label}: critical={len(node.get('critical', []))}, "
                f"important={len(node.get('important', []))}, "
                f"supported={node.get('supported')}, running={node.get('running')}, "
                f"error={node.get('error', '')}"
            )
        return "\n".join(lines)

    def _rollback_pending_count(self) -> int:
        payload = self._read_json(self.updates_rollback_file)
        return len(payload.get("pending_rollback", []))

    def _parse_log_level(self, output: str, level: str) -> List[str]:
        marker = f"[{level}]"
        return [line.strip() for line in output.splitlines() if marker in line]

    def _parse_failures(self, output: str) -> List[str]:
        return self._parse_log_level(output, "FAIL")

    def _parse_warnings(self, output: str) -> List[str]:
        return self._parse_log_level(output, "WARN")

    def _warning_signature(self, warnings: List[str]) -> str:
        return "\n".join(sorted(set(warnings)))

    def _filesystem_failures(self, failures: List[str]) -> List[str]:
        return [line for line in failures if "filesystem_usage" in line.lower()]

    def _has_filesystem_critical(self, failures: List[str]) -> bool:
        return bool(self._filesystem_failures(failures))

    def _suggestions_from_failures(self, failures: List[str]) -> List[str]:
        suggestions = []
        text = "\n".join(failures).lower()
        if "proxmox_api" in text:
            suggestions.append("Перезапустить pveproxy/pvedaemon/pve-cluster и проверить порт 8006.")
        if "pfsense_web" in text:
            suggestions.append("Проверить доступность pfSense 10.10.10.1:8443, перезапустить WebGUI/nginx.")
        if "pfsense_mcp" in text:
            suggestions.append("Проверить локальный pfsense-mcp-server.service, bearer token и endpoint 127.0.0.1:3010/mcp.")
        if "influxdb" in text:
            suggestions.append("Проверить контейнер InfluxDB и restart сервиса influxdb.")
        if "grafana" in text:
            suggestions.append("Проверить grafana-server и NO_PROXY для 10.10.10.0/24.")
        if "loki" in text or "alloy" in text:
            suggestions.append("Проверить LXC логов и restart сервисов loki/alloy.")
        if "filesystem_usage" in text:
            suggestions.append("Проверить самые большие каталоги: du -x /var /srv /home, журналы в /var/log и apt cache.")
            suggestions.append("Проверить давление по снапшотам/хранилищу Proxmox и решить: очистка, ротация или расширение диска.")
        if not suggestions:
            suggestions.append("Запустить расширенную диагностику: /run check")
        return suggestions

    def _warning_text(self, warnings: List[str]) -> str:
        lines = [
            "Предупреждение мониторинга.",
            "",
            "Найдены filesystem warning-события:",
            *[f"- {line}" for line in warnings],
            "",
            "Критический инцидент будет создан только при достижении critical-порога.",
        ]
        return "\n".join(lines)

    def _incident_text(self, failures: List[str], suggestions: List[str]) -> str:
        lines = [
            "Обнаружен инцидент в ТСЖ системе.",
            "",
            "Проблемы:",
            *[f"- {f}" for f in failures],
            "",
            "Предложенные варианты лечения:",
            *[f"- {s}" for s in suggestions],
            "",
            f"Если оператор не ответит в течение {self.operator_timeout // 60} минут, запущу эскалацию в тех.поддержку и автономный fallback сервера.",
            "Команды: /ack, /heal, /run check, /run support, /run fallback, /status",
        ]
        return "\n".join(lines)

    def _sync_warning_state(self, warnings: List[str]) -> None:
        signature = self._warning_signature(warnings) if warnings else ""
        if not warnings:
            if self.state.last_warning_signature:
                self.state.last_warning_signature = ""
                self.state.save()
            return

        if signature == self.state.last_warning_signature:
            return

        self.state.last_warning_signature = signature
        self.state.save()
        self._notify(self._warning_text(warnings))

    def _build_ai_prompt(self, pi: PendingIncident) -> str:
        prompt = (
            "Критичный инцидент ТСЖ системы. "
            f"Incident ID: {pi.incident_id}. "
            f"Failures: {' | '.join(pi.failures)}. "
            f"Suggestions: {' | '.join(pi.suggestions)}. "
        )
        if self._has_filesystem_critical(pi.failures):
            prompt += (
                "Особое внимание: критическое заполнение файловой системы. "
                "Нужно проверить крупнейшие каталоги, логи, apt cache, снапшоты и варианты освобождения места. "
            )
        prompt += (
            "pfSense write-операции в этом пути не разрешены: firewall rules, aliases, NAT, interfaces, routes, VPN и apply/reload config "
            "можно только диагностировать, но не изменять. "
            "Проведи диагностику и лечение самостоятельно в пределах безопасных локальных действий, дай краткий отчёт оператору. "
            f"{self.HUMAN_OPERATOR_STYLE}"
        )
        return prompt

    def _handle_check_cycle(self) -> None:
        rc, out, started = self._run_check_script_once(timeout_sec=240)
        if not started:
            self._log("WARN", "Skipping check cycle because previous check is still running")
            return
        failures = self._parse_failures(out)
        warnings = self._parse_warnings(out)
        if rc == 0 and not failures:
            if self.state.pending_incident:
                self._notify("Инцидент закрыт: система снова в норме.")
                self.state.pending_incident = None
            self._sync_warning_state(warnings)
            self.state.save()
            self._log("INFO", "Check OK")
            return

        if rc != 0 and not failures:
            self._log(
                "WARN",
                "Check command returned non-zero without explicit [FAIL] markers; suppressing empty incident",
            )
            self._notify(
                "Проверка мониторинга завершилась ошибкой запуска или парсинга без явных отказов сервисов. "
                "Критический инцидент не создаю; повторите /run check."
            )
            return

        self._sync_warning_state([])
        now = int(time.time())
        if not self.state.pending_incident:
            incident_id = time.strftime("%Y%m%d-%H%M%S")
            suggestions = self._suggestions_from_failures(failures)
            self.state.pending_incident = PendingIncident(
                incident_id=incident_id,
                created_ts=now,
                failures=failures,
                suggestions=suggestions,
                last_autoheal_ts=0,
                autoheal_attempts=0,
                operator_acked=False,
                escalated_to_ai=False,
                fallback_executed=False,
            )
            self._notify(self._incident_text(failures, suggestions))
        else:
            self.state.pending_incident.failures = failures or self.state.pending_incident.failures
            self.state.pending_incident.suggestions = self._suggestions_from_failures(
                self.state.pending_incident.failures
            )
        self.state.save()

        pi = self.state.pending_incident
        if (
            pi
            and self.fs_immediate_ai_on_critical
            and self._has_filesystem_critical(pi.failures)
            and not pi.escalated_to_ai
        ):
            ai_ok = self._escalate_to_ai()
            pi.escalated_to_ai = ai_ok
            self.state.save()
            if ai_ok:
                self._notify("Критическое заполнение ФС: выполнена немедленная эскалация.")
            else:
                self._notify("Критическое заполнение ФС: немедленная эскалация не удалась.")

        self._attempt_autoheal(force=False)

    def _attempt_autoheal(self, force: bool) -> bool:
        pi = self.state.pending_incident
        if not pi:
            return True

        now = int(time.time())
        if not force and pi.last_autoheal_ts and now - pi.last_autoheal_ts < self.retry_autoheal_sec:
            return False

        pi.last_autoheal_ts = now
        pi.autoheal_attempts += 1
        self.state.save()

        rc, out = self._run_shell(self.heal_script, timeout_sec=420)
        failures = self._parse_failures(out)
        if rc == 0 and not failures:
            msg = (
                f"Авто-лечение успешно (attempt={pi.autoheal_attempts}). "
                "Инцидент закрыт."
            )
            self._notify(msg)
            self.state.pending_incident = None
            self.state.save()
            self._log("INFO", msg)
            if self.exit_on_autoheal_success:
                self._notify("По сценарию: успешное авто-лечение, процесс завершается.")
                raise SystemExit(0)
            return True

        # Keep incident active and refresh details.
        pi.failures = failures or pi.failures
        pi.suggestions = self._suggestions_from_failures(pi.failures)
        self.state.save()
        self._notify(
            f"Авто-лечение неуспешно (attempt={pi.autoheal_attempts}). "
            f"Ожидаю реакцию оператора до {self.operator_timeout // 60} минут."
        )
        return False

    def _tmux_session_exists(self) -> bool:
        rc, _ = self._run_ai_user_shell(f"tmux has-session -t {shlex.quote(self.tmux_session)}", timeout_sec=15)
        return rc == 0

    def _escalate_to_ai_tmux(self, pi: PendingIncident) -> bool:
        if not self._tmux_session_exists():
            if self.tmux_create_if_missing:
                rc, out = self._run_ai_user_shell(
                    f"tmux new-session -d -s {shlex.quote(self.tmux_session)} {shlex.quote(self.tmux_start_cmd)}",
                    timeout_sec=20,
                )
                if rc != 0:
                    self._log("ERROR", f"Failed to create tmux session: {out}")
                    return False
            else:
                self._log("ERROR", f"tmux session '{self.tmux_session}' not found")
                return False

        prompt = self._build_ai_prompt(pi)
        safe = prompt.replace('"', '\\"')
        send_cmd = (
            f'tmux send-keys -t {shlex.quote(self.tmux_session)} "{safe}" C-m'
        )
        rc, out = self._run_ai_user_shell(send_cmd, timeout_sec=15)
        if rc != 0:
            self._log("ERROR", f"tmux escalation failed: {out}")
            return False
        self._log("ACTION", f"Escalated to AI via tmux:{self.tmux_session}")
        return True

    def _escalate_to_ai_codex_exec(self, pi: PendingIncident) -> bool:
        prompt = self._build_ai_prompt(pi)
        self._log("ACTION", f"Escalating incident to codex exec as {self.ai_exec_user}: {pi.incident_id}")
        rc, out, reply = self._run_codex_exec_prompt(prompt, timeout_sec=self.ai_chat_timeout_sec)
        if reply:
            self._notify(f"Отчёт по эскалации:\n{self._sanitize_operator_reply(reply)}")
            if rc != 0:
                self._log("WARN", f"Incident codex exec returned rc={rc} but produced final message")
            return True
        self._log("ERROR", f"Incident codex exec failed rc={rc}: {out[-2000:]}")
        return False

    def _escalate_to_ai(self) -> bool:
        if not self.enable_ai_escalation:
            return False
        pi = self.state.pending_incident
        if not pi:
            return True
        if self.ai_escalation_mode == "tmux":
            return self._escalate_to_ai_tmux(pi)
        return self._escalate_to_ai_codex_exec(pi)

    def _run_server_fallback(self) -> bool:
        if not self.enable_server_fallback:
            return False
        ok = True
        for cmd in self.server_fallback_commands:
            rc, out = self._run_shell(cmd, timeout_sec=480)
            if rc != 0:
                ok = False
                self._log("ERROR", f"Fallback command failed: {cmd}\n{out}")
            else:
                self._log("INFO", f"Fallback command OK: {cmd}")
        return ok

    def _evaluate_timeout_escalation(self) -> None:
        pi = self.state.pending_incident
        if not pi:
            return
        if pi.operator_acked:
            return
        now = int(time.time())
        if now - pi.created_ts < self.operator_timeout:
            return

        if not pi.escalated_to_ai:
            ai_ok = self._escalate_to_ai()
            pi.escalated_to_ai = ai_ok
            self.state.save()
            if ai_ok:
                self._notify("Оператор не ответил. Выполнена эскалация.")
            else:
                self._notify("Оператор не ответил. Эскалация не удалась.")

        if not pi.fallback_executed:
            fallback_ok = self._run_server_fallback()
            pi.fallback_executed = True
            self.state.save()
            if fallback_ok:
                self._notify("Сервер выполнил автономный fallback-план.")
            else:
                self._notify("Автономный fallback-план сервера выполнен с ошибками, требуется оператор.")

    def _cmd_help(self) -> str:
        return (
            "Помощь по кнопкам:\n"
            "\n"
            f"{self.BTN_STATUS}\n"
            "- Показывает текущее состояние системы и инцидентов.\n"
            "- Ничего не меняет, безопасно.\n"
            "\n"
            f"{self.BTN_CHECK}\n"
            "- Запускает проверку сервисов и заполнения файловых систем (Proxmox host, CT 200/201/202/203/205).\n"
            "- Ничего не перезапускает.\n"
            "\n"
            f"{self.BTN_AW_DLP_CHECK}\n"
            "- Проверяет AW-Rus и DLP по свежести bucket-данных и сегодняшнему worktime.\n"
            "- Формирует операторский итог OK/DEGRADED прямо в чате.\n"
            "\n"
            f"{self.BTN_HEAL}\n"
            "- Пробует автоматическое лечение проблем (рестарт нужных сервисов).\n"
            "- Для критичного заполнения ФС авто-очистка не выполняется, нужен разбор причины.\n"
            "- Используйте, если диагностика показала сбой.\n"
            "\n"
            f"{self.BTN_ACK}\n"
            "- Подтверждает, что оператор взял инцидент в работу.\n"
            "- После этого автоматическая эскалация по таймауту приостанавливается.\n"
            "\n"
            f"{self.BTN_RESOLVE}\n"
            "- Закрывает текущий инцидент вручную.\n"
            "- Используйте только если уверены, что проблема решена.\n"
            "\n"
            f"{self.BTN_AI}\n"
            "- Передаёт инцидент напрямую в тех.поддержку через `codex exec` для расширенной диагностики.\n"
            "- Legacy-режим через tmux включается только служебной переменной окружения.\n"
            "- Для критического переполнения ФС это выполняется автоматически сразу.\n"
            "- Полезно, если авто-лечение не помогло.\n"
            "\n"
            f"{self.BTN_AI_CHAT}\n"
            "- Любое свободное текстовое сообщение отправляется в тех.поддержку через `codex exec` от пользователя `codex`.\n"
            "- Этот путь используется для нормального диалога с тех.поддержкой и не зависит от tmux.\n"
            "- Кнопка эскалации использует тот же прямой запуск `codex exec`.\n"
            "\n"
            f"{self.BTN_OVPN_CERTS}\n"
            "- Проверяет сертификаты пользователей OpenVPN в режиме чтения.\n"
            "- Можно использовать кнопку или `/openvpn_certs [filter]`.\n"
            "\n"
            f"{self.BTN_OVPN_EXPIRING}\n"
            f"- Показывает просроченные и истекающие в ближайшие {self.openvpn_expiry_warn_days} дней OpenVPN сертификаты.\n"
            "- Можно использовать кнопку или `/openvpn_expiring`.\n"
            "\n"
            f"{self.BTN_OVPN_CONFIG}\n"
            "- Запускает flow на выпуск нового OpenVPN client config для пользователя.\n"
            "- Стартуйте командой `/openvpn_config USERNAME_OR_CN`.\n"
            "- После этого нужны два отдельных подтверждения.\n"
            "\n"
            f"{self.BTN_OVPN_CONFIG_CONFIRM}\n"
            "- Первый шаг подтверждения выпуска нового OpenVPN конфига.\n"
            "\n"
            f"{self.BTN_OVPN_CONFIG_CANCEL}\n"
            "- Отменяет ожидающий выпуск OpenVPN конфига.\n"
            "\n"
            f"{self.BTN_PFSENSE_CONFIRM}\n"
            "- Первый шаг подтверждения для опасного изменения pfSense.\n"
            "- После этого бот выдаёт одноразовый код для второго подтверждения.\n"
            "\n"
            f"{self.BTN_PFSENSE_CANCEL}\n"
            "- Отменяет ожидающее изменение pfSense.\n"
            "- Используйте, если передумали или запрос нужно сформулировать заново.\n"
            "\n"
            f"{self.BTN_FALLBACK}\n"
            "- Запускает аварийный серверный план восстановления.\n"
            "- Это ручной форсированный режим, применяйте осознанно.\n"
            "\n"
            f"{self.BTN_UPD_CHECK}\n"
            "- Проверяет наличие критичных и важных обновлений на виртуальных узлах Proxmox.\n"
            "- Автоматическая установка поддерживается для LXC; неподдерживаемые VM помечаются отдельно.\n"
            "- Ежедневная авто-проверка также выполняется в 03:00 МСК.\n"
            "\n"
            f"{self.BTN_UPD_INSTALL}\n"
            "- Создаёт запрос на ручную установку критичных и важных обновлений.\n"
            "- Само обновление не запускается без подтверждения.\n"
            "\n"
            f"{self.BTN_UPD_INSTALL_CONFIRM}\n"
            "- Подтверждает и запускает обновление вручную.\n"
            "- Перед обновлением создаётся снапшот узла.\n"
            "- После обновления выполняется проверка восстановления.\n"
            "\n"
            f"{self.BTN_UPD_ROLLBACK_CONFIRM}\n"
            "- Подтверждает откат к созданному снапшоту.\n"
            "- Доступно только после неуспешного ручного обновления.\n"
            "\n"
            f"{self.BTN_PM_SNAPSHOT}\n"
            "- Запускает выбор Proxmox VM/LXC для ручного snapshot.\n"
            f"- Используется snapshot `{self.proxmox_manual_snapshot_name}`.\n"
            "- Предыдущий одноимённый snapshot на выбранном узле удаляется и заменяется новым.\n"
            "\n"
            f"{self.BTN_PM_RESTORE}\n"
            "- Запускает выбор Proxmox VM/LXC для восстановления из ручного snapshot.\n"
            f"- Восстановление выполняется из `{self.proxmox_manual_snapshot_name}`.\n"
            "- После выбора узла требуется отдельное подтверждение одноразовым кодом.\n"
            "\n"
            "Рекомендация по порядку действий:\n"
            "1) Статус -> 2) Диагностика -> 3) Лечение -> 4) Тех.поддержка (если нужно).\n"
            "\n"
            "pfSense write-flow:\n"
            "1) Отправьте текст запроса на изменение.\n"
            "2) Подтвердите первым шагом.\n"
            "3) Отправьте `/pfsense_apply CODE` для второго подтверждения.\n"
            "\n"
            "OpenVPN config flow:\n"
            "1) Отправьте `/openvpn_config USERNAME_OR_CN`.\n"
            "2) Подтвердите первым шагом.\n"
            "3) Отправьте `/openvpn_config_apply CODE`.\n"
            "\n"
            "Proxmox snapshot flow:\n"
            "1) Нажмите кнопку создания/восстановления или используйте `/proxmox_snapshot TARGET`.\n"
            "2) Для восстановления после выбора узла отправьте `/proxmox_restore_apply CODE`.\n"
            "\n"
            "Резервные slash-команды: /status /check /aw_dlp_check /heal /ack /resolve /run ... /openvpn_certs [filter] /openvpn_expiring /openvpn_config USER /openvpn_config_confirm /openvpn_config_cancel /openvpn_config_apply CODE /pfsense_confirm /pfsense_cancel /pfsense_apply CODE /proxmox_snapshot TARGET /proxmox_restore TARGET /proxmox_restore_apply CODE /proxmox_restore_cancel /proxmox_selection_cancel"
        )

    def _cmd_status(self) -> str:
        pfsense_status = self._pfsense_security_status_lines()
        pi = self.state.pending_incident
        if not pi:
            ppc = self.state.pending_pfsense_change
            ppc_line = (
                f"- pending_pfsense_change: {ppc.request_id} stage={ppc.stage}"
                if ppc else
                "- pending_pfsense_change: none"
            )
            ovpn_warn_line = (
                f"- openvpn_expiry_warning_signature: set"
                if self.state.last_openvpn_expiry_signature else
                "- openvpn_expiry_warning_signature: none"
            )
            pps = self.state.pending_proxmox_selection
            pps_line = (
                f"- pending_proxmox_selection: mode={pps.mode}"
                if pps else
                "- pending_proxmox_selection: none"
            )
            ppr = self.state.pending_proxmox_restore
            ppr_line = (
                f"- pending_proxmox_restore: {ppr.kind}:{ppr.guest_id} snapshot={ppr.snapshot}"
                if ppr else
                "- pending_proxmox_restore: none"
            )
            povpn = self.state.pending_openvpn_config
            povpn_line = (
                f"- pending_openvpn_config: {povpn.request_id} cn={povpn.common_name} stage={povpn.stage}"
                if povpn else
                "- pending_openvpn_config: none"
            )
            return (
                "Статус: инцидентов нет.\n"
                f"{pfsense_status}\n"
                f"{ppc_line}\n"
                f"{ovpn_warn_line}\n"
                f"{pps_line}\n"
                f"{ppr_line}\n"
                f"{povpn_line}\n"
                f"- pending_update_install_confirm: {self.state.pending_update_install_confirm}\n"
                f"- pending_rollback_confirm: {self.state.pending_rollback_confirm}\n"
                f"- rollback_pending_items: {self._rollback_pending_count()}"
            )
        age = int(time.time()) - pi.created_ts
        ppc = self.state.pending_pfsense_change
        ppc_line = (
            f"- pending_pfsense_change: {ppc.request_id} stage={ppc.stage}"
            if ppc else
            "- pending_pfsense_change: none"
        )
        ovpn_warn_line = (
            f"- openvpn_expiry_warning_signature: set"
            if self.state.last_openvpn_expiry_signature else
            "- openvpn_expiry_warning_signature: none"
        )
        pps = self.state.pending_proxmox_selection
        pps_line = (
            f"- pending_proxmox_selection: mode={pps.mode}"
            if pps else
            "- pending_proxmox_selection: none"
        )
        ppr = self.state.pending_proxmox_restore
        ppr_line = (
            f"- pending_proxmox_restore: {ppr.kind}:{ppr.guest_id} snapshot={ppr.snapshot}"
            if ppr else
            "- pending_proxmox_restore: none"
        )
        povpn = self.state.pending_openvpn_config
        povpn_line = (
            f"- pending_openvpn_config: {povpn.request_id} cn={povpn.common_name} stage={povpn.stage}"
            if povpn else
            "- pending_openvpn_config: none"
        )
        return (
            f"Статус: активный инцидент {pi.incident_id}\n"
            f"{pfsense_status}\n"
            f"- возраст: {age}s\n"
            f"- autoheal attempts: {pi.autoheal_attempts}\n"
            f"- operator_acked: {pi.operator_acked}\n"
            f"- escalated_to_ai: {pi.escalated_to_ai}\n"
            f"- fallback_executed: {pi.fallback_executed}\n"
            f"- failures: {' | '.join(pi.failures)}\n"
            f"{ppc_line}\n"
            f"{ovpn_warn_line}\n"
            f"{pps_line}\n"
            f"{ppr_line}\n"
            f"{povpn_line}\n"
            f"- pending_update_install_confirm: {self.state.pending_update_install_confirm}\n"
            f"- pending_rollback_confirm: {self.state.pending_rollback_confirm}\n"
            f"- rollback_pending_items: {self._rollback_pending_count()}"
        )

    def _aw_rus_dlp_probe(self) -> Tuple[List[str], List[str]]:
        base = self.aw_rus_api_base.rstrip("/")
        worktime_base = self.aw_rus_worktime_base.rstrip("/")
        host = self.aw_rus_host
        now = datetime.now(timezone.utc)

        def bucket_age(bucket_id: str) -> Tuple[Optional[int], str]:
            try:
                r = requests.get(f"{base}/buckets/{bucket_id}", timeout=20)
                r.raise_for_status()
                data = r.json()
                end = ((data.get("metadata") or {}).get("end") or "").strip()
                if not end:
                    return None, "no-end"
                end_dt = datetime.fromisoformat(end.replace("Z", "+00:00")).astimezone(timezone.utc)
                age = int((now - end_dt).total_seconds())
                return age, end
            except Exception as exc:
                return None, f"error:{exc}"

        checks = [
            (f"aw-watcher-window_{host}", "watcher-window"),
            (f"aw-watcher-afk_{host}", "watcher-afk"),
            (f"aw-dlp-endpoint-signals_{host}", "dlp-endpoint"),
            (f"aw-file-operations_{host}", "dlp-fileops-host"),
            ("aw-file-operations_10.10.10.13", "dlp-fileops-server"),
        ]

        lines = ["Проверка AW-Rus + DLP:"]
        failures: List[str] = []
        for bucket_id, label in checks:
            age, tail = bucket_age(bucket_id)
            if age is None:
                lines.append(f"- {label}: FAIL ({tail})")
                failures.append(label)
                continue
            if age > self.aw_rus_stale_sec:
                lines.append(f"- {label}: STALE age={age}s end={tail}")
                failures.append(label)
            else:
                lines.append(f"- {label}: OK age={age}s end={tail}")

        try:
            r = requests.get(f"{worktime_base}/reports/worktime/today?format=csv", timeout=20)
            r.raise_for_status()
            csv_text = r.text
            target = self.aw_rus_primary_user.upper()
            active_sec = None
            session_rows = []
            for raw in csv_text.splitlines()[1:]:
                parts = [x.strip() for x in raw.split(",")]
                if len(parts) < 2:
                    continue
                try:
                    row_active_sec = int(float(parts[1]))
                except Exception:
                    row_active_sec = None
                session_rows.append(
                    {
                        "user": parts[0],
                        "active_seconds": row_active_sec,
                    }
                )
                if parts[0].upper() == target:
                    active_sec = row_active_sec
                    break
            if active_sec is None:
                lines.append(f"- worktime({target}): FAIL (user row not found)")
                failures.append("worktime")
            elif active_sec <= 0:
                non_machine_rows = [
                    row for row in session_rows
                    if row.get("user") and not row["user"].endswith("$")
                ]
                any_positive = any(
                    (row.get("active_seconds") or 0) > 0
                    for row in non_machine_rows
                )
                if any_positive:
                    lines.append(f"- worktime({target}): STALE active_seconds=0")
                    failures.append("worktime")
                else:
                    lines.append(f"- worktime({target}): OK active_seconds=0 (no active sessions)")
            else:
                lines.append(f"- worktime({target}): OK active_seconds={active_sec}")
        except Exception as exc:
            lines.append(f"- worktime: FAIL ({exc})")
            failures.append("worktime")

        return lines, failures

    def _aw_rus_dlp_status_text(self) -> str:
        lines, failures = self._aw_rus_dlp_probe()
        verdict = "OK" if not failures else f"DEGRADED ({', '.join(failures)})"
        lines.append(f"Итог: {verdict}")
        return "\n".join(lines)

    def _aw_rus_dlp_heal(self, targets: List[str]) -> Tuple[bool, List[str]]:
        base = self.aw_rus_api_base.rstrip("/")
        host = self.aw_rus_host
        now_iso = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
        report: List[str] = []

        bucket_defs = {
            f"aw-dlp-endpoint-signals_{host}": ("aw.dlp.endpoint.signal", "aw-dlp-endpoint-signals", host),
            f"aw-file-operations_{host}": ("aw.file.operation", "aw-file-operations", host),
            "aw-file-operations_10.10.10.13": ("aw.file.operation", "aw-file-operations", "10.10.10.13"),
        }
        map_fail_to_bucket = {
            "dlp-endpoint": f"aw-dlp-endpoint-signals_{host}",
            "dlp-fileops-host": f"aw-file-operations_{host}",
            "dlp-fileops-server": "aw-file-operations_10.10.10.13",
        }

        selected = []
        for key in targets:
            bid = map_fail_to_bucket.get(key)
            if bid and bid not in selected:
                selected.append(bid)

        if not selected:
            report.append("- heal: skipped (no DLP targets)")
            return True, report

        ok = True
        for bucket_id in selected:
            btype, client, hostname = bucket_defs[bucket_id]
            try:
                requests.post(
                    f"{base}/buckets/{bucket_id}",
                    json={"type": btype, "client": client, "hostname": hostname},
                    timeout=20,
                )
                event = {
                    "timestamp": now_iso,
                    "duration": 0,
                    "data": {"source": "tsj-guardian-heal", "signalType": "self_test", "hostname": hostname},
                }
                r = requests.post(f"{base}/buckets/{bucket_id}/events", json=[event], timeout=20)
                r.raise_for_status()
                report.append(f"- heal {bucket_id}: OK")
            except Exception as exc:
                ok = False
                report.append(f"- heal {bucket_id}: FAIL ({exc})")

        return ok, report

    def _aw_rus_worktime_heal(self) -> Tuple[bool, List[str]]:
        report: List[str] = []
        cmd = (self.aw_rus_worktime_heal_cmd or "").strip()
        if not cmd:
            report.append("- worktime-heal: skipped (command not configured)")
            return False, report
        try:
            rc, out = self._run_shell(cmd, timeout_sec=90)
            if rc != 0:
                tail = (out or "").strip().splitlines()[-1:] or [f"rc={rc}"]
                report.append(f"- worktime-heal: FAIL ({tail[0]})")
                return False, report
            report.append("- worktime-heal: restart command OK")
        except Exception as exc:
            report.append(f"- worktime-heal: FAIL ({exc})")
            return False, report

        time.sleep(2)
        try:
            probe_url = f"{self.aw_rus_worktime_base.rstrip('/')}/reports/worktime/today?format=csv"
            r = requests.get(probe_url, timeout=8)
            r.raise_for_status()
            report.append("- worktime-heal: probe OK")
            return True, report
        except Exception as exc:
            report.append(f"- worktime-heal: probe FAIL ({exc})")
            return False, report

    def _aw_rus_dlp_check_and_heal_text(self) -> str:
        before_lines, failures = self._aw_rus_dlp_probe()
        dlp_failures = [x for x in failures if x.startswith("dlp-")]
        worktime_failed = "worktime" in failures

        if not dlp_failures and not worktime_failed:
            verdict = "OK" if not failures else f"DEGRADED ({', '.join(failures)})"
            before_lines.append(f"Итог: {verdict}")
            return "\n".join(before_lines)

        out = []
        out.extend(before_lines)

        heal_ok = True
        if dlp_failures:
            dlp_ok, dlp_lines = self._aw_rus_dlp_heal(dlp_failures)
            heal_ok = heal_ok and dlp_ok
            out.append("- heal trigger: DLP degraded, starting remediation")
            out.extend(dlp_lines)

        if worktime_failed:
            wt_ok, wt_lines = self._aw_rus_worktime_heal()
            heal_ok = heal_ok and wt_ok
            out.append("- heal trigger: worktime degraded, starting remediation")
            out.extend(wt_lines)

        time.sleep(3)
        after_lines, after_failures = self._aw_rus_dlp_probe()
        verdict_after = "OK" if not after_failures else f"DEGRADED ({', '.join(after_failures)})"
        after_lines.append(f"Итог: {verdict_after}")

        out.append(f"- heal status: {'OK' if heal_ok else 'FAILED'}")
        out.append("После лечения:")
        out.extend(after_lines)
        return "\n".join(out)

    def _pfsense_security_status_lines(self) -> str:
        cmd = "/usr/bin/python3 /home/codex/infra-admin/scripts/pfsense_security_status.py"
        try:
            rc, out = self._run_shell(cmd, timeout_sec=40)
        except Exception as exc:
            return f"- pfsense_security: status unavailable ({exc})"

        lines = [line.strip() for line in (out or "").splitlines() if line.strip()]
        if rc != 0 or not lines:
            err = lines[-1] if lines else f"rc={rc}"
            return f"- pfsense_security: status unavailable ({err})"
        return "\n".join(lines)

    def _run_operator_action(self, action: str) -> str:
        action = action.strip().lower()
        if action == "check":
            rc, out, started = self._run_check_script_once(timeout_sec=240)
            if not started:
                return "/run check skipped: previous check is still running"
            summary = "\n".join(out.splitlines()[-12:])
            return f"/run check rc={rc}\n{summary}"
        if action in ("aw-dlp-check", "awrus-dlp-check"):
            return self._aw_rus_dlp_check_and_heal_text()
        if action == "heal":
            ok = self._attempt_autoheal(force=True)
            return f"/run heal result={'ok' if ok else 'failed'}"
        if action in ("ai", "support", "techsupport", "техподдержка", "тех.поддержка"):
            ok = self._escalate_to_ai()
            return f"/run support result={'ok' if ok else 'failed'}"
        if action == "fallback":
            ok = self._run_server_fallback()
            return f"/run fallback result={'ok' if ok else 'failed'}"
        if action == "updates-check":
            rc, out = self._run_shell(f"{self.updates_script} check", timeout_sec=1800)
            summary = self._updates_summary_text()
            return f"/run updates-check rc={rc}\n{summary}\n{out[-1200:]}"
        if action == "updates-install-request":
            self.state.pending_update_install_confirm = True
            self.state.save()
            return (
                "Подтвердите установку критичных и важных обновлений кнопкой "
                f"\"{self.BTN_UPD_INSTALL_CONFIRM}\".\n"
                "Автоматическая установка отключена; действие только ручное."
            )
        if action == "updates-install-confirm":
            if not self.state.pending_update_install_confirm:
                return (
                    "Нет ожидающего запроса на установку. "
                    f"Сначала нажмите \"{self.BTN_UPD_INSTALL}\"."
                )
            self.state.pending_update_install_confirm = False
            self.state.save()
            rc, out = self._run_shell(f"{self.updates_script} apply", timeout_sec=7200)
            rollback_items = self._rollback_pending_count()
            if rc == 0 and rollback_items == 0:
                return (
                    "Критичные и важные обновления установлены успешно.\n"
                    f"{out[-1800:]}"
                )
            self.state.pending_rollback_confirm = rollback_items > 0
            self.state.save()
            if rollback_items > 0:
                return (
                    "Установка обновлений завершилась с проблемами, система не полностью восстановилась.\n"
                    f"Для отката подтвердите кнопкой \"{self.BTN_UPD_ROLLBACK_CONFIRM}\".\n"
                    f"pending_rollback_items={rollback_items}\n{out[-1800:]}"
                )
            return f"Установка критичных и важных обновлений завершилась с ошибкой.\n{out[-1800:]}"
        if action == "updates-rollback-confirm":
            if not self.state.pending_rollback_confirm:
                return (
                    "Нет ожидающего отката. "
                    "Откат доступен только после неуспешного ручного обновления."
                )
            self.state.pending_rollback_confirm = False
            self.state.save()
            rc, out = self._run_shell(f"{self.updates_script} rollback", timeout_sec=5400)
            return f"Откат выполнен (rc={rc}).\n{out[-1800:]}"
        return "Неизвестное действие."

    def _process_message(self, upd: Dict) -> None:
        msg = upd.get("message") or {}
        chat = msg.get("chat") or {}
        chat_id = int(chat.get("id", 0))
        if chat_id not in self.allowed_chats:
            return

        text = (msg.get("text") or "").strip()
        self._log("INFO", f"Incoming Telegram message chat_id={chat_id}: {text!r}")
        self._expire_pending_pfsense_change_if_needed()
        self._expire_pending_openvpn_config_if_needed()
        self._expire_pending_proxmox_selection_if_needed()
        self._expire_pending_proxmox_restore_if_needed()
        self.state.last_operator_message_ts = int(time.time())
        self.state.save()

        if text.startswith("/start") or text.startswith("/help") or text == self.BTN_HELP:
            self._send_menu(chat_id, self._cmd_help())
            return
        if text.startswith("/status") or text == self.BTN_STATUS:
            self._send_text(chat_id, self._cmd_status())
            return
        if text.startswith("/check") or text == self.BTN_CHECK:
            self._send_text(chat_id, self._run_operator_action("check"))
            return
        if text.startswith("/aw_dlp_check") or text == self.BTN_AW_DLP_CHECK:
            self._send_text(chat_id, self._run_operator_action("aw-dlp-check"))
            return
        if text.startswith("/heal") or text == self.BTN_HEAL:
            self._send_text(chat_id, self._run_operator_action("heal"))
            return
        if text.startswith("/ack") or text == self.BTN_ACK:
            if self.state.pending_incident:
                self.state.pending_incident.operator_acked = True
                self.state.save()
                self._send_text(chat_id, "Инцидент подтвержден оператором, автоматическая эскалация приостановлена.")
            else:
                self._send_text(chat_id, "Активных инцидентов нет.")
            return
        if text.startswith("/resolve") or text == self.BTN_RESOLVE:
            self.state.pending_incident = None
            self.state.save()
            self._send_text(chat_id, "Инцидент закрыт вручную.")
            return
        if text == self.BTN_AI:
            self._send_text(chat_id, self._run_operator_action("ai"))
            return
        if self._button_matches(text, self.BTN_OVPN_CERTS, self.BTN_OVPN_CERTS_ALIASES):
            self._start_openvpn_cert_check_async(chat_id)
            return
        if text == self.BTN_OVPN_EXPIRING:
            self._start_openvpn_expiry_check_async(chat_id)
            return
        if text == self.BTN_OVPN_CONFIG:
            if not self.openvpn_config_enabled:
                self._send_text(chat_id, "Выпуск OpenVPN конфигов через бота отключён.")
                return
            self._send_menu(
                chat_id,
                "Для выпуска нового OpenVPN конфига отправьте команду `/openvpn_config USERNAME_OR_CN`.",
            )
            return
        if text == self.BTN_OVPN_CONFIG_CONFIRM or text.startswith("/openvpn_config_confirm"):
            self._send_text(chat_id, self._confirm_openvpn_config_stage_one())
            return
        if text == self.BTN_OVPN_CONFIG_CANCEL or text.startswith("/openvpn_config_cancel"):
            self._send_text(chat_id, self._cancel_openvpn_config())
            return
        if text.startswith("/openvpn_config_apply "):
            code = text.split(maxsplit=1)[1].strip()
            self._apply_openvpn_config(chat_id, code)
            return
        if text == self.BTN_PFSENSE_CONFIRM or text.startswith("/pfsense_confirm"):
            self._send_text(chat_id, self._confirm_pfsense_change_stage_one())
            return
        if text == self.BTN_PFSENSE_CANCEL or text.startswith("/pfsense_cancel"):
            self._send_text(chat_id, self._cancel_pfsense_change())
            return
        if text.startswith("/pfsense_apply "):
            code = text.split(maxsplit=1)[1].strip()
            self._send_text(chat_id, self._apply_pfsense_change(code))
            return
        if text == self.BTN_FALLBACK:
            self._send_text(chat_id, self._run_operator_action("fallback"))
            return
        if text == self.BTN_UPD_CHECK or text.startswith("/updates_check"):
            self._start_updates_action_async(chat_id, "updates-check")
            return
        if text.startswith("/openvpn_certs"):
            parts = text.split(maxsplit=1)
            self._start_openvpn_cert_check_async(chat_id, parts[1] if len(parts) > 1 else "")
            return
        if text.startswith("/openvpn_expiring"):
            self._start_openvpn_expiry_check_async(chat_id)
            return
        if text.startswith("/openvpn_config") and text.strip() == "/openvpn_config":
            self._send_text(chat_id, "Использование: `/openvpn_config USERNAME_OR_CN`.")
            return
        if text.startswith("/openvpn_config "):
            if not self.openvpn_config_enabled:
                self._send_text(chat_id, "Выпуск OpenVPN конфигов через бота отключён.")
                return
            common_name = text.split(maxsplit=1)[1].strip()
            self._send_text(chat_id, self._start_openvpn_config_flow(common_name))
            return
        if text == self.BTN_UPD_INSTALL or text.startswith("/updates_install"):
            self._send_text(chat_id, self._run_operator_action("updates-install-request"))
            return
        if text == self.BTN_UPD_INSTALL_CONFIRM or text.startswith("/updates_confirm"):
            self._start_updates_action_async(chat_id, "updates-install-confirm")
            return
        if text == self.BTN_UPD_ROLLBACK_CONFIRM or text.startswith("/rollback_confirm"):
            self._start_updates_action_async(chat_id, "updates-rollback-confirm")
            return
        if text == self.BTN_PM_SNAPSHOT or text.startswith("/proxmox_snapshot_select"):
            self.state.pending_proxmox_selection = PendingProxmoxSelection(
                mode="snapshot",
                created_ts=int(time.time()),
            )
            self.state.save()
            self._send_menu(chat_id, self._proxmox_target_prompt("snapshot"))
            return
        if text == self.BTN_PM_RESTORE or text.startswith("/proxmox_restore_select"):
            self.state.pending_proxmox_selection = PendingProxmoxSelection(
                mode="restore",
                created_ts=int(time.time()),
            )
            self.state.save()
            self._send_menu(chat_id, self._proxmox_target_prompt("restore"))
            return
        if text.startswith("/proxmox_selection_cancel"):
            self._send_text(chat_id, self._cancel_proxmox_selection())
            return
        if text.startswith("/proxmox_restore_cancel"):
            self._send_text(chat_id, self._cancel_proxmox_restore())
            return
        if text.strip() == "/proxmox_snapshot":
            self.state.pending_proxmox_selection = PendingProxmoxSelection(
                mode="snapshot",
                created_ts=int(time.time()),
            )
            self.state.save()
            self._send_menu(chat_id, self._proxmox_target_prompt("snapshot"))
            return
        if text.startswith("/proxmox_snapshot "):
            try:
                self._send_text(chat_id, self._create_manual_proxmox_snapshot(text.split(maxsplit=1)[1].strip()))
            except Exception as exc:
                self._send_text(chat_id, f"Не удалось создать snapshot: {exc}")
            return
        if text.strip() == "/proxmox_restore":
            self.state.pending_proxmox_selection = PendingProxmoxSelection(
                mode="restore",
                created_ts=int(time.time()),
            )
            self.state.save()
            self._send_menu(chat_id, self._proxmox_target_prompt("restore"))
            return
        if text.startswith("/proxmox_restore "):
            try:
                self._send_text(chat_id, self._start_proxmox_restore_flow(text.split(maxsplit=1)[1].strip()))
            except Exception as exc:
                self._send_text(chat_id, f"Не удалось подготовить восстановление: {exc}")
            return
        if text.startswith("/proxmox_restore_apply "):
            code = text.split(maxsplit=1)[1].strip()
            self._send_text(chat_id, self._apply_proxmox_restore(code))
            return
        if text.startswith("/run "):
            action = text.split(maxsplit=1)[1]
            self._send_text(chat_id, self._run_operator_action(action))
            return
        if self._button_matches(text, self.BTN_AI_CHAT, self.BTN_AI_CHAT_ALIASES):
            self._send_menu(chat_id, self._next_ai_chat_intro_text())
            return
        if self.state.pending_proxmox_selection and not text.startswith("/"):
            mode = self.state.pending_proxmox_selection.mode
            self.state.pending_proxmox_selection = None
            self.state.save()
            try:
                if mode == "snapshot":
                    self._send_text(chat_id, self._create_manual_proxmox_snapshot(text))
                else:
                    self._send_text(chat_id, self._start_proxmox_restore_flow(text))
            except Exception as exc:
                self._send_text(chat_id, f"Не удалось обработать выбор узла Proxmox: {exc}")
            return
        if self.pfsense_change_control_enabled and self._looks_like_pfsense_write_request(text):
            self._send_text(chat_id, self._start_pfsense_change_flow(text))
            return
        if self.ai_chat_enabled:
            self._send_text(chat_id, self._run_ai_chat_codex_exec(text))
            return
        self._send_menu(chat_id, "Не понял команду. Используйте кнопки меню.")

    def run(self) -> None:
        next_check_ts = 0
        while True:
            try:
                self._touch_heartbeat()
                updates = self.api.get_updates(self.state.last_update_id + 1, timeout=10)
                for upd in updates:
                    uid = int(upd.get("update_id", 0))
                    if uid > self.state.last_update_id:
                        self.state.last_update_id = uid
                    self._process_message(upd)
                self.state.save()

                now = time.time()
                if now >= next_check_ts:
                    self._handle_check_cycle()
                    if self.openvpn_expiry_warn_enabled and now >= self.next_openvpn_expiry_warn_ts:
                        self._sync_openvpn_expiry_warning()
                        self.next_openvpn_expiry_warn_ts = now + self.openvpn_expiry_warn_interval_sec
                    next_check_ts = now + self.check_interval

                self._evaluate_timeout_escalation()
                time.sleep(1)
            except SystemExit:
                raise
            except Exception as exc:
                self._log("ERROR", f"Main loop error: {exc}\n{traceback.format_exc()}")
                time.sleep(5)


def main() -> int:
    bot = TSJGuardianBot()
    bot.run()
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SystemExit:
        raise
    except Exception as exc:
        print(f"Fatal: {exc}", file=sys.stderr)
        raise
