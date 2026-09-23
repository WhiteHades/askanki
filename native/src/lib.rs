use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 65_536;
const MAX_HISTORY_ENTRIES: usize = 10_000;
const MAX_HISTORY_LOAD_ENTRIES: usize = 8;
const MAX_HISTORY_EVENT_BYTES: usize = 512 * 1_024;
const MAX_HISTORY_BYTES: u64 = 8 * 1_048_576;
pub const DEFAULT_RETENTION_DAYS: u64 = 30;

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub protocol: u16,
    pub request_id: String,
    pub action: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Accepted { protocol: u16, request_id: String },
    Output { request_id: String, text: String },
    ApprovalRequired { request_id: String, approval_id: String, summary: String },
    Completed { request_id: String, result: Value },
    Cancelled { request_id: String },
    Error { request_id: String, code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub id: String,
    pub note_id: String,
    pub role: String,
    pub content: String,
    pub created_at: u64,
}

#[derive(Debug, Deserialize)]
struct EchoPayload {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalPayload {
    summary: String,
}

#[derive(Debug, Deserialize)]
struct NotePayload {
    note_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HistoryAppendPayload {
    note_id: String,
    role: String,
    content: String,
}

struct HistoryStore {
    path: PathBuf,
    retention_days: u64,
    sequence: u64,
}

impl HistoryStore {
    fn new(path: PathBuf, retention_days: u64) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            let parent_existed = parent.exists();
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            if !parent_existed {
                set_private_permissions(parent, true)?;
            }
        }
        let mut store = Self { path, retention_days, sequence: 0 };
        store.recover_backup()?;
        let entries = store.read_entries()?;
        store.sequence = entries
            .iter()
            .filter_map(|entry| entry.id.rsplit_once('-'))
            .filter_map(|(_, sequence)| sequence.parse::<u64>().ok())
            .max()
            .unwrap_or_default();
        if store.path.exists() {
            set_private_permissions(&store.path, false)?;
        }
        Ok(store)
    }

    fn load(&mut self, note_id: &str, limit: usize) -> Result<Vec<HistoryEntry>, String> {
        self.prune()?;
        let limit = limit.clamp(1, MAX_HISTORY_LOAD_ENTRIES);
        let mut entries: Vec<_> =
            self.read_entries()?.into_iter().filter(|entry| entry.note_id == note_id).collect();
        if entries.len() > limit {
            entries.drain(..entries.len() - limit);
        }
        bounded_history_entries(entries)
    }

    fn append(&mut self, note_id: &str, role: &str, content: &str) -> Result<HistoryEntry, String> {
        validate_note_id(note_id)?;
        validate_role(role)?;
        validate_text(content)?;

        self.prune()?;
        let mut entries = self.read_entries()?;
        let created_at = now_seconds();
        self.sequence = self.sequence.saturating_add(1);
        let entry = HistoryEntry {
            id: format!("{created_at}-{}", self.sequence),
            note_id: note_id.to_owned(),
            role: role.to_owned(),
            content: content.to_owned(),
            created_at,
        };
        entries.push(entry.clone());
        if entries.len() > MAX_HISTORY_ENTRIES {
            let remove_count = entries.len() - MAX_HISTORY_ENTRIES;
            entries.drain(..remove_count);
        }
        self.write_entries(&entries)?;
        Ok(entry)
    }

    fn clear(&mut self, note_id: &str) -> Result<usize, String> {
        validate_note_id(note_id)?;
        let entries = self.read_entries()?;
        let retained: Vec<_> =
            entries.iter().filter(|entry| entry.note_id != note_id).cloned().collect();
        let removed = entries.len() - retained.len();
        if removed > 0 {
            self.write_entries(&retained)?;
        }
        Ok(removed)
    }

    fn prune(&mut self) -> Result<(), String> {
        if self.retention_days == 0 {
            return Ok(());
        }
        let cutoff = now_seconds().saturating_sub(self.retention_days.saturating_mul(86_400));
        let entries = self.read_entries()?;
        let retained: Vec<_> =
            entries.into_iter().filter(|entry| entry.created_at >= cutoff).collect();
        self.write_entries(&retained)
    }

