mod support;

use std::fs::{self, File};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use support::{
    DESCRIPTION_PATH, FakeServer, Recorded, Reply, Run, TOKEN, body_json, fixture, healthcheck_in,
};
use tempfile::TempDir;

const SCREENSHOT: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x01binary\xff";
const SCREENSHOT_ID: &str = "0b3f6c1e-8d2a-4c57-9e41-5a7d3f2b9c10";

/// A fake HealthCheck host that serves an API description which a test can change between runs, and one cache
/// folder shared by every run of the test.
struct Host {
    server: FakeServer,
    description: Arc<Mutex<Option<Value>>>,
    cache: TempDir,
}

impl Host {
    fn start() -> Self {
        let description = Arc::new(Mutex::new(Some(fixture())));
        let served = Arc::clone(&description);
        let server = FakeServer::start(move |request| {
            if request.path() == DESCRIPTION_PATH {
                return match served.lock().unwrap().as_ref() {
                    Some(document) => Reply::json(200, &document.to_string()),
                    None => Reply::empty(503),
                };
            }
            if request.path().ends_with("/file") {
                return Reply::with_type(200, "image/png", SCREENSHOT)
                    .header("Content-Disposition", "attachment; filename=\"shot.png\"");
            }
            Reply::json(200, "{\"done\":true}")
        });
        Host {
            server,
            description,
            cache: tempfile::tempdir().expect("a cache folder"),
        }
    }

    fn serve(&self, document: Option<Value>) {
        *self.description.lock().unwrap() = document;
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_in(&std::env::temp_dir(), args)
    }

    fn run_in(&self, dir: &std::path::Path, args: &[&str]) -> Run {
        let url = self.server.url();
        let cache = self.cache.path().to_str().expect("a UTF-8 path");
        healthcheck_in(
            dir,
            args,
            &[
                ("HEALTHCHECK_URL", &url),
                ("HEALTHCHECK_TOKEN", TOKEN),
                ("HEALTHCHECK_CACHE_DIR", cache),
            ],
        )
    }

    fn description_fetches(&self) -> usize {
        self.server
            .requests()
            .iter()
            .filter(|request| request.path() == DESCRIPTION_PATH)
            .count()
    }

    fn api_requests(&self) -> Vec<Recorded> {
        self.server
            .requests()
            .into_iter()
            .filter(|request| request.path() != DESCRIPTION_PATH)
            .collect()
    }

    fn only_api_request(&self) -> Recorded {
        let requests = self.api_requests();
        assert_eq!(requests.len(), 1, "requests: {requests:#?}");
        requests.into_iter().next().unwrap()
    }

    /// Makes every cached copy look `age` old.
    fn age_cache(&self, age: Duration) {
        for entry in fs::read_dir(self.cache.path()).expect("the cache folder") {
            let file = File::options()
                .write(true)
                .open(entry.expect("a cache entry").path())
                .expect("open the cached copy");
            file.set_modified(SystemTime::now() - age)
                .expect("age the cached copy");
        }
    }
}

