//! The local MCP server: a socket the owner opens on purpose.
//!
//! Bound to 127.0.0.1 and nothing else. There is no token — with no remote
//! surface there is nothing for one to protect — so the Origin check in
//! `protocol` carries the defense against browser pages, and the bind address
//! is what keeps the port off the network.
//!
//! `tiny_http` 0.12 gives a request's reader no read timeout. A client that
//! declares a `Content-Length` and then sends less than it promised — or
//! nothing at all — can stall the serving thread forever. Most refusals (a
//! foreign origin, the wrong method, a non-JSON content type, an over-cap
//! `Content-Length`) never read the body at all, so the stall then happens
//! when the dropped `Request` drains the undeclared remainder off the raw
//! socket — a blocking read with no timeout. A body that lies about its
//! length (absent, chunked, or under-reported) is instead caught by reading
//! up to the cap before answering, and that read can stall on the very same
//! kind of client, one call earlier. Either way, the stall is on a thread
//! nobody joins: `McpState::stop_server` only `unblock`s the listener and
//! lets a stalled thread go, leaked rather than joined, rather than risk
//! freezing the app at quit (`stop` runs on the main thread). The visible
//! fallout: a stalled thread keeps holding the port, so the next `start`
//! lands on the fallback port instead of `PREFERRED_PORT`, and a URL an
//! agent already has stops working. That is a visible failure — the MCP
//! screen always shows the port actually bound — never a silent one.

pub mod config;
pub mod log;
pub mod protocol;
pub mod tools;

use std::io::Read;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use tiny_http::{Header, Request, Response, Server};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::mcp::config::McpConfig;
use crate::mcp::log::CallLog;

pub const PREFERRED_PORT: u16 = 7717;
/// Emitted after any tool wrote, so open screens reload instead of showing a
/// number the agent already changed.
pub const DATA_CHANGED_EVENT: &str = "mcp:data-changed";
/// An open port must not be a way to exhaust memory.
const MAX_BODY_BYTES: usize = 256 * 1024;

fn body_too_large(len: usize) -> bool {
    len > MAX_BODY_BYTES
}

struct Running {
    server: Arc<Server>,
    port: u16,
}

pub struct McpState {
    running: Mutex<Option<Running>>,
    pub log: Arc<CallLog>,
}

impl McpState {
    pub fn new() -> McpState {
        McpState { running: Mutex::new(None), log: Arc::new(CallLog::new()) }
    }

    pub fn port(&self) -> Option<u16> {
        self.running.lock().expect("mcp state mutex poisoned").as_ref().map(|r| r.port)
    }

    /// Unblocks the listener and lets the serving thread go rather than
    /// joining it. See the module doc: that thread can be stuck forever
    /// inside a stalled client's body read, and joining it here — `stop`
    /// runs on the main thread — could freeze the app at quit.
    pub fn stop_server(&self) {
        let mut running = self.running.lock().expect("mcp state mutex poisoned");
        if let Some(r) = running.take() {
            r.server.unblock();
        }
    }
}

impl Default for McpState {
    fn default() -> Self {
        Self::new()
    }
}

