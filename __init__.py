from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

import aqt.editor
import aqt.reviewer
from aqt import gui_hooks, mw
from aqt.webview import WebContent

from askanki_bridge import (
    MAX_AGENT_PAYLOAD_BYTES,
    BridgeError,
    SidecarClient,
    _clean_agent_history,
    _clean_card_context,
    clean_config,
    completed_result,
    error_result,
    find_sidecar,
    validate_note_id,
    validate_role,
    validate_text,
)

MAX_BRIDGE_COMMAND_BYTES = 1_048_576
BRIDGE_PREFIX = "askanki:"
BRIDGE_SCRIPT = """
(() => {
  if (window.ankiAskAnki) return;
  window.ankiAskAnki = {
    call(action, payload = {}) {
      return new Promise((resolve) => {
        if (typeof pycmd !== "function") {
          resolve({ok: false, error: {code: "bridge_unavailable", message: "The Anki bridge is unavailable."}});
          return;
        }
        pycmd("askanki:" + JSON.stringify({action, payload}), resolve);
      });
    }
  };
})();
"""
SUPPORTED_CONTEXT_TYPES = (
    aqt.reviewer.Reviewer,
    aqt.editor.Editor,
    aqt.editor.NewEditor,
)
_client: SidecarClient | None = None
_client_signature: tuple[str, int, str, str] | None = None


def _addon_package() -> str:
    return mw.addonManager.addonFromModule(__name__)


def _addon_directory() -> Path:
    return Path(mw.addonManager.addonsFolder(_addon_package()))


def _user_files_directory() -> Path:
    user_files = _addon_directory() / "user_files"
    try:
        user_files.mkdir(parents=True, exist_ok=True)
    except OSError as error:
        raise BridgeError("user_files_unavailable", "could not create the local data directory") from error
    if os.name != "nt":
        try:
            os.chmod(user_files, 0o700)
        except OSError as error:
            raise BridgeError("history_permissions_failed", "could not secure the local history directory") from error
    return user_files


def _user_config_path() -> Path:
    return _user_files_directory() / "config.json"


def _read_user_config() -> dict[str, Any]:
    path = _user_config_path()
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return {}
    return value if isinstance(value, dict) else {}


def _write_user_config(config: dict[str, Any]) -> None:
    try:
        _user_config_path().write_text(json.dumps(config, indent=2) + "\n")
    except OSError as error:
        raise BridgeError("user_files_unavailable", "could not save local settings") from error


def get_runtime_config() -> dict[str, Any]:
    addon_config = mw.addonManager.getConfig(__name__) or {}
    try:
        user_config = _read_user_config()
    except BridgeError:
        user_config = {}
    cleaned = clean_config({**addon_config, **user_config})
    if user_config != cleaned:
        try:
            _write_user_config(cleaned)
        except BridgeError:
            pass
    meta = mw.addonManager.addonMeta(_addon_package())
    if meta.get("config"):
        mw.addonManager.writeConfig(__name__, {})
    return cleaned


def save_runtime_config(values: Any) -> dict[str, Any]:
    if not isinstance(values, dict):
        raise BridgeError("invalid_config", "config must be an object")
    merged = clean_config({**get_runtime_config(), **values})
    _write_user_config(merged)
    meta = mw.addonManager.addonMeta(_addon_package())
    if meta.get("config"):
        mw.addonManager.writeConfig(__name__, {})
    return merged


def _history_path() -> Path:
    return _user_files_directory() / "history.jsonl"


def _sidecar() -> SidecarClient:
    global _client, _client_signature
    config = get_runtime_config()
    addon_directory = _addon_directory()
    executable = find_sidecar(addon_directory)
    history_path = _history_path()
    signature = (
        str(executable),
        config["history_retention_days"],
        str(history_path),
        str(addon_directory),
    )
    if _client is not None and _client_signature == signature:
        return _client
    if _client is not None:
        _client.close()
    _client = SidecarClient(
        executable=executable,
        history_path=history_path,
        retention_days=config["history_retention_days"],
        working_directory=addon_directory,
    )
    _client_signature = signature
    return _client


