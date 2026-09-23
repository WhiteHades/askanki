import io
import json
import tempfile
import unittest
from pathlib import Path

from askanki_bridge import (
    BridgeError,
    SidecarClient,
    clean_config,
    completed_result,
    error_result,
    find_sidecar,
    validate_role,
    validate_text,
)


class FakeProcess:
    def __init__(self, args, **kwargs):
        self.args = args
        self.kwargs = kwargs
        self.stdin = io.BytesIO()
        self.stdout = io.BytesIO(
            b'{"event":"accepted","protocol":1,"request_id":"request"}\n'
            b'{"event":"completed","request_id":"request","result":{"entries":[]}}\n'
        )

    def poll(self):
        return None

    def wait(self, timeout=None):
        return 0

    def terminate(self):
        return None

    def kill(self):
        return None


class BridgeTests(unittest.TestCase):
    def test_clean_config_removes_legacy_secrets_and_normalizes_values(self):
        config = clean_config(
            {
                "api_key": "legacy-secret",
                "model_name": "legacy-model",
                "provider": "unknown",
                "history_retention_days": True,
                "workspace": 12,
            }
        )

        self.assertEqual(config["provider"], "opencode")
        self.assertEqual(config["history_retention_days"], 30)
        self.assertEqual(config["workspace"], "")
        self.assertNotIn("api_key", config)
        self.assertNotIn("model_name", config)

    def test_untrusted_config_values_do_not_escape_normalization(self):
        config = clean_config({"provider": [], "workspace": {}})
        self.assertEqual(config["provider"], "opencode")
        self.assertEqual(config["workspace"], "")

        with self.assertRaises(BridgeError):
            validate_role([])
        with self.assertRaises(BridgeError):
            validate_text("é" * 65_537)

    def test_sidecar_request_uses_argument_array_without_shell(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "askanki-core"
            executable.touch()
            client = SidecarClient(
                executable=executable,
                history_path=Path(directory) / "history.jsonl",
                retention_days=30,
                popen_factory=FakeProcess,
                request_id_factory=lambda: "request",
            )

            events = client.request("history_load", {"note_id": "note-1"})

            self.assertEqual(completed_result(events), {"entries": []})
            process = client._process
            self.assertIsNotNone(process)
            self.assertFalse(process.kwargs["shell"])
            self.assertIsInstance(process.args, list)
            sent = json.loads(process.stdin.getvalue().decode())
            self.assertEqual(sent["action"], "history_load")
            self.assertEqual(sent["payload"], {"note_id": "note-1"})
            self.assertEqual(process.kwargs["cwd"], str(client.working_directory))

    def test_error_result_preserves_protocol_error(self):
        result = error_result(
            [{"event": "error", "code": "history_read_failed", "message": "failed"}]
        )

        self.assertEqual(result, {"code": "history_read_failed", "message": "failed"})

    def test_find_sidecar_prefers_a_packaged_candidate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packaged = root / "native" / "target" / "debug" / "askanki-core"
            packaged.parent.mkdir(parents=True)
            packaged.touch()

            self.assertEqual(find_sidecar(root), packaged)

    def test_missing_sidecar_is_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            client = SidecarClient(
                executable=Path(directory) / "missing",
                history_path=Path(directory) / "history.jsonl",
                retention_days=30,
            )

            with self.assertRaises(BridgeError) as context:
                client.start()

            self.assertEqual(context.exception.code, "sidecar_unavailable")


if __name__ == "__main__":
    unittest.main()
