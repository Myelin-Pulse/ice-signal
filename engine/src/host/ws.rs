//! Local dashboard transport: a WebSocket broadcaster for the event stream
//! plus a one-page HTTP server for the dashboard itself.
//!
//! Host-side only. The WS stream carries exactly the JSONL schema events,
//! byte for byte — the dashboard sees precisely what the phone app would see
//! over BLE, nothing more. New clients get the session's full event history
//! first, so a dashboard opened mid-session still renders a correct card.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use tungstenite::Message;

struct Shared {
    clients: Vec<Sender<String>>,
    history: Vec<String>,
}

#[derive(Clone)]
pub struct Broadcaster {
    shared: Arc<Mutex<Shared>>,
}

impl Broadcaster {
    /// Bind the HTTP (dashboard page) and WS (event stream) listeners and
    /// serve them on background threads. Pass port 0 for ephemeral ports
    /// (tests). Returns the broadcaster and the actual bound ports.
    pub fn start(http_port: u16, ws_port: u16, page: &'static str) -> Result<(Self, u16, u16)> {
        let http = TcpListener::bind(("127.0.0.1", http_port))?;
        let ws = TcpListener::bind(("127.0.0.1", ws_port))?;
        let bound = (http.local_addr()?.port(), ws.local_addr()?.port());

        let shared = Arc::new(Mutex::new(Shared { clients: Vec::new(), history: Vec::new() }));
        thread::spawn(move || serve_http(http, page));
        let accept_shared = shared.clone();
        thread::spawn(move || accept_ws(ws, accept_shared));

        Ok((Self { shared }, bound.0, bound.1))
    }

    /// Record one event line and fan it out; disconnected clients are dropped.
    pub fn send(&self, line: &str) {
        let mut shared = self.shared.lock().unwrap();
        shared.history.push(line.to_string());
        shared.clients.retain(|tx| tx.send(line.to_string()).is_ok());
    }
}

fn serve_http(listener: TcpListener, page: &'static str) {
    for stream in listener.incoming().flatten() {
        let _ = respond(stream, page);
    }
}

/// Minimal single-page responder: any request gets the dashboard.
fn respond(mut stream: TcpStream, page: &str) -> std::io::Result<()> {
    let mut buf = [0u8; 2048];
    let _ = stream.read(&mut buf)?;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        page.len()
    )?;
    stream.write_all(page.as_bytes())
}

fn accept_ws(listener: TcpListener, shared: Arc<Mutex<Shared>>) {
    for stream in listener.incoming().flatten() {
        let shared = shared.clone();
        thread::spawn(move || {
            let Ok(mut socket) = tungstenite::accept(stream) else { return };
            let (tx, rx) = mpsc::channel::<String>();
            // Replay + register under one lock: an event lands either in the
            // replayed history or in the channel, exactly once, in order.
            {
                let mut s = shared.lock().unwrap();
                for line in &s.history {
                    if socket.send(Message::text(line.clone())).is_err() {
                        return;
                    }
                }
                s.clients.push(tx);
            }
            while let Ok(line) = rx.recv() {
                if socket.send(Message::text(line)).is_err() {
                    return;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_dashboard_page() {
        let (_b, http_port, _) = Broadcaster::start(0, 0, "<html>ice</html>").unwrap();
        let mut conn = TcpStream::connect(("127.0.0.1", http_port)).unwrap();
        conn.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        conn.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with("<html>ice</html>"));
    }

    #[test]
    fn replays_history_then_streams_live_events() {
        let (b, _, ws_port) = Broadcaster::start(0, 0, "").unwrap();
        b.send(r#"{"t":0,"event":"SESSION_START"}"#);

        let (mut client, _) = tungstenite::connect(format!("ws://127.0.0.1:{ws_port}")).unwrap();
        b.send(r#"{"t":280,"event":"SPEAKING_START"}"#);

        let first = client.read().unwrap();
        assert_eq!(first.to_text().unwrap(), r#"{"t":0,"event":"SESSION_START"}"#);
        let second = client.read().unwrap();
        assert_eq!(second.to_text().unwrap(), r#"{"t":280,"event":"SPEAKING_START"}"#);
    }
}