def _ok(result: Any) -> dict[str, Any]:
    return {"ok": True, "result": result}


def _error(code: str, message: str) -> dict[str, Any]:
    return {"ok": False, "error": {"code": code, "message": message}}


def _context_note_id(context: Any) -> str | None:
    if isinstance(context, aqt.reviewer.Reviewer):
        value = getattr(getattr(context, "card", None), "nid", None)
    elif isinstance(context, aqt.editor.Editor):
        value = getattr(getattr(context, "note", None), "id", None)
    elif isinstance(context, aqt.editor.NewEditor):
        value = getattr(context, "nid", None)
    else:
        return None
    if value is None:
        return None
    try:
        number = int(value)
    except (TypeError, ValueError):
        return None
    return str(number) if number > 0 else None


def _context_note(request_payload: dict[str, Any], context: Any) -> str:
    expected = _context_note_id(context)
    if expected is None:
        raise BridgeError("no_active_note", "the current Anki note is not available")
    actual = validate_note_id(request_payload.get("note_id"))
    if actual != expected:
        raise BridgeError("note_context_mismatch", "the request does not match the current note")
    return actual


def _call_sidecar(action: str, payload: dict[str, Any]) -> dict[str, Any]:
    try:
        events = _sidecar().request(action, payload)
    except BridgeError as error:
        return _error(error.code, str(error))
    error = error_result(events)
    if error is not None:
        return _error(error["code"], error["message"])
    result = completed_result(events)
    if result is None:
        cancelled = any(event.get("event") == "cancelled" for event in events)
        return _ok({"cancelled": cancelled, "events": events})
    return _ok(result)


def _dispatch(payload: Any, context: Any) -> dict[str, Any]:
    if not isinstance(payload, dict):
        return _error("invalid_request", "request must be an object")
    action = payload.get("action")
    request_payload = payload.get("payload", {})
    if not isinstance(action, str) or not isinstance(request_payload, dict):
        return _error("invalid_request", "action and payload are required")

    if action == "config_get":
        return _ok({"config": get_runtime_config(), "note_id": _context_note_id(context)})
    if action == "config_save":
        try:
            return _ok({"config": save_runtime_config(request_payload.get("config"))})
        except BridgeError as error:
            return _error(error.code, str(error))
    if action == "sidecar_ping":
        return _call_sidecar("ping", {})
    if action == "agent_run":
        try:
            note_id = _context_note(request_payload, context)
            config = get_runtime_config()
            prompt = validate_text(request_payload.get("prompt"))
            card_context = _clean_card_context(request_payload.get("card_context", {}))
            history = _clean_agent_history(request_payload.get("history", []))
            payload = {
                "note_id": note_id,
                "prompt": prompt,
                "provider": config["provider"],
                "auto_fallback": config["auto_fallback"],
                "workspace": config["workspace"],
                "system_instruction": config["system_instruction"],
                "card_context": card_context,
                "history": history,
            }
            if len(json.dumps(payload, separators=(",", ":")).encode("utf-8")) > MAX_AGENT_PAYLOAD_BYTES:
                raise BridgeError("invalid_agent_payload", "agent request exceeds the maximum size")
        except BridgeError as error:
            return _error(error.code, str(error))
        return _call_sidecar("agent_run", payload)
    if action == "agent_cancel":
        try:
            note_id = _context_note(request_payload, context)
        except BridgeError as error:
            return _error(error.code, str(error))
        return _call_sidecar("agent_cancel", {"note_id": note_id})
    if action == "history_load":
        try:
            note_id = _context_note(request_payload, context)
            limit = request_payload.get("limit", 8)
            if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= 8:
                raise BridgeError("invalid_history_limit", "history limit must be between 1 and 8")
        except BridgeError as error:
            return _error(error.code, str(error))
        return _call_sidecar("history_load", {"note_id": note_id, "limit": limit})
    if action == "history_append":
        try:
            payload = {
                "note_id": _context_note(request_payload, context),
                "role": validate_role(request_payload.get("role")),
                "content": validate_text(request_payload.get("content")),
            }
        except BridgeError as error:
            return _error(error.code, str(error))
        return _call_sidecar("history_append", payload)
    if action == "history_clear":
        try:
            note_id = _context_note(request_payload, context)
        except BridgeError as error:
            return _error(error.code, str(error))
        return _call_sidecar("history_clear", {"note_id": note_id})
    return _error("unknown_action", "unsupported request action")


