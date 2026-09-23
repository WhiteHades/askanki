use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 65_536;

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
    Accepted {
        protocol: u16,
        request_id: String,
    },
    Output {
        request_id: String,
        text: String,
    },
    ApprovalRequired {
        request_id: String,
        approval_id: String,
        summary: String,
    },
    Completed {
        request_id: String,
        result: Value,
    },
    Cancelled {
        request_id: String,
    },
    Error {
        request_id: String,
        code: String,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
struct EchoPayload {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalPayload {
    summary: String,
}

pub fn process_line(line: &str) -> Vec<Event> {
    if line.len() > MAX_REQUEST_BYTES {
        return vec![error_event(
            "unknown",
            "request_too_large",
            "request exceeds the maximum size",
        )];
    }

    match serde_json::from_str::<Request>(line) {
        Ok(request) => process_request(request),
        Err(error) => vec![error_event(
            "unknown",
            "invalid_json",
            &format!("could not parse request: {error}"),
        )],
    }
}

pub fn process_request(request: Request) -> Vec<Event> {
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

    let accepted = Event::Accepted {
        protocol: PROTOCOL_VERSION,
        request_id: request_id.clone(),
    };

    match request.action.as_str() {
        "ping" => vec![
            accepted,
            Event::Completed {
                request_id,
                result: json!({"pong": true}),
            },
        ],
        "echo" => match serde_json::from_value::<EchoPayload>(request.payload) {
            Ok(payload) if payload.text.len() <= MAX_TEXT_BYTES => vec![
                accepted,
                Event::Output {
                    request_id: request_id.clone(),
                    text: payload.text.clone(),
                },
                Event::Completed {
                    request_id,
                    result: json!({"text": payload.text}),
                },
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
        "shutdown" => vec![
            accepted,
            Event::Completed {
                request_id,
                result: json!({"shutdown": true}),
            },
        ],
        _ => vec![error_event(
            &request_id,
            "unknown_action",
            "unsupported request action",
        )],
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

fn error_event(request_id: &str, code: &str, message: &str) -> Event {
    Event::Error {
        request_id: request_id.to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
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
        let events = process_line(&request(
            "request_approval",
            json!({"summary": "write a file"}),
        ));

        assert!(matches!(
            events[1],
            Event::ApprovalRequired { ref summary, .. } if summary == "write a file"
        ));
    }

    #[test]
    fn cancellation_is_reported() {
        let events = process_line(&request("cancel", json!({})));

        assert_eq!(
            events[1],
            Event::Cancelled {
                request_id: "test-request".to_owned(),
            }
        );
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
}
