//! JSON-RPC 2.0 over a single POST, and the Origin check that stands in for
//! authentication.
//!
//! This whole module is a pure function of its arguments: no socket, no port,
//! no clock beyond the log entry. That is what lets the protocol, the security
//! rule and every tool be tested the way the rest of the app is — against an
//! in-memory database, with no network anywhere in the test.

use chrono::Local;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::locale::LocalePack;
use crate::mcp::config::McpConfig;
use crate::mcp::log::{CallEntry, CallLog};
use crate::mcp::tools;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const ARGS_LOG_LIMIT: usize = 160;

pub struct Response {
    /// Empty for a notification, which by definition gets no reply.
    pub json: String,
    pub wrote: bool,
}

/// Strips a trailing `:port`, but only outside an IPv6 literal's brackets: a
/// colon inside `[::1]` is part of the address, not a port separator, and
/// treating it as one turns `[::1]` into the unmatchable `[:`.
fn strip_port(host: &str) -> &str {
    match host.rfind(']') {
        Some(close) => match host[close + 1..].strip_prefix(':') {
            Some(_) => &host[..=close],
            None => host,
        },
        None => host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host),
    }
}

/// A local agent sends no `Origin`; a browser always does. Refusing every
/// non-local origin is therefore what keeps a web page from reaching the port
/// — and without a token, it is the only thing that does.
pub fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(origin) = origin else { return true };
    let host = origin
        .split("://")
        .nth(1)
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or("");
    // Compare the host itself, never a substring: `localhost.evil.example` is
    // not localhost, and a `contains` check would wave it through. Anything
    // that fails to spell one of these four names exactly — a numeric or
    // octal disguise for a loopback address, a userinfo-prefixed host, an
    // opaque "null" origin — is refused, not specially parsed.
    let bare = strip_port(host);
    matches!(bare, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

fn result(id: Value, value: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": value }).to_string()
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= ARGS_LOG_LIMIT {
        return s.to_string();
    }
    s.chars().take(ARGS_LOG_LIMIT).collect::<String>() + "…"
}

pub fn handle(
    conn: &mut Connection,
    pack: &LocalePack,
    cfg: &McpConfig,
    log: &CallLog,
    body: &str,
) -> Response {
    let parsed: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return Response { json: error(Value::Null, -32700, &e.to_string()), wrote: false }
        }
    };

    let id = parsed.get("id").cloned();
    let method = parsed.get("method").and_then(Value::as_str).unwrap_or("");

    // No id means a notification: the spec says answer nothing at all.
    let Some(id) = id else {
        return Response { json: String::new(), wrote: false };
    };

    match method {
        "initialize" => Response {
            json: result(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "finoinc", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),
            wrote: false,
        },

        "tools/list" => {
            let listed: Vec<Value> = tools::specs()
                .iter()
                .filter(|s| cfg.tool_enabled(s.name))
                .map(|s| {
                    json!({
                        "name": s.name,
                        "description": s.description,
                        "inputSchema": (s.schema)()
                    })
                })
                .collect();
            Response { json: result(id, json!({ "tools": listed })), wrote: false }
        }

        "tools/call" => {
            let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

            let outcome = tools::dispatch(conn, pack, cfg, &name, &args);

            log.push(CallEntry {
                // Overwritten by `CallLog::push`, which owns id assignment.
                id: 0,
                at: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                tool: truncate(&name),
                args: truncate(&args.to_string()),
                ok: outcome.is_ok(),
                error: outcome.as_ref().err().map(|e| e.to_string()),
            });

            // A write tool can commit its change and then fail a later step
            // (e.g. a rule insert commits before the backfill that follows
            // it runs) — so whether the webview is told to refresh follows
            // from what kind of tool was called, not from whether the call
            // returned `Ok`. A spurious refresh costs a repaint; a missed one
            // leaves stale money on screen.
            let wrote = tools::specs().iter().any(|s| s.name == name && s.write);

            match outcome {
                // An MCP tool failure is a result with `isError`, not a
                // JSON-RPC error: the agent is meant to read it and adapt,
                // which it cannot do with a transport-level failure.
                Ok(value) => Response {
                    json: result(
                        id,
                        json!({
                            "content": [{ "type": "text", "text": value.to_string() }],
                            "isError": false
                        }),
                    ),
                    wrote,
                },
                Err(e) => Response {
                    json: result(
                        id,
                        json!({
                            "content": [{ "type": "text", "text": e.to_string() }],
                            "isError": true
                        }),
                    ),
                    wrote,
                },
            }
        }

        other => Response {
            json: error(id, -32601, &format!("method not found: {other}")),
            wrote: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::mcp::config::McpConfig;
    use crate::mcp::log::CallLog;
    use serde_json::Value;

    fn ctx() -> (Connection, LocalePack, CallLog) {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrations::apply(&conn).unwrap();
        (conn, LocalePack::embedded_pt_br(), CallLog::new())
    }

    fn cfg_all_on() -> McpConfig {
        let mut c = McpConfig { enabled: true, tools: Default::default(), window_months: 0 };
        for (name, _) in crate::mcp::config::TOOL_DEFAULTS {
            c.tools.insert((*name).to_string(), true);
        }
        c
    }

    fn cfg_defaults() -> McpConfig {
        let mut c = McpConfig { enabled: true, tools: Default::default(), window_months: 12 };
        for (name, default) in crate::mcp::config::TOOL_DEFAULTS {
            c.tools.insert((*name).to_string(), *default);
        }
        c
    }

    fn call(cfg: &McpConfig, body: &str) -> Value {
        let (mut conn, pack, log) = ctx();
        let out = handle(&mut conn, &pack, cfg, &log, body);
        serde_json::from_str(&out.json).expect("a JSON-RPC response")
    }

    /// With no token, `origin_allowed` is the entire authentication story.
    /// The refused list is deliberately adversarial — decimal and octal
    /// spellings of a loopback address, userinfo tricks, an opaque "null"
    /// origin — so that a future "fix" (a `contains`, a lowercase-and-strip
    /// helper) that would quietly rewiden the check gets caught here first.
    #[test]
    fn origin_allowed_refuses_every_disguised_or_foreign_host() {
        let refused = [
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "null",
            "",
            "http://0.0.0.0:7717",
            "http://127.0.0.2:7717",
            "http://2130706433:7717",
            "http://0177.0.0.1:7717",
            "http://127.1:7717",
            "http://localhost@evil.example",
            "http://evil.example@localhost",
            "https://evil.example",
            "http://evil.example",
        ];
        for origin in refused {
            assert!(!origin_allowed(Some(origin)), "should refuse {origin}");
        }
    }

    /// A local agent sends no Origin at all; a browser always sends one. That
    /// asymmetry is the defense against a web page reaching the port.
    #[test]
    fn origin_allowed_allows_no_origin_and_every_real_local_host() {
        assert!(origin_allowed(None));

        let allowed = [
            "http://localhost",
            "http://localhost:7717",
            "http://127.0.0.1:7717",
            "http://[::1]:7717",
            "http://[::1]",
        ];
        for origin in allowed {
            assert!(origin_allowed(Some(origin)), "should allow {origin}");
        }
    }

    #[test]
    fn initialize_announces_the_protocol_version_and_tools() {
        let out = call(
            &cfg_all_on(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );

        assert_eq!(out["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(out["result"]["capabilities"]["tools"].is_object());
        assert_eq!(out["result"]["serverInfo"]["name"], "finoinc");
    }

    /// A notification has no id and gets no response — answering one is a
    /// protocol error, not a courtesy.
    #[test]
    fn a_notification_gets_no_response_at_all() {
        let (mut conn, pack, log) = ctx();

        let out = handle(
            &mut conn,
            &pack,
            &cfg_all_on(),
            &log,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );

        assert!(out.json.is_empty());
    }

    #[test]
    fn tools_list_returns_every_enabled_tool() {
        let out = call(&cfg_all_on(), r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);

        let tools = out["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 9);
        assert!(tools[0]["inputSchema"].is_object());
    }

    /// A disabled tool is not described, not listed, not hinted at.
    #[test]
    fn tools_list_omits_what_is_switched_off() {
        let out = call(&cfg_defaults(), r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);

        let names: Vec<&str> = out["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_transactions"));
        assert!(!names.contains(&"create_rule"), "writes are off by default");
        assert_eq!(names.len(), 6);
    }

    #[test]
    fn tools_call_runs_an_enabled_tool() {
        let out = call(
            &cfg_all_on(),
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_categories","arguments":{}}}"#,
        );

        assert!(out["result"]["content"][0]["text"].is_string());
        assert_eq!(out["result"]["isError"], false);
    }

    /// Calling a disabled tool is an error the agent can see — never a silent
    /// empty result, which would read as "there is nothing there".
    #[test]
    fn tools_call_on_a_disabled_tool_is_an_error() {
        let out = call(
            &cfg_defaults(),
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"create_rule","arguments":{"patterns":["X"],"category_id":1}}}"#,
        );

        assert_eq!(out["result"]["isError"], true);
    }

    #[test]
    fn an_unknown_method_is_a_jsonrpc_error() {
        let out = call(&cfg_all_on(), r#"{"jsonrpc":"2.0","id":5,"method":"resources/list"}"#);

        assert_eq!(out["error"]["code"], -32601);
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let out = call(&cfg_all_on(), "not json at all");

        assert_eq!(out["error"]["code"], -32700);
    }

    #[test]
    fn a_write_reports_that_it_wrote() {
        let (mut conn, pack, log) = ctx();
        conn.execute(
            "INSERT INTO categories (name, color_token, kind) VALUES ('Market', NULL, 'expense')",
            [],
        )
        .unwrap();

        let out = handle(
            &mut conn,
            &pack,
            &cfg_all_on(),
            &log,
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"create_rule","arguments":{"patterns":["MERCADO"],"category_id":1}}}"#,
        );

        assert!(out.wrote, "the webview has to be told its screens are stale");
    }

    #[test]
    fn a_read_does_not_report_a_write() {
        let (mut conn, pack, log) = ctx();

        let out = handle(
            &mut conn,
            &pack,
            &cfg_all_on(),
            &log,
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"list_categories","arguments":{}}}"#,
        );

        assert!(!out.wrote);
    }

    /// `wrote` is `ToolSpec.write` — a property of which tool was named — not
    /// something read off the outcome. This call fails outright (the category
    /// does not exist, so the `INSERT` trips the foreign key and the
    /// transaction rolls back, leaving nothing written), and `wrote` is still
    /// `true`: detection does not look at whether the call actually
    /// succeeded, only at which tool it was.
    #[test]
    fn a_write_that_fails_still_reports_that_it_wrote() {
        let (mut conn, pack, log) = ctx();

        let out = handle(
            &mut conn,
            &pack,
            &cfg_all_on(),
            &log,
            r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"create_rule","arguments":{"patterns":["MERCADO"],"category_id":999999}}}"#,
        );

        assert!(out.wrote, "a write tool reports wrote regardless of the outcome");
    }

    #[test]
    fn a_read_that_fails_does_not_report_a_write() {
        let (mut conn, pack, log) = ctx();

        let out = handle(
            &mut conn,
            &pack,
            &cfg_all_on(),
            &log,
            r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"get_month_summary","arguments":{"month":"not-a-month"}}}"#,
        );

        assert!(!out.wrote);
    }

    #[test]
    fn every_call_lands_in_the_log() {
        let (mut conn, pack, log) = ctx();

        handle(&mut conn, &pack, &cfg_all_on(), &log,
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"list_categories","arguments":{}}}"#);
        handle(&mut conn, &pack, &cfg_defaults(), &log,
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"create_rule","arguments":{}}}"#);

        let entries = log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].tool, "create_rule");
        assert!(!entries[0].ok, "a refused call is still a call");
        assert!(entries[1].ok);
    }

    /// `tools/list` is not a tool call — logging it would bury the calls that
    /// actually touched data under handshake noise.
    #[test]
    fn listing_tools_is_not_logged_as_a_call() {
        let (mut conn, pack, log) = ctx();

        handle(&mut conn, &pack, &cfg_all_on(), &log, r#"{"jsonrpc":"2.0","id":10,"method":"tools/list"}"#);

        assert!(log.entries().is_empty());
    }
}