def _is_supported_context(context: Any) -> bool:
    return isinstance(context, SUPPORTED_CONTEXT_TYPES)


def _frontend_root() -> str:
    return "frontend/dist"


def inject_frontend(web_content: WebContent, context: Any) -> None:
    if not _is_supported_context(context):
        return

    addon_package = _addon_package()
    root = _frontend_root()
    dev_server = os.environ.get("ASKANKI_DEV_SERVER")
    if dev_server:
        dev_entry = "/src/main.tsx"
        web_content.head += f'<script type="module" src="{dev_server.rstrip("/")}{dev_entry}"></script>'
    else:
        web_content.js.append(f"/_addons/{addon_package}/{root}/assets/index.js")
        web_content.css.append(f"/_addons/{addon_package}/{root}/assets/index.css")

    web_content.head += f"<script>{BRIDGE_SCRIPT}</script>"


def inject_editor_runtime(editor: Any) -> None:
    if isinstance(editor, aqt.editor.Editor):
        return
    webview = getattr(editor, "web", None)
    if webview is None:
        return
    addon_package = _addon_package()
    root = _frontend_root()
    dev_server = os.environ.get("ASKANKI_DEV_SERVER")
    if dev_server:
        dev_entry = "/src/main.tsx"
        javascript_url = f"{dev_server.rstrip('/')}{dev_entry}"
        stylesheet_url = ""
    else:
        javascript_url = webview.webBundlePath(
            f"/_addons/{addon_package}/{root}/assets/index.js"
        )
        stylesheet_url = webview.webBundlePath(
            f"/_addons/{addon_package}/{root}/assets/index.css"
        )
    script = f"""
(() => {{
  if (!document.querySelector('script[data-askanki-frontend]')) {{
    const element = document.createElement('script');
    element.type = 'module';
    element.dataset.askankiFrontend = 'true';
    element.src = {json.dumps(javascript_url)};
    document.head.appendChild(element);
  }}
  if ({json.dumps(stylesheet_url)} && !document.querySelector('link[data-askanki-style]')) {{
    const element = document.createElement('link');
    element.rel = 'stylesheet';
    element.dataset.askankiStyle = 'true';
    element.href = {json.dumps(stylesheet_url)};
    document.head.appendChild(element);
  }}
  {BRIDGE_SCRIPT}
}})();
"""
    webview.eval(script)


def handle_pycmd(handled: tuple[bool, Any], command: str, context: Any) -> tuple[bool, Any]:
    if not isinstance(command, str) or not command.startswith(BRIDGE_PREFIX):
        return handled
    if not _is_supported_context(context):
        return handled
    try:
        command_size = len(command.encode("utf-8"))
    except UnicodeEncodeError:
        return True, _error("invalid_json", "request must be valid UTF-8")
    if command_size > MAX_BRIDGE_COMMAND_BYTES:
        return True, _error("request_too_large", "request exceeds the maximum size")
    try:
        payload = json.loads(command[len(BRIDGE_PREFIX) :])
    except json.JSONDecodeError:
        return True, _error("invalid_json", "request must be valid JSON")
    return True, _dispatch(payload, context)


get_runtime_config()
mw.addonManager.setWebExports(__name__, r"frontend/dist/.*")
gui_hooks.webview_will_set_content.append(inject_frontend)
gui_hooks.editor_did_init.append(inject_editor_runtime)
gui_hooks.webview_did_receive_js_message.append(handle_pycmd)
