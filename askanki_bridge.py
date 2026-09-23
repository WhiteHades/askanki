from __future__ import annotations

import json
import os
import platform
import queue
import signal
import subprocess
import threading
import uuid
from collections.abc import Callable, Iterator, Mapping
from pathlib import Path
from typing import Any

DEFAULT_CONFIG = {
    "provider": "opencode",
    "workspace": "",
    "auto_fallback": True,
    "history_retention_days": 30,
    "system_instruction": (
        "You are a careful learning assistant integrated into Anki. "
        "Explain the current card clearly, use the language-learning profile when it is relevant, and be concise."
    ),
}
ALLOWED_PROVIDERS = frozenset({"opencode", "codex"})
ALLOWED_ROLES = frozenset({"user", "assistant", "tool", "error", "system"})
MAX_ID_LENGTH = 128
MAX_TEXT_LENGTH = 65_536
MAX_WORKSPACE_LENGTH = 4_096
MAX_INSTRUCTION_LENGTH = 16_384
MAX_EVENT_BYTES = 1_048_576
REQUEST_TIMEOUT_SECONDS = 10.0
TERMINAL_EVENTS = frozenset({"completed", "cancelled", "error", "approval_required"})


class BridgeError(RuntimeError):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def _byte_length(value: str) -> int:
    try:
        return len(value.encode("utf-8"))
    except UnicodeEncodeError:
        return 1_000_000_000


def clean_config(raw: Mapping[str, Any] | None) -> dict[str, Any]:
    source = raw if isinstance(raw, Mapping) else {}
    provider = source.get("provider", DEFAULT_CONFIG["provider"])
    if not isinstance(provider, str) or provider not in ALLOWED_PROVIDERS:
        provider = DEFAULT_CONFIG["provider"]

    workspace = source.get("workspace", DEFAULT_CONFIG["workspace"])
    if not isinstance(workspace, str) or _byte_length(workspace) > MAX_WORKSPACE_LENGTH:
        workspace = DEFAULT_CONFIG["workspace"]

    retention = source.get(
        "history_retention_days", DEFAULT_CONFIG["history_retention_days"]
    )
    if isinstance(retention, bool) or not isinstance(retention, int) or not 0 <= retention <= 3650:
        retention = DEFAULT_CONFIG["history_retention_days"]

    auto_fallback = source.get("auto_fallback", DEFAULT_CONFIG["auto_fallback"])
    if not isinstance(auto_fallback, bool):
        auto_fallback = DEFAULT_CONFIG["auto_fallback"]

    instruction = source.get(
        "system_instruction", DEFAULT_CONFIG["system_instruction"]
    )
    if not isinstance(instruction, str) or _byte_length(instruction) > MAX_INSTRUCTION_LENGTH:
        instruction = DEFAULT_CONFIG["system_instruction"]

    return {
        "provider": provider,
        "workspace": workspace,
        "auto_fallback": auto_fallback,
        "history_retention_days": retention,
        "system_instruction": instruction,
    }


def validate_note_id(value: Any) -> str:
    if not isinstance(value, str) or not 1 <= _byte_length(value) <= MAX_ID_LENGTH:
        raise BridgeError("invalid_note_id", "note_id must be a non-empty string")
    return value


def validate_role(value: Any) -> str:
    if not isinstance(value, str) or value not in ALLOWED_ROLES:
        raise BridgeError("invalid_role", "unsupported history role")
    return value


def validate_text(value: Any) -> str:
    if not isinstance(value, str) or _byte_length(value) > MAX_TEXT_LENGTH:
        raise BridgeError("invalid_text", "history content must be a bounded string")
    return value