/// The fixture with one more operation, as a later release of HealthCheck would add it.
fn with_operation(group_action: &str, path: &str) -> Value {
    let mut document = fixture();
    document["paths"][path] = json!({
        "post": {
            "tags": ["Projects"],
            "summary": "A new operation",
            "operationId": group_action,
            "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }],
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    document
}

/// The fixture with an operation whose query parameter and body fields take names the CLI already uses.
fn with_clashing_names() -> Value {
    let mut document = fixture();
    document["paths"]["/api/v1/projects/{id}/annotate"] = json!({
        "patch": {
            "tags": ["Projects"],
            "summary": "Annotate a Project",
            "operationId": "projects.annotate",
            "parameters": [
                { "name": "id", "in": "path", "required": true, "schema": { "type": "integer" } },
                { "name": "notify", "in": "query", "schema": { "type": "boolean" } }
            ],
            "requestBody": {
                "required": true,
                "content": { "application/json": { "schema": {
                    "type": "object",
                    "required": ["out"],
                    "properties": {
                        "notify": { "type": "boolean" },
                        "out": { "type": "string" }
                    }
                } } }
            },
            "responses": { "200": { "description": "OK", "content": { "application/json": {} } } }
        }
    });
    document
}

/// The names under `Commands:` in a `--help` answer.
fn listed_commands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| line.trim_end() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[test]
fn an_action_with_a_path_parameter_sends_its_method_and_path() {
    let host = Host::start();

    let run = host.run(&["projects", "get", "7"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "done": true }));
    let request = host.only_api_request();
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "/api/v1/projects/7");
    assert_eq!(
        request.header("Authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert!(request.body.is_empty());
}

#[test]
fn path_parameters_follow_the_path_order() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "update-url-check",
        "12",
        "34",
        "--name",
        "Home page",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "PATCH");
    assert_eq!(request.url, "/api/v1/projects/12/url-checks/34");
    assert_eq!(body_json(&request), json!({ "name": "Home page" }));
}

#[test]
fn query_options_are_typed_and_encoded() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "list-run-pages",
        "1",
        "2",
        "3",
        "--page",
        "2",
        "--page-size",
        "50",
        "--search",
        "sign in&out",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.only_api_request();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.path(),
        "/api/v1/projects/1/smoke-tests/2/runs/3/pages"
    );
    assert_eq!(request.form("page").as_deref(), Some("2"));
    assert_eq!(request.form("pageSize").as_deref(), Some("50"));
    assert_eq!(request.form("search").as_deref(), Some("sign in&out"));
    assert!(
        request.url.contains("search=sign%20in%26out"),
        "{}",
        request.url
    );
}

#[test]
fn an_integer_option_refuses_text_before_any_request() {
    let host = Host::start();

    let option = host.run(&[
        "projects",
        "list-run-pages",
        "1",
        "2",
        "3",
        "--page",
        "five",
    ]);
    let path = host.run(&["projects", "get", "seven"]);

    assert_eq!(option.code, 2, "{option:#?}");
    assert_eq!(option.stderr_json()["error"], "usage");
    assert_eq!(path.code, 2, "{path:#?}");
    assert_eq!(path.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn body_options_are_typed_from_the_schema() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "set-email-settings",
        "1",
        "7",
        "--availability-change",
        "true",
        "--dns-change",
        "false",
        "--weekly-report",
        "true",
    ]);
    let nested = host.run(&[
        "projects",
        "create-smoke-test-input",
        "1",
        "2",
        "--url",
        "https://shop.example/login",
        "--fields",
        "{\"name\":\"email\",\"type\":\"text\",\"value\":\"ada@example.test\"}",
        "--fields",
        "{\"name\":\"remember\",\"type\":\"checkbox\",\"value\":\"on\"}",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(nested.code, 0, "{nested:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].url, "/api/v1/projects/1/email-settings/7");
    assert_eq!(requests[0].header("Content-Type"), Some("application/json"));
    assert_eq!(
        body_json(&requests[0]),
        json!({ "availabilityChange": true, "dnsChange": false, "weeklyReport": true })
    );
    assert_eq!(requests[1].method, "POST");
    assert_eq!(
        body_json(&requests[1]),
        json!({
            "url": "https://shop.example/login",
            "fields": [
                { "name": "email", "type": "text", "value": "ada@example.test" },
                { "name": "remember", "type": "checkbox", "value": "on" },
            ],
        })
    );
}

#[test]
fn a_boolean_option_refuses_anything_but_true_or_false() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "set-email-settings",
        "1",
        "7",
        "--dns-change",
        "maybe",
    ]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn field_options_win_over_the_same_keys_in_body() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "create-url-check",
        "1",
        "--body",
        "{\"name\":\"Old\",\"url\":\"https://shop.example\"}",
        "--name",
        "New",
    ]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "name": "New", "url": "https://shop.example" })
    );
}

#[test]
fn body_reads_a_file() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("project.json"),
        "{\"name\":\"From a file\"}",
    )
    .unwrap();

    let run = host.run_in(
        dir.path(),
        &["projects", "create", "--body", "@project.json"],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        body_json(&host.only_api_request()),
        json!({ "name": "From a file" })
    );
}

