//! In-process filtering proxy — network-level ad/tracker blocking with exact stats.
//!
//! WebKitGTK's NetworkSession is pointed at `127.0.0.1:<port>`. All browser traffic
//! flows through this loopback proxy:
//!   - HTTPS: only the CONNECT hostname is visible (we never MITM TLS — the tunnel
//!     is byte-for-byte relayed; filtering happens by hostname + host-level rules).
//!   - HTTP: the full URL is visible and every rule applies.
//! Blocking decisions come from the shared `AdBlockEngine`. The proxy keeps zero
//! persistent logs: counters live in memory only.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::privacy::adblock::AdBlockEngine;

const BLOCK_RESPONSE: &[u8] = b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\nX-Peregrine-Filter: blocked\r\n\r\n";
const MAX_HEADER: usize = 32 * 1024;

pub struct FilteringProxy {
    pub port: u16,
}

/// Read one request head (request line + headers), returning raw bytes and the parsed
/// request line.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<(Vec<u8>, String)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_head_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            // note: extra bytes beyond head belong to a possible body; we keep buf for relay
            return Ok((buf, head));
        }
        if buf.len() > MAX_HEADER {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "header too large"));
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof in head"))
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

/// Extract the hostname:port from a CONNECT target or absolute-URI request line.
fn parse_target(request_line: &str) -> Option<(String, u16, bool)> {
    // returns (host, port, is_connect)
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    if method.eq_ignore_ascii_case("CONNECT") {
        let (h, p) = match target.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(443)),
            None => (target.to_string(), 443),
        };
        Some((h, p, true))
    } else {
        // absolute-form: GET http://host/path HTTP/1.1
        let url = url::Url::parse(target).ok()?;
        let port = url.port_or_known_default().unwrap_or(80);
        Some((url.host_str()?.to_string(), port, false))
    }
}

pub async fn run(engine: Arc<AdBlockEngine>, shutdown: Arc<tokio::sync::Notify>) -> std::io::Result<FilteringProxy> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.notified() => break,
                accepted = listener.accept() => {
                    let (mut stream, _addr) = match accepted {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let engine = engine.clone();
                    tokio::spawn(async move {
                        let _ = handle_connection(&mut stream, engine).await;
                    });
                }
            }
        }
    });
    Ok(FilteringProxy { port })
}

async fn handle_connection(stream: &mut TcpStream, engine: Arc<AdBlockEngine>) -> std::io::Result<()> {
    let (raw, head) = read_head(stream).await?;
    let request_line = head.lines().next().unwrap_or("").to_string();
    let Some((host, port, is_connect)) = parse_target(&request_line) else {
        return Ok(());
    };

    // ---- filter decision ----
    let pseudo_url = if is_connect {
        format!("https://{host}/")
    } else {
        // for plain http the full absolute URL is in the request line
        request_line.split_whitespace().nth(1).unwrap_or_default().to_string()
    };
    let blocked = engine.check_network(&pseudo_url, &host);

    if let Some(category) = blocked {
        engine.record_block(&host, category, 0);
        let _ = stream.write_all(BLOCK_RESPONSE).await;
        let _ = stream.shutdown().await;
        return Ok(());
    }

    // ---- upstream connect ----
    let mut upstream = match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        TcpStream::connect((host.as_str(), port)),
    )
    .await
    {
        Ok(Ok(s)) => s,
        _ => {
            let _ = stream
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            return Ok(());
        }
    };

    if is_connect {
        // Tell the client the tunnel is up, then relay raw bytes (no TLS inspection).
        stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    } else {
        // Relay the original request head (we already consumed it), forcing
        // connection-close semantics so each subsequent request is inspected.
        let mut out = raw.clone();
        // rewrite "Connection: keep-alive" → close (best-effort, case-insensitive)
        let head_lower = head.to_ascii_lowercase();
        if head_lower.contains("connection:") {
            let replaced = head.replace_regex_connection();
            out = replaced.into_bytes();
        }
        upstream.write_all(&out).await?;
    }

    // ---- bidirectional relay ----
    relay(stream, &mut upstream).await
}

trait ConnRewrite {
    fn replace_regex_connection(&self) -> String;
}
impl ConnRewrite for String {
    fn replace_regex_connection(&self) -> String {
        // Replace any Connection header value with "close" line by line (cheap, no regex).
        let mut out = String::with_capacity(self.len());
        for line in self.lines() {
            if line.to_ascii_lowercase().starts_with("connection:") {
                out.push_str("Connection: close");
            } else {
                out.push_str(line);
            }
            out.push_str("\r\n");
        }
        out.push_str("\r\n");
        out
    }
}

async fn relay(a: &mut TcpStream, b: &mut TcpStream) -> std::io::Result<()> {
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::io::copy_bidirectional(a, b),
    )
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_connect_target() {
        let (h, p, c) = parse_target("CONNECT tracker.example.net:443 HTTP/1.1").unwrap();
        assert_eq!(h, "tracker.example.net");
        assert_eq!(p, 443);
        assert!(c);
    }

    #[test]
    fn parse_absolute_uri() {
        let (h, p, c) = parse_target("GET http://ads.example.com/banner.js HTTP/1.1").unwrap();
        assert_eq!(h, "ads.example.com");
        assert_eq!(p, 80);
        assert!(!c);
    }
}
