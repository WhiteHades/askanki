use std::env;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use askanki_core::{DEFAULT_RETENTION_DAYS, Event, MAX_REQUEST_BYTES, Runtime};

enum InputLine {
    Eof,
    Line(String),
    TooLarge,
    Error(String),
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
    let (input_sender, input_receiver) = mpsc::channel();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        loop {
            let input = match read_input_line(&mut reader) {
                Ok(input) => input,
                Err(error) => InputLine::Error(error.to_string()),
            };
            let terminal =
                matches!(input, InputLine::Eof | InputLine::TooLarge | InputLine::Error(_));
            if input_sender.send(input).is_err() || terminal {
                return;
            }
        }
    });
    let (event_sender, event_receiver) = mpsc::channel();
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let mut shutdown = false;
    let mut input_closed = false;

    while !shutdown {
        match input_receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(InputLine::Line(line)) => {
                let events = runtime.process_line_with_sender(line.trim_end(), &event_sender);
                let should_shutdown = events.iter().any(Event::is_shutdown);
                for event in events {
                    write_event_to(&mut writer, &event)?;
                }
                writer.flush()?;
                shutdown = should_shutdown;
            }
            Ok(InputLine::Eof) => input_closed = true,
            Ok(InputLine::TooLarge) => {
                write_event_to(
                    &mut writer,
                    &Event::Error {
                        request_id: "unknown".to_owned(),
                        code: "request_too_large".to_owned(),
                        message: "request exceeds the maximum size".to_owned(),
                    },
                )?;
                writer.flush()?;
                shutdown = true;
            }
            Ok(InputLine::Error(message)) => {
                write_event_to(
                    &mut writer,
                    &Event::Error {
                        request_id: "unknown".to_owned(),
                        code: "invalid_input".to_owned(),
                        message,
                    },
                )?;
                writer.flush()?;
                shutdown = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                input_closed = true;
                if !runtime.has_active_runs() {
                    shutdown = true;
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
        loop {
            match event_receiver.try_recv() {
                Ok(event) => {
                    let should_shutdown = event.is_shutdown();
                    write_event_to(&mut writer, &event)?;
                    writer.flush()?;
                    shutdown |= should_shutdown;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if input_closed && !runtime.has_active_runs() {
            shutdown = true;
        }
    }
    runtime.cancel_all();
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
