use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

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
const MAX_AGENT_EVENT_BYTES: usize = 1_048_576;
const MAX_AGENT_OUTPUT_BYTES: usize = 4 * 1_048_576;
const MAX_AGENT_HISTORY_ENTRIES: usize = 8;
const MAX_WORKSPACE_BYTES: usize = 4_096;
const AGENT_TIMEOUT: Duration = Duration::from_secs(120);
const AGENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
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

#[derive(Debug, Serialize, PartialEq, Clone)]
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

#[derive(Debug, Deserialize)]
struct AgentRunPayload {
    note_id: String,
    prompt: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    auto_fallback: bool,
    #[serde(default)]
    workspace: String,
    #[serde(default)]
    system_instruction: String,
    #[serde(default)]
    card_context: Value,
    #[serde(default)]
    history: Vec<Value>,
}

#[derive(Clone)]
struct ActiveRun {
    note_id: String,
    cancel: Arc<AtomicBool>,
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

pub struct Runtime {
    history: Option<HistoryStore>,
    active_runs: Arc<Mutex<HashMap<String, ActiveRun>>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self { history: None, active_runs: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl Runtime {
    pub fn new(history_path: Option<PathBuf>, retention_days: u64) -> Result<Self, String> {
        let history =
            history_path.map(|path| HistoryStore::new(path, retention_days)).transpose()?;
        Ok(Self { history, active_runs: Arc::new(Mutex::new(HashMap::new())) })
    }

    pub fn process_line(&mut self, line: &str) -> Vec<Event> {
        let (sender, receiver) = mpsc::channel();
        let mut events = self.process_line_with_sender(line, &sender);
        drop(sender);
        events.extend(receiver.iter());
        events
    }

    pub fn process_line_with_sender(
        &mut self,
        line: &str,
        sender: &mpsc::Sender<Event>,
    ) -> Vec<Event> {
        if line.len() > MAX_REQUEST_BYTES {
            return vec![error_event(
                "unknown",
                "request_too_large",
                "request exceeds the maximum size",
            )];
        }
        let request = match serde_json::from_str::<Request>(line) {
            Ok(request) => request,
            Err(error) => {
                return vec![error_event(
                    "unknown",
                    "invalid_json",
                    &format!("could not parse request: {error}"),
                )];
            }
        };
        if request.action == "agent_run" {
            return self.start_agent(request, sender);
        }
        if request.action == "agent_cancel" {
            return self.cancel_agent(request);
        }
        process_request_with_history(request, self.history.as_mut())
    }

    fn start_agent(&mut self, request: Request, sender: &mpsc::Sender<Event>) -> Vec<Event> {
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
        let accepted =
            Event::Accepted { protocol: PROTOCOL_VERSION, request_id: request_id.clone() };
        let payload: AgentRunPayload = match serde_json::from_value(request.payload) {
            Ok(payload) => payload,
            Err(error) => {
                return vec![
                    accepted,
                    error_event(
                        &request_id,
                        "invalid_agent_payload",
                        &format!("could not parse agent payload: {error}"),
                    ),
                ];
            }
        };
        if let Err(error) = validate_agent_payload(&payload) {
            return vec![accepted, error_event(&request_id, "invalid_agent_payload", &error)];
        }
        {
            let runs = self.active_runs.lock().expect("agent run lock poisoned");
            if !runs.is_empty() {
                return vec![
                    accepted,
                    error_event(
                        &request_id,
                        "agent_busy",
                        "another local agent run is already active",
                    ),
                ];
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let active = ActiveRun { note_id: payload.note_id.clone(), cancel: cancel.clone() };
        self.active_runs
            .lock()
            .expect("agent run lock poisoned")
            .insert(request_id.clone(), active);
        let runs = self.active_runs.clone();
        let sender = sender.clone();
        let provider = payload.provider.clone().unwrap_or_else(|| "opencode".to_owned());
        thread::spawn(move || {
            let result = run_agent(&request_id, &payload, &provider, cancel.clone(), &sender);
            if let Ok(mut active) = runs.lock() {
                active.remove(&request_id);
            }
            match result {
                Ok(AgentRunOutcome::Completed { text, provider, emitted_output }) => {
                    if !emitted_output {
                        let _ = sender.send(Event::Output {
                            request_id: request_id.clone(),
                            text: text.clone(),
                        });
                    }
                    let _ = sender.send(Event::Completed {
                        request_id: request_id.clone(),
                        result: json!({
                            "text": text,
                            "provider": provider,
                            "steps": [{"id": "agent", "label": "Local agent finished", "status": "succeeded"}],
                            "tools": []
                        }),
                    });
                }
                Ok(AgentRunOutcome::Cancelled) => {
                    let _ = sender.send(Event::Cancelled { request_id: request_id.clone() });
                }
                Err(error) => {
                    let _ = sender.send(error_event(&request_id, &error.code, &error.message));
                }
            }
        });
        vec![accepted]
    }

    fn cancel_agent(&mut self, request: Request) -> Vec<Event> {
        let request_id = request.request_id;
        if request.protocol != PROTOCOL_VERSION {
            return vec![error_event(
                &request_id,
                "unsupported_protocol",
                "unsupported protocol version",
            )];
        }
        let accepted =
            Event::Accepted { protocol: PROTOCOL_VERSION, request_id: request_id.clone() };
        let note_id = match serde_json::from_value::<NotePayload>(request.payload) {
            Ok(payload) if validate_note_id(&payload.note_id).is_ok() => payload.note_id,
            Ok(_) => {
                return vec![
                    accepted,
                    error_event(
                        &request_id,
                        "invalid_agent_payload",
                        "note_id must be between 1 and 128 bytes",
                    ),
                ];
            }
            Err(error) => {
                return vec![
                    accepted,
                    error_event(
                        &request_id,
                        "invalid_agent_payload",
                        &format!("could not parse cancel payload: {error}"),
                    ),
                ];
            }
        };
        let cancelled = self
            .active_runs
            .lock()
            .expect("agent run lock poisoned")
            .values()
            .find(|run| run.note_id == note_id)
            .map(|run| {
                run.cancel.store(true, Ordering::SeqCst);
                true
            })
            .unwrap_or(false);
        vec![accepted, Event::Completed { request_id, result: json!({"cancelled": cancelled}) }]
    }

    pub fn cancel_all(&self) {
        if let Ok(runs) = self.active_runs.lock() {
            for run in runs.values() {
                run.cancel.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn has_active_runs(&self) -> bool {
        self.active_runs.lock().map(|runs| !runs.is_empty()).unwrap_or(false)
    }
}

pub fn process_line(line: &str) -> Vec<Event> {
    Runtime::default().process_line(line)
}

pub fn process_request(request: Request) -> Vec<Event> {
    process_request_with_history(request, None)
}

struct AgentError {
    code: String,
    message: String,
}

enum AgentRunOutcome {
    Completed { text: String, provider: String, emitted_output: bool },
    Cancelled,
}

fn agent_error(code: &str, message: impl Into<String>) -> AgentError {
    AgentError { code: code.to_owned(), message: message.into() }
}

fn validate_agent_payload(payload: &AgentRunPayload) -> Result<(), String> {
    validate_note_id(&payload.note_id)?;
    validate_text(&payload.prompt)?;
    if payload.prompt.contains('\0') {
        return Err("agent prompt contains an invalid null byte".to_owned());
    }
    if payload.workspace.len() > MAX_WORKSPACE_BYTES || payload.workspace.contains('\0') {
        return Err("agent workspace is invalid".to_owned());
    }
    if payload
        .provider
        .as_deref()
        .is_some_and(|provider| provider != "opencode" && provider != "codex")
    {
        return Err("unsupported local agent".to_owned());
    }
    if payload.system_instruction.len() > MAX_TEXT_BYTES {
        return Err("agent instruction exceeds the maximum size".to_owned());
    }
    if payload.history.len() > MAX_AGENT_HISTORY_ENTRIES {
        return Err("agent history exceeds the maximum number of entries".to_owned());
    }
    if !payload.card_context.is_null() && !payload.card_context.is_object() {
        return Err("card context must be an object".to_owned());
    }
    let mut total = payload.prompt.len() + payload.system_instruction.len();
    for entry in &payload.history {
        let role =
            entry.get("role").and_then(Value::as_str).ok_or("history entry role is missing")?;
        let content = entry
            .get("content")
            .and_then(Value::as_str)
            .ok_or("history entry content is missing")?;
        validate_role(role)?;
        validate_text(content)?;
        total = total.saturating_add(content.len());
    }
    for key in ["text", "front", "back"] {
        if let Some(text) = payload.card_context.get(key).and_then(Value::as_str) {
            validate_text(text)?;
            total = total.saturating_add(text.len());
        }
    }
    for key in ["math", "code", "tables", "image_labels"] {
        if let Some(values) = payload.card_context.get(key).and_then(Value::as_array) {
            if values.len() > 32 {
                return Err("card context section exceeds the maximum number of items".to_owned());
            }
            for value in values {
                let text = value.as_str().ok_or("card context item is not text")?;
                validate_text(text)?;
                total = total.saturating_add(text.len());
            }
        }
    }
    if total > MAX_AGENT_OUTPUT_BYTES {
        return Err("agent request exceeds the maximum size".to_owned());
    }
    Ok(())
}

fn agent_workspace(workspace: &str) -> Result<PathBuf, AgentError> {
    let path = if workspace.trim().is_empty() {
        std::env::current_dir().map_err(|_| {
            agent_error("agent_workspace_unavailable", "the current workspace is unavailable")
        })?
    } else {
        PathBuf::from(workspace)
    };
    if !path.is_dir() {
        return Err(agent_error(
            "agent_workspace_unavailable",
            "the selected workspace is not a directory",
        ));
    }
    Ok(path)
}

fn build_agent_prompt(payload: &AgentRunPayload) -> String {
    let mut prompt = String::new();
    if !payload.system_instruction.trim().is_empty() {
        prompt.push_str("System instruction:\n");
        prompt.push_str(&payload.system_instruction);
        prompt.push_str("\n\n");
    }
    prompt.push_str("Current Anki card:\n");
    let card_text = payload
        .card_context
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("No card text is available.");
    prompt.push_str(card_text);
    for (label, key) in [
        ("Front", "front"),
        ("Back", "back"),
        ("Math", "math"),
        ("Code", "code"),
        ("Tables", "tables"),
        ("Image labels", "image_labels"),
    ] {
        if let Some(value) = payload.card_context.get(key) {
            let rendered = match value {
                Value::String(text) if !text.trim().is_empty() => text.clone(),
                Value::Array(values) => values
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            if !rendered.is_empty() {
                prompt.push_str(&format!("\n{label}:\n{rendered}"));
            }
        }
    }
    if let Some(image_count) = payload.card_context.get("image_count").and_then(Value::as_u64) {
        if image_count > 0 {
            prompt.push_str(&format!("\nImages: {image_count}"));
        }
    }
    prompt.push_str("\n\nConversation:\n");
    for entry in &payload.history {
        let role = entry.get("role").and_then(Value::as_str).unwrap_or("user");
        let content = entry.get("content").and_then(Value::as_str).unwrap_or("");
        prompt.push_str(&format!("{}: {}\n", role, content));
    }
    prompt.push_str(&format!("user: {}", payload.prompt));
    prompt
}

fn agent_command(provider: &str, prompt: &str, workspace: &Path) -> Result<Command, AgentError> {
    let mut command = match provider {
        "opencode" => {
            let mut command = Command::new("opencode");
            command.arg("run").arg("--format").arg("json").env(
                "OPENCODE_CONFIG_CONTENT",
                r#"{"permission":{"*":"deny","read":"allow","glob":"allow","grep":"allow"}}"#,
            );
            if workspace != std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) {
                command.arg("--dir").arg(workspace);
            }
            command.arg(prompt);
            command
        }
        "codex" => {
            let mut command = Command::new("codex");
            command.arg("exec").arg("--json").arg("--sandbox").arg("read-only");
            if workspace != std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")) {
                command.arg("--cd").arg(workspace);
            }
            command
        }
        _ => return Err(agent_error("invalid_agent_provider", "unsupported local agent")),
    };
    command
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    Ok(command)
}

fn send_agent_output(
    sender: &mpsc::Sender<Event>,
    request_id: &str,
    text: &str,
    output: &mut String,
    emitted_output: &mut bool,
) -> Result<(), AgentError> {
    let text = if text.len() > MAX_AGENT_EVENT_BYTES {
        let mut end = MAX_AGENT_EVENT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };
    if text.is_empty() {
        return Ok(());
    }
    if output.len().saturating_add(text.len()) > MAX_AGENT_OUTPUT_BYTES {
        return Err(agent_error(
            "agent_output_too_large",
            "the local agent produced too much output",
        ));
    }
    output.push_str(text);
    *emitted_output = true;
    sender
        .send(Event::Output { request_id: request_id.to_owned(), text: text.to_owned() })
        .map_err(|_| agent_error("agent_runtime_closed", "the local agent runtime closed"))
}

fn extract_agent_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(values) => {
            Some(values.iter().filter_map(extract_agent_text).collect::<Vec<_>>().join(""))
        }
        Value::Object(object) => {
            for key in ["text", "delta", "content", "message", "msg", "part", "result"] {
                if let Some(value) = object.get(key) {
                    if let Some(text) = extract_agent_text(value) {
                        if !text.is_empty() {
                            return Some(text);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn agent_line_text(line: &str) -> Option<String> {
    match serde_json::from_str::<Value>(line) {
        Ok(value) => extract_agent_text(&value),
        Err(_) => Some(line.to_owned()),
    }
}

fn read_agent_lines(stdout: impl Read, sender: mpsc::Sender<Result<String, String>>) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => return,
            Ok(_) if line.len() > MAX_AGENT_EVENT_BYTES => {
                let _ = sender.send(Err("the local agent returned an oversized event".to_owned()));
                return;
            }
            Ok(_) => {
                let text = String::from_utf8_lossy(&line).trim_end().to_owned();
                if !text.is_empty() && sender.send(Ok(text)).is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(Err(error.to_string()));
                return;
            }
        }
    }
}

#[cfg(not(unix))]
fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn terminate_child(child: &mut Child) {
    let process_group = format!("-{}", child.id());
    let _ = Command::new("kill").args(["-TERM", "--", &process_group]).status();
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && child.try_wait().ok().flatten().is_none() {
        thread::sleep(Duration::from_millis(10));
    }
    if child.try_wait().ok().flatten().is_none() {
        let _ = Command::new("kill").args(["-KILL", "--", &process_group]).status();
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn run_provider(
    request_id: &str,
    provider: &str,
    payload: &AgentRunPayload,
    workspace: &Path,
    cancel: &AtomicBool,
    sender: &mpsc::Sender<Event>,
) -> Result<AgentRunOutcome, AgentError> {
    let prompt = build_agent_prompt(payload);
    let mut command = agent_command(provider, &prompt, workspace)?;
    let mut child = command.spawn().map_err(|error| {
        agent_error("agent_start_failed", format!("could not start {provider}: {error}"))
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        if provider == "codex" {
            if let Err(error) = stdin.write_all(prompt.as_bytes()) {
                terminate_child(&mut child);
                return Err(agent_error(
                    "agent_start_failed",
                    format!("could not send the prompt to {provider}: {error}"),
                ));
            }
        }
        let _ = stdin.flush();
    }
    let stdout = child.stdout.take().ok_or_else(|| {
        agent_error("agent_start_failed", "the local agent stdout is unavailable")
    })?;
    let (line_sender, line_receiver) = mpsc::channel();
    let output_thread = thread::spawn(move || read_agent_lines(stdout, line_sender));
    let started = Instant::now();
    let mut output = String::new();
    let mut emitted_output = false;
    let mut status = None;
    let mut cancelled = false;
    let mut timed_out = false;
    loop {
        match line_receiver.recv_timeout(AGENT_POLL_INTERVAL) {
            Ok(Ok(line)) => {
                let Some(text) = agent_line_text(&line) else {
                    continue;
                };
                if let Err(error) =
                    send_agent_output(sender, request_id, &text, &mut output, &mut emitted_output)
                {
                    terminate_child(&mut child);
                    let _ = output_thread.join();
                    return Err(error);
                }
            }
            Ok(Err(error)) => {
                terminate_child(&mut child);
                return Err(agent_error("agent_protocol_error", error));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
            terminate_child(&mut child);
            break;
        }
        if started.elapsed() >= AGENT_TIMEOUT {
            timed_out = true;
            terminate_child(&mut child);
            break;
        }
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                status = Some(exit_status);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                terminate_child(&mut child);
                return Err(agent_error("agent_failed", error.to_string()));
            }
        }
    }
    if status.is_none() && !cancelled && !timed_out {
        status = child.wait().ok();
    }
    let _ = output_thread.join();
    while let Ok(message) = line_receiver.try_recv() {
        if let Ok(line) = message {
            let Some(text) = agent_line_text(&line) else {
                continue;
            };
            send_agent_output(sender, request_id, &text, &mut output, &mut emitted_output)?;
        }
    }
    if cancelled {
        return Ok(AgentRunOutcome::Cancelled);
    }
    if timed_out {
        return Err(agent_error("agent_timeout", "the local agent run timed out"));
    }
    let status = status.ok_or_else(|| {
        agent_error("agent_failed", "the local agent did not report an exit status")
    })?;
    if !status.success() {
        return Err(agent_error("agent_failed", "the local agent exited unsuccessfully"));
    }
    if output.trim().is_empty() {
        return Err(agent_error("agent_empty_output", "the local agent returned no text"));
    }
    Ok(AgentRunOutcome::Completed { text: output, provider: provider.to_owned(), emitted_output })
}

fn run_agent(
    request_id: &str,
    payload: &AgentRunPayload,
    selected_provider: &str,
    cancel: Arc<AtomicBool>,
    sender: &mpsc::Sender<Event>,
) -> Result<AgentRunOutcome, AgentError> {
    let workspace = agent_workspace(&payload.workspace)?;
    let providers = if payload.auto_fallback && selected_provider == "opencode" {
        vec!["opencode", "codex"]
    } else if payload.auto_fallback && selected_provider == "codex" {
        vec!["codex", "opencode"]
    } else {
        vec![selected_provider]
    };
    let mut last_error = None;
    for provider in &providers {
        match run_provider(request_id, provider, payload, &workspace, &cancel, sender) {
            Ok(outcome) => return Ok(outcome),
            Err(error) if error.code == "agent_start_failed" && providers.len() > 1 => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| agent_error("agent_unavailable", "no local agent is available")))
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
    fn agent_payload_rejects_unsupported_provider() {
        let events = process_line(&request(
            "agent_run",
            json!({
                "note_id": "note-1",
                "prompt": "hello",
                "provider": "unknown",
                "card_context": {"text": "card"},
                "history": []
            }),
        ));

        assert!(
            matches!(events[1], Event::Error { ref code, .. } if code == "invalid_agent_payload")
        );
    }

    #[test]
    fn agent_line_text_extracts_nested_output() {
        let line = r#"{"type":"text","part":{"text":"hello"}}"#;
        assert_eq!(agent_line_text(line).as_deref(), Some("hello"));
        assert_eq!(agent_line_text("plain output").as_deref(), Some("plain output"));
    }

    #[test]
    fn agent_prompt_preserves_structured_card_sections() {
        let payload: AgentRunPayload = serde_json::from_value(json!({
            "note_id": "note-1",
            "prompt": "explain",
            "provider": "opencode",
            "workspace": "",
            "system_instruction": "",
            "card_context": {
                "text": "full card",
                "front": "question",
                "back": "answer",
                "math": ["x + 1"],
                "code": ["const answer = 42"],
                "tables": ["cell"],
                "image_labels": ["diagram"],
                "image_count": 1
            },
            "history": []
        }))
        .unwrap();
        let prompt = build_agent_prompt(&payload);
        assert!(prompt.contains("Front:\nquestion"));
        assert!(prompt.contains("Math:\nx + 1"));
        assert!(prompt.contains("Images: 1"));
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
