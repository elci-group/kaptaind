//! `kaptaind-cli probe` — health/metrics/events scraper (Workstream D3).
//!
//! Wraps the daemon's HTTP endpoints (`/health`, `/metrics`,
//! `/metrics/prometheus`, `/events`) so operators don't hand-curl. Uses a
//! hand-written HTTP/1.1 client over `std::net::TcpStream` (no `reqwest`).

use kaptaind::config::loader::Config;
use kaptaind::util::style::*;
use serde::Serialize;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const HOST: &str = "127.0.0.1";
const TIMEOUT: Duration = Duration::from_secs(5);

pub enum ProbeAction {
    Health,
    Metrics { prometheus: bool },
    Events { follow: bool },
}

pub fn handle_probe(config: &Config, action: &ProbeAction, format: &str) -> anyhow::Result<()> {
    let port = config.health_port;
    let json = format.eq_ignore_ascii_case("json");

    match action {
        ProbeAction::Health => oneshot(port, "/health", "application/json", json),
        ProbeAction::Metrics { prometheus } => {
            let (path, accept) = if *prometheus {
                ("/metrics/prometheus", "text/plain")
            } else {
                ("/metrics", "application/json")
            };
            oneshot(port, path, accept, json)
        }
        ProbeAction::Events { follow } => {
            if *follow {
                stream_sse(port, "/events", json)
            } else {
                oneshot(port, "/events", "text/event-stream", json)
            }
        }
    }
}

#[derive(Serialize)]
struct ProbeError<'a> {
    reachable: bool,
    port: u16,
    endpoint: &'a str,
    message: String,
}

/// Report a connection failure: clear text + exit 0 for text, nonzero for JSON.
fn unreachable(endpoint: &str, port: u16, json: bool) -> anyhow::Result<()> {
    let msg = format!("daemon not running / no health server on {HOST}:{port} ({endpoint})");
    if json {
        let body = ProbeError {
            reachable: false,
            port,
            endpoint,
            message: msg,
        };
        println!("{}", serde_json::to_string_pretty(&body)?);
        anyhow::bail!("health server unreachable");
    }
    println!("{} {}", "🔌".yellow(), msg.yellow());
    Ok(())
}

/// Perform a single GET and print the response body.
fn oneshot(port: u16, path: &str, accept: &str, json: bool) -> anyhow::Result<()> {
    let (status, body) = match http_get(port, path, accept) {
        Ok(r) => r,
        Err(_) => return unreachable(path, port, json),
    };

    if json {
        // If the body is JSON, pass it through; otherwise wrap it.
        match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
            Err(_) => {
                let wrapped = serde_json::json!({
                    "status": status,
                    "endpoint": path,
                    "body": body,
                });
                println!("{}", serde_json::to_string_pretty(&wrapped)?);
            }
        }
    } else {
        println!(
            "{} {} → HTTP {}",
            "🛰️ ".blue(),
            path.bold().cyan(),
            status.to_string().blue()
        );
        println!("{body}");
    }
    Ok(())
}

/// Stream an SSE endpoint, printing lines as they arrive until EOF/interrupt.
fn stream_sse(port: u16, path: &str, json: bool) -> anyhow::Result<()> {
    let stream = match connect(port, path, "text/event-stream") {
        Ok(s) => s,
        Err(_) => return unreachable(path, port, json),
    };
    let mut reader = BufReader::new(stream);
    // Skip response headers.
    skip_headers(&mut reader)?;

    if !json {
        println!(
            "{} following {} (Ctrl-C to stop)",
            "📡".blue(),
            path.bold().cyan()
        );
    }

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim_end_matches(['\r', '\n']);
                if trimmed.is_empty() {
                    continue;
                }
                if json {
                    let ev = serde_json::json!({ "event": trimmed });
                    println!("{}", serde_json::to_string(&ev)?);
                } else {
                    println!("{trimmed}");
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    Ok(())
}

fn connect(port: u16, path: &str, accept: &str) -> std::io::Result<TcpStream> {
    let stream = TcpStream::connect((HOST, port))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {HOST}:{port}\r\nAccept: {accept}\r\nConnection: close\r\n\r\n"
    );
    let mut stream = stream;
    stream.write_all(req.as_bytes())?;
    stream.flush()?;
    Ok(stream)
}

/// GET and read the full response (status code + body). Relies on
/// `Connection: close` so the server terminates the body with EOF.
fn http_get(port: u16, path: &str, accept: &str) -> std::io::Result<(u16, String)> {
    let stream = connect(port, path, accept)?;
    let mut reader = BufReader::new(stream);

    let status = read_status_line(&mut reader)?;
    skip_headers(&mut reader)?;

    let mut body = String::new();
    // Read until EOF (Connection: close). A read timeout bounds a misbehaving server.
    match reader.read_to_string(&mut body) {
        Ok(_) => {}
        Err(err)
            if err.kind() == std::io::ErrorKind::TimedOut
                || err.kind() == std::io::ErrorKind::WouldBlock =>
        {
            // Partial body is acceptable; return what we have.
        }
        Err(err) => return Err(err),
    }
    Ok((status, body))
}

fn read_status_line(reader: &mut BufReader<TcpStream>) -> std::io::Result<u16> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    // "HTTP/1.1 200 OK"
    let code = line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);
    Ok(code)
}

fn skip_headers(reader: &mut BufReader<TcpStream>) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
    }
    Ok(())
}
