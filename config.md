# Configuration

AskAnki stores default add-on settings in `config.json`. User-specific settings and chat history belong under Anki's `user_files/` directory and are not source files.

## Local agents

AskAnki uses locally installed OpenCode and Codex command-line programs. The selected workspace is the only directory the agent may use without an explicit approval for a side effect. Model and provider settings remain owned by the selected CLI.

## Chat history

Conversations are grouped by Anki note. Retention and deletion are controlled from the assistant settings.
