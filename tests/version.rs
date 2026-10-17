mod support;

use support::healthcheck;

#[test]
fn version_prints_the_crate_version_as_plain_text() {
    let run = healthcheck(&["--version"], &[]);

    assert_eq!(run.code, 0);
    assert_eq!(
        run.stdout,
        format!("healthcheck {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn no_command_is_a_usage_mistake() {
    let run = healthcheck(&[], &[]);

    assert_eq!(run.code, 2);
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(run.stdout.is_empty());
}