class SidecarClient:
    def __init__(
        self,
        executable: Path,
        history_path: Path,
        retention_days: int,
        working_directory: Path | None = None,
        popen_factory: Callable[..., Any] | None = None,
        request_id_factory: Callable[[], str] | None = None,
    ) -> None:
        self.executable = executable
        self.history_path = history_path
        self.retention_days = retention_days
        self.working_directory = working_directory or history_path.parent
        self._popen_factory = popen_factory or subprocess.Popen
        self._request_id_factory = request_id_factory or (lambda: uuid.uuid4().hex)
        self._process: Any = None
        self._events: queue.Queue[bytes | None | BaseException] = queue.Queue()
        self._reader_thread: threading.Thread | None = None
        self._state_lock = threading.RLock()
        self._io_lock = threading.Lock()

    def start(self) -> None:
        with self._state_lock:
            if self._process is not None and self._process.poll() is None:
                return
            if not self.executable.is_file():
                raise BridgeError("sidecar_unavailable", "the local AskAnki runtime is not installed")
            self.working_directory.mkdir(parents=True, exist_ok=True)
            args = [
                str(self.executable),
                "--history-path",
                str(self.history_path),
                "--retention-days",
                str(self.retention_days),
            ]
            kwargs = {
                "stdin": subprocess.PIPE,
                "stdout": subprocess.PIPE,
                "stderr": subprocess.DEVNULL,
                "text": False,
                "bufsize": 0,
                "shell": False,
                "cwd": str(self.working_directory),
            }
            if os.name != "nt":
                kwargs["start_new_session"] = True
            try:
                process = self._popen_factory(args, **kwargs)
                self._process = process
                self._events = queue.Queue()
                self._reader_thread = threading.Thread(
                    target=self._read_events,
                    args=(process, self._events),
                    daemon=True,
                )
                self._reader_thread.start()
            except OSError as error:
                raise BridgeError(
                    "sidecar_start_failed",
                    f"could not start the local AskAnki runtime: {error}",
                ) from error

    def stream(self, action: str, payload: Mapping[str, Any] | None = None) -> Iterator[dict[str, Any]]:
        with self._io_lock:
            self.start()
            with self._state_lock:
                process = self._process
            if process is None:
                raise BridgeError("sidecar_start_failed", "the local AskAnki runtime did not start")
            request_id = self._request_id_factory()
            request = {
                "protocol": 1,
                "request_id": request_id,
                "action": action,
                "payload": dict(payload or {}),
            }
            encoded_request = (json.dumps(request, separators=(",", ":")) + "\n").encode()
            try:
                assert process.stdin is not None
                process.stdin.write(encoded_request)
                process.stdin.flush()
            except (BrokenPipeError, OSError, ValueError) as error:
                self._invalidate(process)
                raise BridgeError("sidecar_write_failed", "could not send to the local AskAnki runtime") from error

            with self._state_lock:
                events = self._events
            while True:
                try:
                    line = events.get(timeout=REQUEST_TIMEOUT_SECONDS)
                except queue.Empty as error:
                    self._invalidate(process)
                    raise BridgeError("sidecar_timeout", "the local AskAnki runtime timed out") from error
                if isinstance(line, BaseException):
                    self._invalidate(process)
                    raise BridgeError("sidecar_read_failed", "could not read from the local AskAnki runtime") from line
                if line is None or line == b"":
                    self._invalidate(process)
                    raise BridgeError("sidecar_closed", "the local AskAnki runtime exited unexpectedly")
                if len(line) > MAX_EVENT_BYTES or not line.endswith(b"\n"):
                    self._invalidate(process)
                    raise BridgeError("sidecar_protocol_error", "the local AskAnki runtime returned an oversized event")
                try:
                    event = json.loads(line.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError) as error:
                    self._invalidate(process)
                    raise BridgeError("sidecar_protocol_error", "the local AskAnki runtime returned invalid JSON") from error
                if (
                    not isinstance(event, dict)
                    or not isinstance(event.get("event"), str)
                    or event.get("request_id") != request_id
                ):
                    self._invalidate(process)
                    raise BridgeError("sidecar_protocol_error", "the local AskAnki runtime returned an invalid event")
                yield event
                if event["event"] in TERMINAL_EVENTS:
                    return

    def request(self, action: str, payload: Mapping[str, Any] | None = None) -> list[dict[str, Any]]:
        return list(self.stream(action, payload))

    @staticmethod
    def _read_events(
        process: Any,
        events: queue.Queue[bytes | None | BaseException],
    ) -> None:
        try:
            assert process.stdout is not None
            while True:
                line = process.stdout.readline(MAX_EVENT_BYTES + 1)
                events.put(line)
                if not line:
                    return
        except (OSError, ValueError) as error:
            events.put(error)

    def close(self) -> None:
        with self._state_lock:
            process = self._process
            self._process = None
            self._reader_thread = None
            self._events = queue.Queue()
        if process is None or process.poll() is not None:
            return

        if self._io_lock.acquire(timeout=0.2):
            try:
                if process.stdin is not None:
                    process.stdin.write(
                        (
                            json.dumps(
                                {
                                    "protocol": 1,
                                    "request_id": uuid.uuid4().hex,
                                    "action": "shutdown",
                                    "payload": {},
                                },
                                separators=(",", ":"),
                            )
                            + "\n"
                        ).encode()
                    )
                    process.stdin.flush()
                process.wait(timeout=1)
                return
            except (BrokenPipeError, OSError, ValueError, subprocess.TimeoutExpired):
                pass
            finally:
                self._io_lock.release()
        self._terminate(process)

    def _invalidate(self, process: Any) -> None:
        with self._state_lock:
            if self._process is process:
                self._process = None
        self._terminate(process)

    @staticmethod
    def _terminate(process: Any) -> None:
        if process.poll() is not None:
            return
        if os.name != "nt":
            try:
                os.killpg(os.getpgid(process.pid), signal.SIGTERM)
            except (AttributeError, ProcessLookupError, PermissionError):
                process.terminate()
        else:
            process.terminate()
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            process.kill()


def sidecar_filename() -> str:
    return "askanki-core.exe" if os.name == "nt" else "askanki-core"


def sidecar_candidates(addon_directory: Path) -> list[Path]:
    filename = sidecar_filename()
    machine = platform.machine().lower()
    target_names = {
        "x86_64": ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "x86_64-apple-darwin"],
        "amd64": ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "x86_64-apple-darwin"],
        "aarch64": ["aarch64-unknown-linux-gnu", "aarch64-pc-windows-msvc", "aarch64-apple-darwin"],
        "arm64": ["aarch64-unknown-linux-gnu", "aarch64-pc-windows-msvc", "aarch64-apple-darwin"],
    }.get(machine, [machine])
    candidates = [addon_directory / "native" / target / filename for target in target_names]
    candidates.extend(
        [
            addon_directory / "native" / "target" / "debug" / filename,
            addon_directory / "native" / "target" / "release" / filename,
            Path(__file__).parent / "native" / "target" / "debug" / filename,
        ]
    )
    result: list[Path] = []
    for candidate in candidates:
        if candidate not in result:
            result.append(candidate)
    return result


def find_sidecar(addon_directory: Path) -> Path:
    for candidate in sidecar_candidates(addon_directory):
        if candidate.is_file():
            return candidate
    raise BridgeError("sidecar_unavailable", "the local AskAnki runtime is not installed")


def completed_result(events: list[dict[str, Any]]) -> dict[str, Any] | None:
    for event in events:
        if event.get("event") == "completed" and isinstance(event.get("result"), dict):
            return event["result"]
    return None


def error_result(events: list[dict[str, Any]]) -> dict[str, str] | None:
    for event in events:
        if event.get("event") == "error":
            return {
                "code": str(event.get("code", "sidecar_error")),
                "message": str(event.get("message", "the local AskAnki runtime failed")),
            }
    return None
