use mockito::{Matcher, Server};

mod helpers {
    use std::process::Command;

    pub struct BsctlRunner {
        pub base_url: String,
    }

    impl BsctlRunner {
        pub fn new(base_url: &str) -> Self {
            Self {
                base_url: base_url.to_string(),
            }
        }

        pub fn run(&self, args: &[&str]) -> std::process::Output {
            let binary = env!("CARGO_BIN_EXE_bsctl");
            Command::new(binary)
                .arg("--base-url")
                .arg(&self.base_url)
                .args(args)
                .env("NO_COLOR", "1")
                .output()
                .expect("Failed to run bsctl")
        }

        pub fn stdout(&self, args: &[&str]) -> String {
            let output = self.run(args);
            String::from_utf8(output.stdout).unwrap()
        }

        #[allow(dead_code)]
        pub fn stderr(&self, args: &[&str]) -> String {
            let output = self.run(args);
            String::from_utf8(output.stderr).unwrap()
        }
    }
}

#[test]
fn catalog_list_table() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[
                {"kind":"Component","metadata":{"name":"my-service","namespace":"default","description":"A service"},"spec":{"type":"service"}},
                {"kind":"Resource","metadata":{"name":"client-tc3","namespace":"default","description":"TC3 Client"},"spec":{"type":"client-account"}}
            ]"#,
        )
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["catalog", "list"]);

    assert!(output.contains("my-service"));
    assert!(output.contains("client-tc3"));
    assert!(output.contains("NAME"));
    mock.assert();
}

#[test]
fn catalog_list_with_kind_filter() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .match_query(Matcher::UrlEncoded("filter".into(), "kind=Resource".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"kind":"Resource","metadata":{"name":"r1"},"spec":{}}]"#)
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["catalog", "list", "--kind", "Resource"]);

    assert!(output.contains("r1"));
    assert!(output.contains("NAME"));
    mock.assert();
}

#[test]
fn catalog_list_with_type_filter() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .match_query(Matcher::UrlEncoded(
            "filter".into(),
            "spec.type=client-account".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"kind":"Resource","metadata":{"name":"client-tc3"},"spec":{"type":"client-account"}}]"#)
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["catalog", "list", "-t", "client-account"]);

    assert!(output.contains("client-tc3"));
    assert!(output.contains("TYPE"));
    mock.assert();
}

#[test]
fn catalog_list_json_output() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"kind":"Component","metadata":{"name":"svc1","namespace":"default","description":"Desc"},"spec":{"type":"service"}}]"#)
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["catalog", "list", "-o", "json"]);

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed[0]["name"], "svc1");
    assert_eq!(parsed[0]["type"], "service");
    mock.assert();
}

#[test]
fn catalog_get() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities/by-name/component/default/my-svc")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"kind":"Component","metadata":{"name":"my-svc","namespace":"default","description":"My Service","labels":{"env":"prod"}},"spec":{"lifecycle":"production","owner":"team-a","system":"platform"}}"#,
        )
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["catalog", "get", "component:default/my-svc"]);

    assert!(output.contains("Component:"));
    assert!(output.contains("my-svc"));
    assert!(output.contains("team-a"));
    assert!(output.contains("My Service"));
    mock.assert();
}

#[test]
fn catalog_get_default_namespace() {
    let mut server = Server::new();
    let mock = server
        .mock(
            "GET",
            "/api/catalog/entities/by-name/resource/default/client-tc3",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"kind":"Resource","metadata":{"name":"client-tc3"},"spec":{}}"#)
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["catalog", "get", "resource:client-tc3"]);

    assert!(output.contains("Resource:"));
    assert!(output.contains("client-tc3"));
    mock.assert();
}

#[test]
fn template_list() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/catalog/entities")
        .match_query(Matcher::UrlEncoded("filter".into(), "kind=Template".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[
                {"metadata":{"name":"tenant-creation","namespace":"default","title":"Create Tenant","description":"Creates a new tenant"}},
                {"metadata":{"name":"aws-account-creation","namespace":"default","title":"Create AWS Account"}}
            ]"#,
        )
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["template", "list"]);

    assert!(output.contains("tenant-creation"));
    assert!(output.contains("Create Tenant"));
    assert!(output.contains("aws-account-creation"));
    assert!(output.contains("NAME"));
    mock.assert();
}

#[test]
fn api_get() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/custom/endpoint")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"ok","count":42}"#)
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["api", "get", "/api/custom/endpoint"]);

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["count"], 42);
    mock.assert();
}

#[test]
fn api_error_handling() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/not-found")
        .with_status(404)
        .with_body("Not Found")
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.run(&["api", "get", "/api/not-found"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("404") || stderr.contains("API error"));
    mock.assert();
}

#[test]
fn search_query() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/api/search/query")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("term".into(), "tenant".into()),
            Matcher::UrlEncoded("limit".into(), "25".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"results":[{"type":"software-catalog","document":{"title":"tenant-tc3-dev-1","text":"TC3 Dev","location":"/catalog/default/resource/tenant-tc3-dev-1","kind":"Resource"}}],"numberOfResults":1}"#,
        )
        .create();

    let runner = helpers::BsctlRunner::new(&server.url());
    let output = runner.stdout(&["search", "query", "tenant"]);

    assert!(output.contains("tenant-tc3-dev-1"));
    assert!(output.contains("KIND"));
    mock.assert();
}