    fn read_entries(&self) -> Result<Vec<HistoryEntry>, String> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        if metadata.len() > MAX_HISTORY_BYTES {
            return Err("history file exceeds the maximum size".to_owned());
        }
        let content = fs::read_to_string(&self.path).map_err(|error| error.to_string())?;
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|error| error.to_string()))
            .collect()
    }

    fn write_entries(&self, entries: &[HistoryEntry]) -> Result<(), String> {
        let mut retained = entries.to_vec();
        let mut content = serialize_entries(&retained)?;
        while content.len() as u64 > MAX_HISTORY_BYTES && retained.len() > 1 {
            retained.drain(..1);
            content = serialize_entries(&retained)?;
        }
        if content.len() as u64 > MAX_HISTORY_BYTES {
            return Err("history file exceeds the maximum size".to_owned());
        }
        let temporary = self.path.with_extension("jsonl.tmp");
        fs::write(&temporary, content).map_err(|error| error.to_string())?;
        set_private_permissions(&temporary, false)?;
        self.replace_file(&temporary)
    }

    fn recover_backup(&self) -> Result<(), String> {
        let backup = self.path.with_extension("jsonl.bak");
        if !backup.exists() {
            return Ok(());
        }
        if self.path.exists() {
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        } else {
            fs::rename(&backup, &self.path).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn replace_file(&self, temporary: &Path) -> Result<(), String> {
        let backup = self.path.with_extension("jsonl.bak");
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
        let had_previous = self.path.exists();
        if had_previous {
            fs::rename(&self.path, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(temporary, &self.path) {
            if had_previous && backup.exists() {
                let _ = fs::rename(&backup, &self.path);
            }
            let _ = fs::remove_file(temporary);
            return Err(error.to_string());
        }
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| error.to_string())?;
        }
        set_private_permissions(&self.path, false)
    }
}

#[derive(Default)]
pub struct Runtime {
    history: Option<HistoryStore>,
}

impl Runtime {
    pub fn new(history_path: Option<PathBuf>, retention_days: u64) -> Result<Self, String> {
        let history =
            history_path.map(|path| HistoryStore::new(path, retention_days)).transpose()?;
        Ok(Self { history })
    }

    pub fn process_line(&mut self, line: &str) -> Vec<Event> {
        if line.len() > MAX_REQUEST_BYTES {
            return vec![error_event(
                "unknown",
                "request_too_large",
                "request exceeds the maximum size",
            )];
        }
        match serde_json::from_str::<Request>(line) {
            Ok(request) => process_request_with_history(request, self.history.as_mut()),
            Err(error) => vec![error_event(
                "unknown",
                "invalid_json",
                &format!("could not parse request: {error}"),
            )],
        }
    }
}

pub fn process_line(line: &str) -> Vec<Event> {
    Runtime::default().process_line(line)
}

pub fn process_request(request: Request) -> Vec<Event> {
    process_request_with_history(request, None)
}

fn process_request_with_history(
    request: Request,
    history: Option<&mut HistoryStore>,
) -> Vec<Event> {
    let request_id = request.request_id;

    if request.protocol != PROTOCOL_VERSION {
        return vec![error_event(
            &request_id,
            "unsupported_protocol",
            "unsupported protocol version",
        )];
    }

    if request_id.is_empty() || request_id.len() > MAX_ID_BYTES {
        return vec![error_event(
            "unknown",
            "invalid_request_id",
            "request_id must be between 1 and 128 bytes",
        )];
    }

    let accepted = Event::Accepted { protocol: PROTOCOL_VERSION, request_id: request_id.clone() };

    match request.action.as_str() {
        "ping" => vec![accepted, Event::Completed { request_id, result: json!({"pong": true}) }],
        "echo" => match serde_json::from_value::<EchoPayload>(request.payload) {
            Ok(payload) if payload.text.len() <= MAX_TEXT_BYTES => vec![
                accepted,
                Event::Output { request_id: request_id.clone(), text: payload.text.clone() },
                Event::Completed { request_id, result: json!({"text": payload.text}) },
            ],
            Ok(_) => vec![error_event(
                &request_id,
                "text_too_large",
                "echo text exceeds the maximum size",
            )],
            Err(error) => vec![error_event(
                &request_id,
                "invalid_echo_payload",
                &format!("could not parse echo payload: {error}"),
            )],
        },
        "request_approval" => match serde_json::from_value::<ApprovalPayload>(request.payload) {
            Ok(payload) if payload.summary.len() <= MAX_TEXT_BYTES => vec![
                accepted,
                Event::ApprovalRequired {
                    approval_id: format!("approval-{request_id}"),
                    request_id,
                    summary: payload.summary,
                },
            ],
            Ok(_) => vec![error_event(
                &request_id,
                "summary_too_large",
                "approval summary exceeds the maximum size",
            )],
            Err(error) => vec![error_event(
                &request_id,
                "invalid_approval_payload",
                &format!("could not parse approval payload: {error}"),
            )],
        },
        "cancel" => vec![accepted, Event::Cancelled { request_id }],
        "shutdown" => {
            vec![accepted, Event::Completed { request_id, result: json!({"shutdown": true}) }]
        }
        "history_load" => history_action_load(accepted, request_id, request.payload, history),
        "history_append" => history_action_append(accepted, request_id, request.payload, history),
        "history_clear" => history_action_clear(accepted, request_id, request.payload, history),
        _ => vec![error_event(&request_id, "unknown_action", "unsupported request action")],
    }
}

fn history_action_load(
    accepted: Event,
    request_id: String,
    payload: Value,
    history: Option<&mut HistoryStore>,
) -> Vec<Event> {
    let (note_id, limit) = match parse_note_payload(payload) {
        Ok(values) => values,
        Err(error) => return vec![error_event(&request_id, "invalid_note_payload", &error)],
    };
    let Some(store) = history else {
        return vec![accepted, history_unavailable(&request_id)];
    };
    match store.load(&note_id, limit) {
        Ok(entries) => {
            vec![accepted, Event::Completed { request_id, result: json!({"entries": entries}) }]
        }
        Err(error) => vec![accepted, error_event(&request_id, "history_read_failed", &error)],
    }
}

fn history_action_append(
    accepted: Event,
    request_id: String,
    payload: Value,
    history: Option<&mut HistoryStore>,
) -> Vec<Event> {
    let payload = match serde_json::from_value::<HistoryAppendPayload>(payload) {
        Ok(payload) => payload,
        Err(error) => {
            return vec![error_event(
                &request_id,
                "invalid_history_payload",
                &format!("could not parse history payload: {error}"),
            )];
        }
    };
    let Some(store) = history else {
        return vec![accepted, history_unavailable(&request_id)];
    };
    match store.append(&payload.note_id, &payload.role, &payload.content) {
        Ok(entry) => {
            vec![accepted, Event::Completed { request_id, result: json!({"entry": entry}) }]
        }
        Err(error) => vec![accepted, error_event(&request_id, "history_write_failed", &error)],
    }
}

fn history_action_clear(
    accepted: Event,
    request_id: String,
    payload: Value,
    history: Option<&mut HistoryStore>,
) -> Vec<Event> {
    let note_id = match parse_note_payload(payload) {
        Ok((note_id, _)) => note_id,
        Err(error) => return vec![error_event(&request_id, "invalid_note_payload", &error)],
    };
    let Some(store) = history else {
        return vec![accepted, history_unavailable(&request_id)];
    };
    match store.clear(&note_id) {
        Ok(removed) => {
            vec![accepted, Event::Completed { request_id, result: json!({"removed": removed}) }]
        }
        Err(error) => vec![accepted, error_event(&request_id, "history_write_failed", &error)],
    }
}

fn parse_note_payload(payload: Value) -> Result<(String, usize), String> {
    let payload: NotePayload = serde_json::from_value(payload)
        .map_err(|error| format!("could not parse note payload: {error}"))?;
    validate_note_id(&payload.note_id)?;
    let limit = payload.limit.unwrap_or(MAX_HISTORY_LOAD_ENTRIES);
    if !(1..=MAX_HISTORY_LOAD_ENTRIES).contains(&limit) {
        return Err(format!("history limit must be between 1 and {MAX_HISTORY_LOAD_ENTRIES}"));
    }
    Ok((payload.note_id, limit))
}

fn validate_note_id(note_id: &str) -> Result<(), String> {
    if note_id.is_empty() || note_id.len() > MAX_ID_BYTES {
        return Err("note_id must be between 1 and 128 bytes".to_owned());
    }
    Ok(())
}

fn validate_role(role: &str) -> Result<(), String> {
    if matches!(role, "user" | "assistant" | "tool" | "error" | "system") {
        Ok(())
    } else {
        Err("unsupported history role".to_owned())
    }
}

fn validate_text(text: &str) -> Result<(), String> {
    if text.len() <= MAX_TEXT_BYTES {
        Ok(())
    } else {
        Err("history content exceeds the maximum size".to_owned())
    }
}

fn history_unavailable(request_id: &str) -> Event {
    error_event(request_id, "history_unavailable", "history storage is not configured")
}

fn bounded_history_entries(mut entries: Vec<HistoryEntry>) -> Result<Vec<HistoryEntry>, String> {
    loop {
        let size = serde_json::to_vec(&entries).map_err(|error| error.to_string())?.len();
        if size <= MAX_HISTORY_EVENT_BYTES {
            return Ok(entries);
        }
        if entries.len() <= 1 {
            return Err("history entry exceeds the event size limit".to_owned());
        }
        entries.drain(..1);
    }
}

fn serialize_entries(entries: &[HistoryEntry]) -> Result<String, String> {
    let content = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    Ok(if content.is_empty() { content } else { format!("{content}\n") })
}

#[cfg(unix)]
fn set_private_permissions(path: &Path, directory: bool) -> Result<(), String> {
    let mode = if directory { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_private_permissions(path: &Path, directory: bool) -> Result<(), String> {
    let _ = (path, directory);
    Ok(())
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn error_event(request_id: &str, code: &str, message: &str) -> Event {
    Event::Error {
        request_id: request_id.to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

impl Event {
    pub fn is_shutdown(&self) -> bool {
        matches!(
            self,
            Event::Completed { result, .. } if result.get("shutdown").and_then(Value::as_bool) == Some(true)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: &str, payload: Value) -> String {
        json!({
            "protocol": PROTOCOL_VERSION,
            "request_id": "test-request",
            "action": action,
            "payload": payload
        })
        .to_string()
    }

    fn history_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("askanki-{name}-{}", now_seconds()))
    }

    #[test]
    fn ping_returns_accepted_and_completed_events() {
        let events = process_line(&request("ping", json!({})));

        assert_eq!(
            events,
            vec![
                Event::Accepted {
                    protocol: PROTOCOL_VERSION,
                    request_id: "test-request".to_owned(),
                },
                Event::Completed {
                    request_id: "test-request".to_owned(),
                    result: json!({"pong": true}),
                },
            ]
        );
    }

    #[test]
    fn echo_streams_output_before_completion() {
        let events = process_line(&request("echo", json!({"text": "hello"})));

        assert!(matches!(events[1], Event::Output { ref text, .. } if text == "hello"));
        assert!(matches!(events[2], Event::Completed { .. }));
    }

    #[test]
    fn approval_request_is_explicit() {
        let events = process_line(&request("request_approval", json!({"summary": "write a file"})));

        assert!(matches!(
            events[1],
            Event::ApprovalRequired { ref summary, .. } if summary == "write a file"
        ));
    }

    #[test]
    fn cancellation_is_reported() {
        let events = process_line(&request("cancel", json!({})));

        assert_eq!(events[1], Event::Cancelled { request_id: "test-request".to_owned() });
    }

    #[test]
    fn invalid_json_returns_a_protocol_error() {
        let events = process_line("not-json");

        assert!(matches!(events[0], Event::Error { ref code, .. } if code == "invalid_json"));
    }

    #[test]
    fn unsupported_protocol_is_rejected() {
        let line = json!({
            "protocol": 99,
            "request_id": "test-request",
            "action": "ping",
            "payload": {}
        })
        .to_string();

        let events = process_line(&line);

        assert!(
            matches!(events[0], Event::Error { ref code, .. } if code == "unsupported_protocol")
        );
    }

    #[test]
    fn history_round_trip_is_note_scoped() {
        let path = history_path("round-trip");
        let mut runtime = Runtime::new(Some(path.clone()), DEFAULT_RETENTION_DAYS).unwrap();

        let append = runtime.process_line(&request(
            "history_append",
            json!({"note_id": "note-1", "role": "user", "content": "hello"}),
        ));
        assert!(matches!(append[1], Event::Completed { .. }));

        let load = runtime.process_line(&request("history_load", json!({"note_id": "note-1"})));
        let Event::Completed { result, .. } = &load[1] else {
            panic!("history load did not complete");
        };
        assert_eq!(result["entries"].as_array().map(Vec::len), Some(1));

        let other = runtime.process_line(&request("history_load", json!({"note_id": "note-2"})));
        let Event::Completed { result, .. } = &other[1] else {
            panic!("history load did not complete");
        };
        assert_eq!(result["entries"].as_array().map(Vec::len), Some(0));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn history_clear_removes_only_the_requested_note() {
        let path = history_path("clear");
        let mut runtime = Runtime::new(Some(path.clone()), DEFAULT_RETENTION_DAYS).unwrap();

        for note_id in ["note-1", "note-2"] {
            runtime.process_line(&request(
                "history_append",
                json!({"note_id": note_id, "role": "user", "content": "hello"}),
            ));
        }
        runtime.process_line(&request("history_clear", json!({"note_id": "note-1"})));
        let load = runtime.process_line(&request("history_load", json!({"note_id": "note-2"})));
        let Event::Completed { result, .. } = &load[1] else {
            panic!("history load did not complete");
        };
        assert_eq!(result["entries"].as_array().map(Vec::len), Some(1));

        let _ = fs::remove_file(path);
    }
}