#[test]
fn a_required_body_with_no_input_is_an_empty_object_and_an_optional_one_is_left_out() {
    let host = Host::start();
    let mut optional = fixture();
    optional["paths"]["/api/v1/projects/{id}"]["patch"]["requestBody"]["required"] = json!(false);

    let required_run = host.run(&["projects", "create"]);
    host.serve(Some(optional));
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    let optional_run = host.run(&["projects", "update", "1"]);

    assert_eq!(required_run.code, 0, "{required_run:#?}");
    assert_eq!(optional_run.code, 0, "{optional_run:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].body, b"{}");
    assert_eq!(requests[1].method, "PATCH");
    assert!(requests[1].body.is_empty());
    assert_eq!(requests[1].header("Content-Type"), None);
}

#[test]
fn a_clash_renames_the_later_option_and_help_says_so() {
    let host = Host::start();
    host.serve(Some(with_clashing_names()));

    let run = host.run(&[
        "projects",
        "annotate",
        "1",
        "--notify",
        "true",
        "--field-notify",
        "false",
        "--field-out",
        "Transfer",
    ]);
    let help = host.run(&["projects", "annotate", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let request = host.api_requests().remove(0);
    assert_eq!(request.path(), "/api/v1/projects/1/annotate");
    assert_eq!(request.form("notify").as_deref(), Some("true"));
    assert_eq!(
        body_json(&request),
        json!({ "notify": false, "out": "Transfer" })
    );
    for expected in [
        "named --field-notify because --notify is taken",
        "named --field-out because --out is taken",
        "Required in the body.",
    ] {
        assert!(
            help.stdout.contains(expected),
            "{expected}: {}",
            help.stdout
        );
    }
}

#[test]
fn help_on_an_action_shows_its_summary_description_and_parameters() {
    let host = Host::start();

    let get = host.run(&["projects", "get", "--help"]);
    let settings = host.run(&["projects", "set-email-settings", "--help"]);
    let pages = host.run(&["projects", "list-run-pages", "--help"]);

    assert_eq!(get.code, 0, "{get:#?}");
    for expected in [
        "Get a Project",
        "Returns the Project with its name and the slug of its public status page.",
        "Type: integer.",
    ] {
        assert!(get.stdout.contains(expected), "{expected}: {}", get.stdout);
    }
    for expected in [
        "--availability-change <BOOLEAN>",
        "--weekly-report <BOOLEAN>",
        "--body <JSON|@FILE>",
    ] {
        assert!(
            settings.stdout.contains(expected),
            "{expected}: {}",
            settings.stdout
        );
    }
    for expected in [
        "--page <INTEGER>",
        "--page-size <INTEGER>",
        "--search <STRING>",
    ] {
        assert!(
            pages.stdout.contains(expected),
            "{expected}: {}",
            pages.stdout
        );
    }
    assert!(host.api_requests().is_empty());
}

#[test]
fn help_on_a_group_lists_its_actions_and_its_tags() {
    let host = Host::start();

    let run = host.run(&["projects", "--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in [
        "list-run-pages",
        "create-url-check",
        "download-run-page-screenshot",
        "Download a screenshot",
        "Projects, Smoke Test Inputs, Smoke Test Runs, Smoke Tests, URL checks",
    ] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
}

#[test]
fn help_lists_the_hand_written_commands_and_the_generated_groups() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let commands = listed_commands(&run.stdout);
    for expected in [
        "login", "logout", "status", "accounts", "api", "skill", "projects",
    ] {
        assert!(
            commands.iter().any(|command| command == expected),
            "{expected}: {}",
            run.stdout
        );
    }
}

#[test]
fn help_lists_no_chat_command() {
    let host = Host::start();

    let help = host.run(&["--help"]);
    let chat = host.run(&["chat", "send", "hello"]);

    assert_eq!(help.code, 0, "{help:#?}");
    assert!(
        !listed_commands(&help.stdout).iter().any(|c| c == "chat"),
        "{}",
        help.stdout
    );
    let lowered = help.stdout.to_lowercase();
    assert!(!lowered.contains("chat"), "{}", help.stdout);
    assert!(!lowered.contains("assistant"), "{}", help.stdout);
    assert_eq!(chat.code, 2, "{chat:#?}");
    assert_eq!(chat.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn no_generated_action_offers_file() {
    let host = Host::start();
    let top = host.run(&["--help"]);
    let mut checked = 0;

    for group in listed_commands(&top.stdout) {
        if group == "help" {
            continue;
        }
        let group_help = host.run(&[&group, "--help"]);
        assert_eq!(group_help.code, 0, "{group}: {group_help:#?}");
        for action in listed_commands(&group_help.stdout) {
            if action == "help" {
                continue;
            }
            let help = host.run(&[&group, &action, "--help"]);
            assert_eq!(help.code, 0, "{group} {action}: {help:#?}");
            assert!(
                !help.stdout.contains("--file"),
                "{group} {action}: {}",
                help.stdout
            );
            checked += 1;
        }
    }

    // The fixture's eleven generated `projects` actions, and the two `accounts` actions beside them.
    assert_eq!(checked, 13, "{checked} actions were walked");
    assert!(host.api_requests().is_empty());
}

#[test]
fn a_second_run_within_the_hour_uses_the_cached_copy() {
    let host = Host::start();

    host.run(&["projects", "get", "1"]);
    let second = host.run(&["projects", "get", "2"]);

    assert_eq!(second.code, 0, "{second:#?}");
    assert_eq!(host.description_fetches(), 1);
}

#[test]
fn a_copy_older_than_an_hour_is_fetched_again() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.age_cache(Duration::from_secs(61 * 60));

    let run = host.run(&["projects", "get", "2"]);
    let after = host.run(&["projects", "get", "3"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(after.code, 0, "{after:#?}");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_stale_copy_is_used_when_the_fetch_fails() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["projects", "get", "2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/projects/2");
}

#[test]
fn no_copy_and_no_fetch_is_a_network_error() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["projects", "get", "1"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "network");
    assert!(host.api_requests().is_empty());
}

#[test]
fn hand_written_commands_never_fetch_the_description() {
    let host = Host::start();
    host.serve(None);

    let api = host.run(&["api", "GET", "/api/v1/projects"]);
    let accounts = host.run(&["accounts", "list"]);

    assert_eq!(api.code, 0, "{api:#?}");
    assert_eq!(accounts.code, 0, "{accounts:#?}");
    assert_eq!(host.description_fetches(), 0);
}

#[test]
fn an_operation_added_after_caching_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.serve(Some(with_operation(
        "projects.archive",
        "/api/v1/projects/{id}/archive",
    )));

    let run = host.run(&["projects", "archive", "1"]);
    let again = host.run(&["projects", "archive", "2"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(again.code, 0, "{again:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/projects/1/archive");
    assert_eq!(host.description_fetches(), 2);
}

#[test]
fn a_new_group_works_through_the_refetch() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.serve(Some(with_operation(
        "reports.generate",
        "/api/v1/reports/{id}/generate",
    )));

    let run = host.run(&["reports", "generate", "q3"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(host.api_requests()[1].url, "/api/v1/reports/q3/generate");
}

#[test]
fn a_command_still_unknown_after_the_refetch_is_a_usage_mistake() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);

    let action = host.run(&["projects", "explode", "1"]);
    let group = host.run(&["dragons", "list"]);

    assert_eq!(action.code, 2, "{action:#?}");
    assert_eq!(action.stderr_json()["error"], "usage");
    assert_eq!(group.code, 2, "{group:#?}");
    assert_eq!(host.description_fetches(), 3);
    assert_eq!(host.api_requests().len(), 1);
}

#[test]
fn the_hand_written_accounts_commands_win_over_the_generated_ones() {
    let host = Host::start();

    let list = host.run(&["accounts", "list"]);
    let switch = host.run(&["accounts", "switch", "2"]);
    let help = host.run(&["accounts", "--help"]);
    let list_help = host.run(&["accounts", "list", "--help"]);

    assert_eq!(list.code, 0, "{list:#?}");
    assert_eq!(switch.code, 0, "{switch:#?}");
    let requests = host.api_requests();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].url, "/api/v1/accounts");
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].url, "/api/v1/accounts/2/switch");
    let actions = listed_commands(&help.stdout);
    for name in ["list", "switch"] {
        assert_eq!(
            actions.iter().filter(|action| *action == name).count(),
            1,
            "{}",
            help.stdout
        );
    }
    // The generated operation would describe itself with the API description's text.
    assert!(
        !list_help.stdout.contains("List the User's Accounts"),
        "{}",
        list_help.stdout
    );
}

#[test]
fn a_download_writes_the_bytes_to_out_and_prints_a_summary() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();

    let run = host.run_in(
        dir.path(),
        &[
            "projects",
            "download-run-page-screenshot",
            "1",
            "2",
            "3",
            "4",
            SCREENSHOT_ID,
            "--out",
            "shot.png",
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(fs::read(dir.path().join("shot.png")).unwrap(), SCREENSHOT);
    assert_eq!(
        run.stdout_json(),
        json!({
            "path": "shot.png",
            "bytes": SCREENSHOT.len(),
            "contentType": "image/png",
            "fileName": "shot.png",
        })
    );
    let request = host.only_api_request();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.url,
        format!("/api/v1/projects/1/smoke-tests/2/runs/3/pages/4/screenshots/{SCREENSHOT_ID}/file")
    );
}

#[test]
fn path_parameters_are_percent_encoded() {
    let host = Host::start();
    let dir = tempfile::tempdir().unwrap();

    let run = host.run_in(
        dir.path(),
        &[
            "projects",
            "download-run-page-screenshot",
            "1",
            "2",
            "3",
            "4",
            "a b/c",
            "--out",
            "shot.png",
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(
        host.only_api_request()
            .url
            .ends_with("/screenshots/a%20b%2Fc/file"),
        "{:#?}",
        host.api_requests()
    );
}

#[test]
fn a_download_without_out_is_a_usage_mistake_and_sends_no_request() {
    let host = Host::start();

    let run = host.run(&[
        "projects",
        "download-run-page-screenshot",
        "1",
        "2",
        "3",
        "4",
        SCREENSHOT_ID,
    ]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn out_on_a_command_that_answers_json_is_a_usage_mistake() {
    let host = Host::start();

    let run = host.run(&["projects", "get", "1", "--out", "project.json"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(host.api_requests().is_empty());
}

#[test]
fn a_generated_browser_handoff_answer_exits_1_with_the_problem() {
    let server = FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(
            403,
            "{\"type\":\"browser-handoff\",\"title\":\"Finish in the browser\",\"status\":403,\"detail\":\"Confirm in the browser.\",\"handoffUrl\":\"https://example.test/go\"}",
        )
    });

    let run = healthcheck_in(
        &std::env::temp_dir(),
        &["projects", "get", "1"],
        &[
            ("HEALTHCHECK_URL", &server.url()),
            ("HEALTHCHECK_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    let problem = run.stderr_json();
    assert_eq!(problem["type"], "browser-handoff");
    assert_eq!(problem["handoffUrl"], "https://example.test/go");
    assert!(run.stdout.is_empty());
}

#[test]
fn a_generated_not_found_prints_the_problem_and_exits_1() {
    let server = FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(
            404,
            "{\"type\":\"not-found\",\"title\":\"Not found\",\"status\":404}",
        )
    });

    let run = healthcheck_in(
        &std::env::temp_dir(),
        &["projects", "get", "99"],
        &[
            ("HEALTHCHECK_URL", &server.url()),
            ("HEALTHCHECK_TOKEN", TOKEN),
        ],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["type"], "not-found");
    assert!(run.stdout.is_empty());
}

#[test]
fn help_without_the_description_lists_the_hand_written_commands_and_says_what_is_missing() {
    let host = Host::start();
    host.serve(None);

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(run.stdout.contains("login"), "{}", run.stdout);
    assert!(!run.stdout.contains("projects"), "{}", run.stdout);
    assert!(
        run.stdout.contains("openapi/v1.json are missing"),
        "{}",
        run.stdout
    );
    assert_ends_with_the_docs_address(&host, &run);
}

fn assert_ends_with_the_docs_address(host: &Host, run: &Run) {
    let expected = format!("Docs: {}/agents", host.server.url());
    assert!(run.stdout.trim_end().ends_with(&expected), "{}", run.stdout);
}

#[test]
fn help_ends_with_the_docs_address_on_the_current_host() {
    let host = Host::start();

    let run = host.run(&["--help"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_ends_with_the_docs_address(&host, &run);
}

#[test]
fn an_unknown_command_with_a_failed_fetch_does_not_fetch_twice() {
    let host = Host::start();
    host.run(&["projects", "get", "1"]);
    host.age_cache(Duration::from_secs(2 * 60 * 60));
    host.serve(None);

    let run = host.run(&["projects", "explode", "1"]);

    assert_eq!(run.code, 2, "{run:#?}");
    assert_eq!(host.description_fetches(), 2);
}