/// Every response carries the negotiated protocol version, which the spec
/// requires on the transport once `initialize` has agreed on it.
fn respond(request: Request, body: String, status: u16) {
    let content_type = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("a static header always parses");
    let version = Header::from_bytes(
        &b"MCP-Protocol-Version"[..],
        protocol::PROTOCOL_VERSION.as_bytes(),
    )
    .expect("a static header always parses");
    let _ = request.respond(
        Response::from_string(body)
            .with_status_code(status)
            .with_header(content_type)
            .with_header(version),
    );
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

/// `Err` when `Origin` appears more than once. With no token, this header is
/// the entire authentication story, so a repeat is refused outright rather
/// than resolved by picking one twin over the other — stricter and cheaper
/// than trying to decide which one to believe.
fn origin_header(request: &Request) -> Result<Option<&str>, ()> {
    let mut found = None;
    for h in request.headers() {
        if h.field.equiv("Origin") {
            if found.is_some() {
                return Err(());
            }
            found = Some(h.value.as_str());
        }
    }
    Ok(found)
}

/// Every check that needs nothing but the request itself — no config, no
/// lock, no database. Kept apart from `serve` so a real socket can drive
/// every guard without a Tauri app behind it. `None` means the caller should
/// go on to load config and dispatch.
fn refusal(request: &Request) -> Option<(u16, &'static str)> {
    match origin_header(request) {
        Err(()) => return Some((403, r#"{"error":"origin not allowed"}"#)),
        Ok(origin) if !protocol::origin_allowed(origin) => {
            return Some((403, r#"{"error":"origin not allowed"}"#))
        }
        Ok(_) => {}
    }
    if request.method() != &tiny_http::Method::Post {
        return Some((405, r#"{"error":"use POST"}"#));
    }
    // A form-encoded body is how a plain HTML form reaches a port without a
    // preflight — requiring JSON closes that door. Media types compare
    // case-insensitively per RFC 9110, and a trailing `; charset=...`
    // parameter must not defeat the match.
    let is_json = header(request, "Content-Type")
        .map(|ct| {
            ct.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json")
        })
        .unwrap_or(false);
    if !is_json {
        return Some((415, r#"{"error":"expected application/json"}"#));
    }
    if body_too_large(request.body_length().unwrap_or(0)) {
        return Some((413, r#"{"error":"body too large"}"#));
    }
    None
}

/// Binds the preferred port, falling back to whatever the OS hands out. A busy
/// 7717 must not mean a broken feature — the screen shows the port that won.
fn bind() -> AppResult<Server> {
    match Server::http(("127.0.0.1", PREFERRED_PORT)) {
        Ok(server) => Ok(server),
        Err(_) => Server::http(("127.0.0.1", 0))
            .map_err(|e| AppError::Invalid(format!("could not open a local port: {e}"))),
    }
}

pub fn start(app: &AppHandle) -> AppResult<u16> {
    let state = app.state::<McpState>();
    let mut running = state.running.lock().expect("mcp state mutex poisoned");
    if let Some(r) = running.as_ref() {
        return Ok(r.port);
    }

    let server = Arc::new(bind()?);
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| AppError::Invalid("server bound to a non-IP address".into()))?
        .port();

    {
        let server = server.clone();
        let app = app.clone();
        let log = state.log.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                serve(&app, &log, request);
            }
        });
    }

    *running = Some(Running { server, port });
    Ok(port)
}

/// Takes the request by value: `Request::respond` consumes it, so each guard
/// simply answers and returns.
fn serve(app: &AppHandle, log: &CallLog, mut request: Request) {
    if let Some((status, body)) = refusal(&request) {
        respond(request, body.to_string(), status);
        return;
    }

    // `Content-Length` already bounded the size in `refusal`, but a chunked
    // or absent length must not be trusted for that — reading one byte past
    // the cap catches a body that lied about its length, without ever
    // holding more than `MAX_BODY_BYTES + 1` bytes in memory.
    let mut bytes = Vec::new();
    let read = request.as_reader().take(MAX_BODY_BYTES as u64 + 1).read_to_end(&mut bytes);
    match read {
        Ok(n) if n <= MAX_BODY_BYTES => {}
        _ => {
            respond(request, r#"{"error":"body too large"}"#.to_string(), 413);
            return;
        }
    }
    // JSON is UTF-8 by definition — invalid bytes are a different failure
    // than "too large", and worth telling apart from the `-32700` a
    // malformed-but-well-encoded body gets from `protocol::handle`.
    let body = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            respond(request, r#"{"error":"body is not valid utf-8"}"#.to_string(), 400);
            return;
        }
    };

    let out = {
        let db = app.state::<Db>();
        let locale = app.state::<crate::locale::LocaleState>();
        let mut conn = db.conn.lock().expect("db mutex poisoned");
        let pack = locale.pack.lock().expect("locale mutex poisoned");

        match McpConfig::load(&conn) {
            Ok(cfg) if !cfg.enabled => {
                drop(pack);
                drop(conn);
                // The config can be switched off without the socket being
                // torn down first, so this is checked on every request, not
                // only when the listener starts.
                respond(request, r#"{"error":"mcp disabled"}"#.to_string(), 403);
                return;
            }
            Ok(cfg) => protocol::handle(&mut conn, &pack, &cfg, log, &body),
            Err(e) => {
                drop(pack);
                drop(conn);
                // `e` is a rusqlite message that can carry a quote or a
                // backslash — interpolating it into a hand-built JSON literal
                // would produce malformed JSON on exactly the input that most
                // needs a readable error.
                respond(request, serde_json::json!({ "error": e.to_string() }).to_string(), 500);
                return;
            }
        }
    };

    if out.wrote {
        let _ = app.emit(DATA_CHANGED_EVENT, ());
    }

    // A notification is answered with 202 and no body, per JSON-RPC.
    let status = if out.json.is_empty() { 202 } else { 200 };
    respond(request, out.json, status);
}

pub fn stop(app: &AppHandle) {
    app.state::<McpState>().stop_server();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    /// Sends a raw HTTP request to a fresh server whose only handler is
    /// `refusal` (falling through to a bare 200 when it allows the request),
    /// and returns the raw response text. Lets every guard in `refusal` be
    /// driven over a real socket without a Tauri `AppHandle`.
    fn round_trip(raw: &str) -> String {
        let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();

        let handle = {
            let server = server.clone();
            std::thread::spawn(move || {
                if let Some(request) = server.incoming_requests().next() {
                    match refusal(&request) {
                        Some((status, body)) => respond(request, body.to_string(), status),
                        None => respond(request, "{}".to_string(), 200),
                    }
                }
            })
        };

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(raw.as_bytes()).unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        handle.join().unwrap();
        response
    }

    /// `respond` must produce a real, complete HTTP response — status line,
    /// headers and body — over an actual socket, not just a value that looks
    /// right in isolation. This does not exercise `serve`, `start`, or
    /// `protocol::handle`: those are covered separately (`refusal` above,
    /// `protocol::handle` without a socket in `protocol.rs`).
    #[test]
    fn respond_writes_a_valid_http_response_over_a_real_socket() {
        let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();

        let handle = {
            let server = server.clone();
            std::thread::spawn(move || {
                if let Some(request) = server.incoming_requests().next() {
                    respond(request, "{\"ok\":true}".to_string(), 200);
                }
            })
        };

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        handle.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
        assert!(response.contains("\"ok\":true"));
    }

    /// The response must carry the negotiated version on the transport, not
    /// only inside the `initialize` result.
    #[test]
    fn the_protocol_version_is_a_response_header() {
        let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();

        let handle = {
            let server = server.clone();
            std::thread::spawn(move || {
                if let Some(request) = server.incoming_requests().next() {
                    respond(request, "{}".to_string(), 200);
                }
            })
        };

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        handle.join().unwrap();

        assert!(
            response.contains(crate::mcp::protocol::PROTOCOL_VERSION),
            "got: {response}"
        );
    }

    #[test]
    fn a_foreign_origin_is_refused_with_403() {
        let response = round_trip(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://evil.example\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
    }

    /// With no token, `Origin` is the entire authentication story — a repeat
    /// is refused outright rather than resolved by picking one.
    #[test]
    fn a_repeated_origin_header_is_refused_outright() {
        let response = round_trip(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://localhost\r\nOrigin: http://evil.example\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
    }

    #[test]
    fn a_non_post_method_is_refused_with_405() {
        let response =
            round_trip("GET /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 405"), "got: {response}");
    }

    #[test]
    fn a_non_json_content_type_is_refused_with_415() {
        let response = round_trip(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 415"), "got: {response}");
    }

    /// RFC 9110 media types compare case-insensitively — `Application/JSON`
    /// is `application/json`.
    #[test]
    fn a_case_insensitive_json_content_type_is_accepted() {
        let response = round_trip(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: Application/JSON\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    }

    #[test]
    fn a_body_over_the_cap_is_refused_with_413() {
        let body = "x".repeat(MAX_BODY_BYTES + 1);
        let response = round_trip(&format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ));
        assert!(response.starts_with("HTTP/1.1 413"), "got: {response}");
    }

    /// A client that declares a `Content-Length` and never sends the body
    /// wedges the serving thread forever: dropping the `Request` drains the
    /// undeclared remainder off the raw socket with a blocking read that
    /// `tiny_http` gives no timeout.
    #[test]
    fn a_client_that_never_sends_its_declared_body_wedges_the_serving_thread() {
        let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();

        let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
        let thread = {
            let server = server.clone();
            std::thread::spawn(move || {
                if let Some(request) = server.incoming_requests().next() {
                    let _ = accepted_tx.send(());
                    if let Some((status, body)) = refusal(&request) {
                        respond(request, body.to_string(), status);
                    }
                }
            })
        };

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )
        .unwrap();
        // The declared body is deliberately never sent: the thread is now
        // inside `Request`'s drop, blocked draining the socket. (Calling
        // `server.unblock()` here would race the header-parsing worker for
        // this very request — winning that race makes `incoming_requests`
        // yield `None` instead of the request, and the wedge is never
        // reached at all. `stop`'s `unblock()` only matters for a listener
        // waiting on its *next* accept, which this single-request test does
        // not exercise.)

        // Wait for the request to actually reach the serving thread before
        // timing the join: otherwise a slow scheduler could pass this test
        // for the wrong reason — the request never having been accepted.
        accepted_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the request was never accepted");

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(thread.join());
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(500)).is_err(),
            "the serving thread returned on its own — this client no longer wedges it"
        );

        // Keep `stream` alive for the whole assertion above: dropping it
        // early would close the connection and let the drain finish, which
        // would defeat the point of this test.
        drop(stream);
    }

    /// `stop_server` must return even while the serving thread is
    /// permanently wedged inside a stalled client's body read — the
    /// alternative is a quit that never completes, since `stop` runs on the
    /// main thread.
    #[test]
    fn stop_server_returns_while_the_serving_thread_is_wedged() {
        let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();

        let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
        {
            let server = server.clone();
            std::thread::spawn(move || {
                if let Some(request) = server.incoming_requests().next() {
                    let _ = accepted_tx.send(());
                    if let Some((status, body)) = refusal(&request) {
                        respond(request, body.to_string(), status);
                    }
                }
            });
        }

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )
        .unwrap();
        // See the sibling test above for why `accepted_rx` is awaited before
        // any timing starts, and why `server.unblock()` is not called here.
        accepted_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the request was never accepted");

        let state =
            McpState { running: Mutex::new(Some(Running { server, port })), log: Arc::new(CallLog::new()) };

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            state.stop_server();
            let _ = tx.send(());
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(500)).is_ok(),
            "stop_server() did not return while the serving thread was wedged"
        );

        drop(stream);
    }
}
