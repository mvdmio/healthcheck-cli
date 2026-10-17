mod support;

use std::fs;
use std::path::Path;

use support::{FakeServer, Reply, healthcheck};

#[test]
fn skill_prints_the_repo_skill_file_without_a_request() {
    let server = FakeServer::start(|_| Reply::empty(500));
    let repo_file = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("SKILL.md"))
        .expect("read SKILL.md");

    let run = healthcheck(&["skill"], &[("HEALTHCHECK_URL", &server.url())]);

    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, repo_file);
    assert!(run.stderr.is_empty());
    assert!(server.requests().is_empty());
}

#[test]
fn skill_starts_with_front_matter_a_skill_loader_accepts() {
    let run = healthcheck(&["skill"], &[]);

    let front_matter = run
        .stdout
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front_matter, _)| front_matter)
        .expect("front matter between --- lines");
    assert!(front_matter.lines().any(|line| line == "name: healthcheck"));
    assert!(
        front_matter
            .lines()
            .any(|line| line.starts_with("description: ") && line.len() > "description: ".len())
    );
}

#[test]
fn skill_names_the_healthcheck_cli_its_token_and_its_install_lines() {
    let run = healthcheck(&["skill"], &[]);

    for expected in [
        "healthcheck",
        "HEALTHCHECK_TOKEN",
        "HEALTHCHECK_URL",
        "curl -LsSf https://healthcheck.mvdm.io/cli/install.sh | sh",
        "powershell -ExecutionPolicy ByPass -c \"irm https://healthcheck.mvdm.io/cli/install.ps1 | iex\"",
    ] {
        assert!(run.stdout.contains(expected), "skill lacks {expected}");
    }
}

#[test]
fn skill_covers_the_screenshot_download_with_out() {
    let run = healthcheck(&["skill"], &[]);

    assert!(run.stdout.contains("--out"));
    assert!(
        run.stdout
            .contains("healthcheck projects download-run-page-screenshot")
    );
}

#[test]
fn skill_mentions_no_cut_feature() {
    let run = healthcheck(&["skill"], &[]);
    let skill = run.stdout.to_lowercase();

    for absent in ["assistant", "handoff", "chat", "--file"] {
        assert!(!skill.contains(absent), "skill still mentions {absent}");
    }
    assert!(!run.stdout.contains("exit 3"));
    assert!(!run.stdout.contains("| 3 |"));
}
