// A tiny HTTP server for tests: one thread, each connection answered with the
// next canned response, every request recorded as it arrived.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub struct Canned {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Canned {
    pub fn json(status: u16, body: &str) -> Self {
        Canned { status, headers: vec![("Content-Type", "application/json".into())], body: body.as_bytes().to_vec() }
    }
}

/// Starts the server; returns its `http://127.0.0.1:<port>` origin and the
/// requests it saw (headers and body, as text).
pub fn serve(responses: Vec<Canned>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    std::thread::spawn(move || {
        for canned in responses {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            record.lock().unwrap().push(String::from_utf8_lossy(&buf).to_string());
            let mut head = format!("HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n", canned.status, canned.body.len());
            for (k, v) in &canned.headers {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&canned.body);
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}
