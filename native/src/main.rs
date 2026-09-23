use std::io::{self, BufRead, Write};

use askanki_core::{Event, MAX_REQUEST_BYTES, process_line};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }

        let events = if line.len() > MAX_REQUEST_BYTES {
            vec![Event::Error {
                request_id: "unknown".to_owned(),
                code: "request_too_large".to_owned(),
                message: "request exceeds the maximum size".to_owned(),
            }]
        } else {
            process_line(line.trim_end())
        };

        let shutdown = events.iter().any(Event::is_shutdown);
        for event in events {
            let encoded = serde_json::to_string(&event)
                .map_err(|error| io::Error::other(error.to_string()))?;
            writer.write_all(encoded.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;

        if shutdown {
            break;
        }
    }

    Ok(())
}
