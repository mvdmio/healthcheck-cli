mod support;

use std::fs;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;
use support::{FakeServer, Reply, TOKEN, healthcheck, healthcheck_in, healthcheck_with_token};

fn env(server: &FakeServer) -> Vec<(&'static str, String)> {
    vec![
        ("HEALTHCHECK_URL", server.url()),
        ("HEALTHCHECK_TOKEN", TOKEN.into()),
    ]
}

fn api(server: &FakeServer, args: &[&str]) -> support::Run {
    healthcheck_with_token(server, &[&["api"], args].concat())
}

#[test]
fn get_sends_the_bearer_and_user_agent_and_prints_compact_json() {
    let server = FakeServer::start(|_| Reply::json(200, "{ \"items\": [ { \"id\": 1 } ] }"));

    let run = api(&server, &["GET", "/api/v1/projects?page=2&pageSize=5"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout, "{\"items\":[{\"id\":1}]}\n");
    let request = server.only_request();
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "/api/v1/projects?page=2&pageSize=5");
    assert_eq!(
        request.header("Authorization"),
        Some("Bearer mvdm_pat_test")
    );
    assert_eq!(
        request.header("User-Agent"),
        Some(concat!("healthcheck-cli/", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn a_trailing_slash_on_the_host_is_ignored() {
    let server = FakeServer::start(|_| Reply::json(200, "{}"));

    let run = healthcheck(
        &["api", "get", "/api/v1/projects"],
        &[
            ("HEALTHCHECK_URL", &format!("{}/", server.url())),
            ("HEALTHCHECK_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(server.only_request().url, "/api/v1/projects");
}

#[test]
fn inline_body_is_sent_as_json() {
    let server = FakeServer::start(|_| Reply::json(201, "{\"id\":2}"));

    let run = api(
        &server,
        &["POST", "/api/v1/projects", "--body", "{\"name\":\"Flood\"}"],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "id": 2 }));
    let request = server.only_request();
    assert_eq!(request.method, "POST");
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(request.body, b"{\"name\":\"Flood\"}");
}

#[test]
fn file_body_is_sent_as_json() {
    let server = FakeServer::start(|_| Reply::json(200, "{}"));
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("project.json"), "{\"name\":\"Fire\"}").unwrap();
    let env = env(&server);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let run = healthcheck_in(
        dir.path(),
        &[
            "api",
            "PATCH",
            "/api/v1/projects/1",
            "--body",
            "@project.json",
        ],
        &env,
    );

    assert_eq!(run.code, 0, "{run:#?}");
    let request = server.only_request();
    assert_eq!(request.method, "PATCH");
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(request.body, b"{\"name\":\"Fire\"}");
}

#[test]
fn a_missing_body_file_fails_locally() {
    let server = FakeServer::start(|_| Reply::json(200, "{}"));

    let run = api(
        &server,
        &["POST", "/api/v1/projects", "--body", "@no-such-file.json"],
    );

    assert_eq!(run.code, 1);
    assert_eq!(run.stderr_json()["error"], "file");
    assert!(server.requests().is_empty());
}

#[test]
fn an_empty_success_prints_ok_and_the_status() {
    let server = FakeServer::start(|_| Reply::empty(204));

    let run = api(&server, &["DELETE", "/api/v1/projects/1"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "ok": true, "status": 204 }));
}

#[test]
fn a_problem_lands_on_stderr_unchanged() {
    const PROBLEM: &str = "{\"type\":\"not-found\",\"title\":\"Not found\",\"status\":404,\"detail\":\"No Project 9.\"}";
    let server = FakeServer::start(|_| Reply::problem(404, PROBLEM));

    let run = api(&server, &["GET", "/api/v1/projects/9"]);

    assert_eq!(run.code, 1);
    assert_eq!(run.stderr.trim_end(), PROBLEM);
    assert!(run.stdout.is_empty());
}

#[test]
fn a_non_json_error_body_becomes_a_json_error() {
    let server =
        FakeServer::start(|_| Reply::with_type(502, "text/html", b"<html>Bad gateway</html>"));

    let run = api(&server, &["GET", "/api/v1/projects"]);

    assert_eq!(run.code, 1);
    assert_eq!(run.stderr_json()["error"], "http");
    assert!(run.stdout.is_empty());
}

#[test]
fn large_numbers_keep_their_precision() {
    let server = FakeServer::start(|_| {
        Reply::json(
            200,
            "{\"n\":123456789012345678901234567890,\"d\":0.1000000000000000055511151231257827}",
        )
    });

    let run = api(&server, &["GET", "/api/v1/projects"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        run.stdout,
        "{\"n\":123456789012345678901234567890,\"d\":0.1000000000000000055511151231257827}\n"
    );
}

#[test]
fn a_forbidden_is_an_error() {
    const PROBLEM: &str = "{\"type\":\"forbidden\",\"title\":\"Forbidden\",\"status\":403}";
    let server = FakeServer::start(|_| Reply::problem(403, PROBLEM));

    let run = api(&server, &["POST", "/api/v1/projects"]);

    assert_eq!(run.code, 1);
    assert_eq!(run.stderr.trim_end(), PROBLEM);
    assert!(run.stdout.is_empty());
}

#[test]
fn a_browser_handoff_problem_is_an_ordinary_problem_and_exits_1() {
    const PROBLEM: &str = "{\"type\":\"browser-handoff\",\"title\":\"Finish in the browser\",\"status\":403,\"handoffUrl\":\"https://auth.example/go\"}";
    let server = FakeServer::start(|_| Reply::problem(403, PROBLEM));

    let run = api(&server, &["POST", "/api/v1/projects"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr.trim_end(), PROBLEM);
    assert!(run.stdout.is_empty());
}

#[test]
fn usage_mistakes_exit_2_before_any_request() {
    let server = FakeServer::start(|_| Reply::json(200, "{}"));

    for args in [
        vec!["GET"],
        vec!["GET", "/api/v1/projects", "--unknown"],
        vec!["FETCH", "/api/v1/projects"],
        vec!["GET", "api/v1/projects"],
        vec!["GET", "/api/v1/projects with space"],
        vec!["POST", "/api/v1/projects", "--body", "{not json"],
    ] {
        let run = api(&server, &args);

        assert_eq!(run.code, 2, "{args:?}: {run:#?}");
        let error = run.stderr_json();
        assert_eq!(error["error"], "usage", "{args:?}");
        assert!(error["message"].as_str().is_some_and(|m| !m.is_empty()));
        assert!(run.stdout.is_empty());
    }
    assert!(server.requests().is_empty());
}

fn rate_limited_then_ok(limited: usize) -> FakeServer {
    let calls = AtomicUsize::new(0);
    FakeServer::start(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) < limited {
            Reply::problem(429, "{\"type\":\"rate-limited\",\"status\":429}")
                .header("Retry-After", "0")
        } else {
            Reply::json(200, "{\"done\":true}")
        }
    })
}

#[test]
fn two_rate_limits_then_success_retries_twice() {
    let server = rate_limited_then_ok(2);

    let run = api(&server, &["GET", "/api/v1/projects"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "done": true }));
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn four_rate_limits_give_up_after_three_retries() {
    let server = rate_limited_then_ok(4);

    let run = api(&server, &["GET", "/api/v1/projects"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["type"], "rate-limited");
    assert_eq!(server.requests().len(), 4);
}

#[test]
fn retry_after_as_a_past_http_date_retries_at_once() {
    let calls = AtomicUsize::new(0);
    let server = FakeServer::start(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::problem(429, "{\"type\":\"rate-limited\"}")
                .header("Retry-After", "Wed, 21 Oct 2015 07:28:00 GMT")
        } else {
            Reply::json(200, "{}")
        }
    });

    let run = api(&server, &["GET", "/api/v1/projects"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(server.requests().len(), 2);
}

const SCREENSHOT_PATH: &str = "/api/v1/projects/1/smoke-tests/2/runs/3/pages/4/screenshots/5/file";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x01\x02\xff binary";

fn png_server() -> FakeServer {
    FakeServer::start(|_| {
        Reply::with_type(200, "image/png", PNG).header(
            "Content-Disposition",
            "attachment; filename=shot.png; filename*=UTF-8''Scherm%20Q3.png",
        )
    })
}

#[test]
fn a_binary_response_without_out_is_refused() {
    let server = png_server();

    let run = api(&server, &["GET", SCREENSHOT_PATH]);

    assert_eq!(run.code, 1);
    assert!(run.stdout.is_empty());
    assert_eq!(run.stderr_json()["error"], "binary-response");
}

#[test]
fn a_binary_response_with_out_is_written_to_the_file() {
    let server = png_server();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.png");
    let path_text = path.to_str().unwrap();

    let run = api(&server, &["GET", SCREENSHOT_PATH, "--out", path_text]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(fs::read(&path).unwrap(), PNG);
    assert_eq!(
        run.stdout_json(),
        json!({
            "path": path_text,
            "bytes": PNG.len(),
            "contentType": "image/png",
            "fileName": "Scherm Q3.png",
        })
    );
}

#[test]
fn no_credential_fails_locally_and_names_both_ways_in() {
    let server = FakeServer::start(|_| Reply::json(200, "{}"));

    let run = healthcheck(
        &["api", "GET", "/api/v1/projects"],
        &[("HEALTHCHECK_URL", &server.url())],
    );

    assert_eq!(run.code, 1);
    let error = run.stderr_json();
    assert_eq!(error["error"], "not-signed-in");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("healthcheck login"), "{message}");
    assert!(message.contains("HEALTHCHECK_TOKEN"), "{message}");
    assert!(server.requests().is_empty());
}

#[test]
fn an_invalid_host_is_a_local_error_not_a_usage_mistake() {
    let run = healthcheck(
        &["api", "GET", "/api/v1/projects"],
        &[
            ("HEALTHCHECK_URL", "not a url"),
            ("HEALTHCHECK_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "config");
}

#[test]
fn an_unreachable_host_is_a_network_error() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let run = healthcheck(
        &["api", "GET", "/api/v1/projects"],
        &[
            ("HEALTHCHECK_URL", &format!("http://127.0.0.1:{port}")),
            ("HEALTHCHECK_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 1);
    assert_eq!(run.stderr_json()["error"], "network");
}
