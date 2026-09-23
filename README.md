# AskAnki

AskAnki is a local-first Anki add-on for card-aware AI chat in the reviewer and editor.

It uses user-installed OpenCode and Codex CLIs rather than a hosted API. The current rewrite moves the runtime to a Rust sidecar, keeps a minimal Python shim for Anki integration, and replaces the old Vue surface with a React chat interface.

## Development

The frontend and native runtime are built separately during the migration. See the local engineering specification and tickets for the current implementation phases.

## License

MIT. See `LICENSE`.
