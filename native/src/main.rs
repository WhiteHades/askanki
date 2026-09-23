use std::env;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;

use askanki_core::{DEFAULT_RETENTION_DAYS, Event, MAX_REQUEST_BYTES, Runtime};

enum InputLine {
    Eof,
    Line(String),
    TooLarge,
}

fn main() -> io::Result<()> {
    let mut runtime = match runtime_from_args() {
        Ok(runtime) => runtime,
        Err(message) => {
            write_event(&Event::Error {
                request_id: "unknown".to_owned(),
                code: "invalid_runtime_arguments".to_owned(),
                message,
            })?;
            return Ok(());
        }
    };
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    loop {
        let input = match read_input_line(&mut reader) {
            Ok(input) => input,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                write_event_to(
                    &mut writer,
                    &Event::Error {
                        request_id: "unknown".to_owned(),
                        code: "invalid_utf8".to_owned(),
                        message: "request is not valid UTF-8".to_owned(),
                    },
                )?;
                writer.flush()?;
                break;
            }
            Err(error) => return Err(error),
        };
        match input {
            InputLine::Eof => break,
            InputLine::TooLarge => {
                write_event_to(
                    &mut writer,
                    &Event::Error {
                        request_id: "unknown".to_owned(),
                        code: "request_too_large".to_owned(),
                        message: "request exceeds the maximum size".to_owned(),
                    },
                )?;
                writer.flush()?;
                break;
            }
            InputLine::Line(line) => {
                let events = runtime.process_line(line.trim_end());
                let shutdown = events.iter().any(Event::is_shutdown);
                for event in events {
                    write_event_to(&mut writer, &event)?;
                }
                writer.flush()?;
                if shutdown {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn read_input_line(reader: &mut impl BufRead) -> io::Result<InputLine> {
    let mut line = String::new();
    let bytes_read = reader.by_ref().take((MAX_REQUEST_BYTES + 1) as u64).read_line(&mut line)?;
    if bytes_read == 0 {
        return Ok(InputLine::Eof);
    }
    if bytes_read > MAX_REQUEST_BYTES {
        return Ok(InputLine::TooLarge);
    }
    Ok(InputLine::Line(line))
}

fn runtime_from_args() -> Result<Runtime, String> {
    let mut history_path = None;
    let mut retention_days = DEFAULT_RETENTION_DAYS;
    let mut args = env::args().skip(1);

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--history-path" => {
                history_path = Some(PathBuf::from(
                    args.next().ok_or_else(|| "--history-path requires a value".to_owned())?,
                ));
            }
            "--retention-days" => {
                let value =
                    args.next().ok_or_else(|| "--retention-days requires a value".to_owned())?;
                retention_days =
                    value.parse().map_err(|_| "--retention-days must be an integer".to_owned())?;
            }
            _ => return Err(format!("unsupported runtime argument: {argument}")),
        }
    }

    Runtime::new(history_path, retention_days)
}

fn write_event(event: &Event) -> io::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    write_event_to(&mut writer, event)
}

fn write_event_to(writer: &mut impl Write, event: &Event) -> io::Result<()> {
    let encoded =
        serde_json::to_string(event).map_err(|error| io::Error::other(error.to_string()))?;
    writer.write_all(encoded.as_bytes())?;
    writer.write_all(b"\n")
}
