# Configuration

AskAnki stores packaged defaults in `config.json`. User-specific settings are stored in `user_files/config.json`, and chat history is stored in `user_files/history.jsonl`; neither is source-controlled.

## Local agents

AskAnki uses locally installed OpenCode and Codex command-line programs. OpenCode is the default, with automatic fallback only when the selected local agent is unavailable or cannot start. The selected workspace is the only directory the agent may use without an explicit approval for a side effect. The current adapters keep write-capable tools disabled until the approval relay is available. Model and provider settings remain owned by the selected CLI.

## Chat history

Conversations are grouped by Anki note. Retention and deletion are controlled from the assistant settings.
