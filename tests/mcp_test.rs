use std::io::Write;
use std::process::{Command, Stdio};

use mockito::{Matcher, Server};

fn mcp_call(base_url: &str, messages: &[&str]) -> String {
    let binary = env!("CARGO_BIN_EXE_bsctl");
    let mut child = Command::new(binary)
        .arg("--base-url")
        .arg(base_url)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start bsctl mcp");

    let stdin = child.stdin.as_mut().unwrap();
    // Initialize
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":0,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}},"clientInfo":{{"name":"test","version":"1.0"}}}}}}"#).unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    // Send tool calls
    for (i, msg) in messages.iter().enumerate() {
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":{},"method":"tools/call","params":{}}}"#,
            i + 1,
            msg
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    String::from_utf8(output.stdout).unwrap()
}

fn extract_result(output: &str, id: usize) -> serde_json::Value {
    for line in output.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("id").and_then(|v| v.as_u64()) == Some(id as u64) {
                return json;
            }
        }
    }
    panic!("No response for id {id} in: {output}");
}

fn result_text(resp: &serde_json::Value) -> String {
    resp.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
}

fn is_error(resp: &serde_json::Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("isError"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn mcp_catalog_list() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"kind":"Component","metadata":{"name":"svc1"},"spec":{"type":"service"}}]"#)
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"catalog_list","arguments":{}}"#],
    );
    let resp = extract_result(&output, 1);
    let text = result_text(&resp);

    assert!(text.contains("svc1"));
    assert!(!is_error(&resp));
    mock.assert();
}

#[test]
fn mcp_catalog_get() {
    let mut server = Server::new();
    let mock = server
        .mock(
            "GET",
            "/api/catalog/entities/by-name/component/default/my-svc",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"kind":"Component","metadata":{"name":"my-svc"},"spec":{}}"#)
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"catalog_get","arguments":{"entity_ref":"component:my-svc"}}"#],
    );
    let resp = extract_result(&output, 1);
    assert!(result_text(&resp).contains("my-svc"));
    mock.assert();
}

#[test]
fn mcp_catalog_refresh() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/api/catalog/refresh")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{}"#)
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"catalog_refresh","arguments":{"entity_ref":"component:my-svc"}}"#],
    );
    let resp = extract_result(&output, 1);
    assert!(result_text(&resp).contains("Refreshed"));
    mock.assert();
}

#[test]
fn mcp_search() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/search/query")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[{"type":"software-catalog","document":{"title":"found-it"}}]}"#)
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"search","arguments":{"term":"test"}}"#],
    );
    let resp = extract_result(&output, 1);
    assert!(result_text(&resp).contains("found-it"));
    mock.assert();
}

#[test]
fn mcp_template_list() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[{"metadata":{"name":"my-template","title":"My Template","description":"desc"}}]"#,
        )
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"template_list","arguments":{}}"#],
    );
    let resp = extract_result(&output, 1);
    assert!(result_text(&resp).contains("my-template"));
    mock.assert();
}

#[test]
fn mcp_catalog_facets() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entity-facets")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"facets":{"kind":[{"value":"Component","count":5}]}}"#)
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"catalog_facets","arguments":{"field":"kind"}}"#],
    );
    let resp = extract_result(&output, 1);
    assert!(result_text(&resp).contains("Component"));
    mock.assert();
}

#[test]
fn mcp_error_returns_is_error() {
    let mut server = Server::new();
    let mock = server
        .mock(
            "GET",
            "/api/catalog/entities/by-name/component/default/nope",
        )
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":{"name":"NotFoundError","message":"Not found"}}"#)
        .create();

    let output = mcp_call(
        &server.url(),
        &[r#"{"name":"catalog_get","arguments":{"entity_ref":"component:nope"}}"#],
    );
    let resp = extract_result(&output, 1);
    assert!(is_error(&resp));
    assert!(result_text(&resp).contains("Not found"));
    mock.assert();
}
