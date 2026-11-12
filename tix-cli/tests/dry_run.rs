//! `--dry-run` on the commands that delete or overwrite: each one reports
//! what the real run would do, fails where the real run would fail, and
//! leaves everything exactly as it found it.

mod common;

use common::{World, stderr, ticket_document};
use std::process::Output;

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A clean ticket: `destroy --dry-run` lists the prune and the deletion,
/// asks nothing, and deletes nothing.
#[test]
fn test_destroy_dry_run_touches_nothing() {
    let world = World::new();
    let ticket = world.setup("K");
    let before = ticket_document(&ticket);

    let output = world.tix(&["destroy", "--dry-run", ticket.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let plan = stdout(&output);
    assert!(plan.contains("would prune worktree api"), "{plan}");
    assert!(plan.contains("would destroy K"), "{plan}");
    assert!(!stderr(&output).contains("[y/N]"), "{}", stderr(&output));

    assert!(ticket.join("api").is_dir());
    assert_eq!(ticket_document(&ticket), before);
    assert_eq!(world.registered_worktrees().len(), 1);
}

/// A dirty worktree: the dry run fails as the real run would, and with
/// `--force` previews the forced destroy instead — still touching nothing.
#[test]
fn test_destroy_dry_run_dirty_worktree() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::write(ticket.join("api/wip.txt"), "work in progress").unwrap();
    let ticket_arg = ticket.to_str().unwrap();

    let output = world.tix(&["destroy", "--dry-run", ticket_arg]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("destroy would stop at worktree 'api'"),
        "{message}"
    );
    assert!(message.contains("1 uncommitted change"), "{message}");
    assert!(message.contains("--force"), "{message}");

    let output = world.tix(&["destroy", "--dry-run", "--force", ticket_arg]);
    assert!(output.status.success(), "{}", stderr(&output));
    let plan = stdout(&output);
    assert!(plan.contains("would prune worktree api"), "{plan}");
    assert!(
        plan.contains("discarding 1 uncommitted change (--force)"),
        "{plan}"
    );
    assert!(plan.contains("would destroy K"), "{plan}");

    assert!(ticket.join("api/wip.txt").is_file());
    assert_eq!(world.registered_worktrees().len(), 1);
}

/// `remove --dry-run` reports each outcome the real run would have —
/// including a refusal — and removes nothing.
#[test]
fn test_remove_dry_run() {
    let world = World::new();
    let ticket = world.setup("K");
    let ticket_arg = ticket.to_str().unwrap();
    let before = ticket_document(&ticket);

    let output = world.tix(&["remove", "api", "--dry-run", "--ticket", ticket_arg]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("would remove worktree api"),
        "{}",
        stdout(&output)
    );

    std::fs::write(ticket.join("api/wip.txt"), "work in progress").unwrap();
    let output = world.tix(&["remove", "api", "--dry-run", "--ticket", ticket_arg]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("would refuse to remove worktree 'api'"),
        "{message}"
    );
    assert!(message.contains("pass --force"), "{message}");

    assert!(ticket.join("api/wip.txt").is_file());
    assert_eq!(ticket_document(&ticket), before);
    assert_eq!(world.registered_worktrees().len(), 1);
}

/// A worktree deleted by hand is reported as dropped, and stays in the
/// ticket document until the real run.
#[test]
fn test_remove_dry_run_missing_worktree() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::remove_dir_all(ticket.join("api")).unwrap();
    let before = ticket_document(&ticket);

    let output = world.tix(&[
        "remove",
        "api",
        "--dry-run",
        "--ticket",
        ticket.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("missing on disk"),
        "{}",
        stdout(&output)
    );
    assert_eq!(ticket_document(&ticket), before);
    assert_eq!(world.registered_worktrees().len(), 1);
}

/// The config edits validate and report under `--dry-run`, fail where the
/// real write would, and never change a byte of the file.
#[test]
fn test_config_edits_dry_run() {
    let world = World::new();
    let config = world.root().join("config.toml");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(
        "\n[defaults]\nbranch_prefix = \"feature\" # keep\nrepositories = [\"api\", \"web\"]\n",
    );
    std::fs::write(&config, &text).unwrap();

    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "config",
                "set",
                "defaults.branch_prefix",
                "fix",
                "--dry-run",
            ],
            "would set defaults.branch_prefix = fix (replacing \"feature\"",
        ),
        (
            &[
                "config",
                "set",
                "defaults.default_repository_owner",
                "acme",
                "--dry-run",
            ],
            "currently unset",
        ),
        (
            &["config", "unset", "defaults.branch_prefix", "--dry-run"],
            "would unset defaults.branch_prefix (currently \"feature\"",
        ),
        (
            &[
                "config",
                "remove",
                "defaults.repositories",
                "web",
                "--dry-run",
            ],
            "leaving defaults.repositories = [\"api\"]",
        ),
    ];
    for (args, expected) in cases {
        let output = world.tix(args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        assert!(
            stdout(&output).contains(expected),
            "{args:?}: {}",
            stdout(&output)
        );
    }

    // What the real write rejects, the dry run rejects.
    for args in [
        &["config", "unset", "cli.code_directory", "--dry-run"][..],
        &["config", "set", "defaults.no_such_key", "1", "--dry-run"],
        &[
            "config",
            "remove",
            "defaults.repositories",
            "nope",
            "--dry-run",
        ],
    ] {
        let output = world.tix(args);
        assert!(!output.status.success(), "{args:?}: {}", stdout(&output));
    }

    assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
}

/// `config init --dry-run --force` prompts, then prints the would-be config
/// instead of overwriting the existing one.
#[test]
fn test_config_init_dry_run() {
    let world = World::new();
    let config = world.root().join("config.toml");
    let before = std::fs::read_to_string(&config).unwrap();

    let output = world.tix(&["config", "init", "--dry-run"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("already exists"),
        "{}",
        stderr(&output)
    );

    let output = world.tix_with_input(
        &["config", "init", "--dry-run", "--force"],
        "/tmp/t\n/tmp/c\n\n\n\n\n",
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let printed = stdout(&output);
    assert!(
        printed.contains("tickets_directory = \"/tmp/t\""),
        "{printed}"
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}\n{}", stdout(output), stderr(output)))
}

/// `-o json` prints the plan as data. A refused destroy still prints the
/// plan up to the refusal, marked as failing, alongside the non-zero exit;
/// with `--force` it counts what would be discarded.
#[test]
fn test_destroy_dry_run_json() {
    let world = World::new();
    let ticket = world.setup("K");
    let ticket_arg = ticket.to_str().unwrap();

    let output = world.tix(&["destroy", "--dry-run", "-o", "json", ticket_arg]);
    assert!(output.status.success(), "{}", stderr(&output));
    let plan = json(&output);
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["command"], "destroy");
    assert_eq!(plan["would_succeed"], true);
    let actions = plan["actions"].as_array().unwrap();
    assert_eq!(actions[0]["action"], "prune");
    assert_eq!(actions[0]["worktree"], "api");
    assert_eq!(actions[0]["repo"], "api");
    assert_eq!(actions[1]["action"], "delete-ticket");
    assert_eq!(actions[1]["ticket"], "K");

    std::fs::write(ticket.join("api/wip.txt"), "work in progress").unwrap();
    let output = world.tix(&["destroy", "--dry-run", "-o", "json", ticket_arg]);
    assert!(!output.status.success());
    let plan = json(&output);
    assert_eq!(plan["would_succeed"], false);
    assert_eq!(plan["actions"], serde_json::json!([]));
    assert!(stderr(&output).contains("destroy would stop at worktree 'api'"));

    let output = world.tix(&["destroy", "--dry-run", "--force", "-o", "json", ticket_arg]);
    assert!(output.status.success(), "{}", stderr(&output));
    let action = &json(&output)["actions"][0];
    assert_eq!(action["forced"], true);
    assert_eq!(action["uncommitted_changes"], 1);
    assert!(ticket.join("api/wip.txt").is_file());
}

/// The default output names the count too.
#[test]
fn test_remove_dry_run_force_counts_changes() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::write(ticket.join("api/a.txt"), "a").unwrap();
    std::fs::write(ticket.join("api/b.txt"), "b").unwrap();

    let output = world.tix(&[
        "remove",
        "api",
        "--dry-run",
        "--force",
        "--ticket",
        ticket.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("discarding 2 uncommitted changes"),
        "{}",
        stdout(&output)
    );
    assert!(ticket.join("api/a.txt").is_file());
}

/// `-o toml` prints the same plan as TOML.
#[test]
fn test_remove_dry_run_toml() {
    let world = World::new();
    let ticket = world.setup("K");
    let output = world.tix(&[
        "remove",
        "api",
        "--dry-run",
        "-o",
        "toml",
        "--ticket",
        ticket.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let plan: toml::Table = toml::from_str(&stdout(&output)).unwrap();
    assert_eq!(plan["command"].as_str(), Some("remove"));
    let action = &plan["actions"].as_array().unwrap()[0];
    assert_eq!(action["action"].as_str(), Some("remove"));
    assert_eq!(action["worktree"].as_str(), Some("api"));
}

/// Config plans carry values as data — a list as a list — and a rejected
/// edit is a failing plan.
#[test]
fn test_config_dry_run_json() {
    let world = World::new();
    let config = world.root().join("config.toml");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("\n[defaults]\nrepositories = [\"api\", \"web\"]\n");
    std::fs::write(&config, &text).unwrap();

    let output = world.tix(&[
        "config",
        "set",
        "defaults.repositories",
        "[\"api\"]",
        "--dry-run",
        "-o",
        "json",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let action = &json(&output)["actions"][0];
    assert_eq!(action["action"], "set");
    assert_eq!(action["value"], serde_json::json!(["api"]));
    assert_eq!(action["previous"], serde_json::json!(["api", "web"]));

    let output = world.tix(&[
        "config",
        "remove",
        "defaults.repositories",
        "web",
        "--dry-run",
        "-o",
        "json",
    ]);
    let action = &json(&output)["actions"][0];
    assert_eq!(action["value"], "web");
    assert_eq!(action["remaining"], serde_json::json!(["api"]));

    let output = world.tix(&[
        "config",
        "unset",
        "cli.code_directory",
        "--dry-run",
        "-o",
        "json",
    ]);
    assert!(!output.status.success());
    assert_eq!(json(&output)["would_succeed"], false);

    assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
}

/// `fmt` and `init` plan their documents in JSON; `fmt` plans nothing for
/// a file already in house style.
#[test]
fn test_document_dry_runs_json() {
    let world = World::new();
    let config = world.root().join("config.toml");

    let output = world.tix(&["config", "fmt", "--dry-run", "-o", "json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(json(&output)["actions"], serde_json::json!([]));

    // The collapsed inline-table shape fmt exists to undo (#146).
    let collapsed = format!(
        "defaults = {{ branch_prefix = \"x\" }}\n{}",
        std::fs::read_to_string(&config).unwrap()
    );
    std::fs::write(&config, &collapsed).unwrap();
    let output = world.tix(&["config", "fmt", "--dry-run", "-o", "json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let action = &json(&output)["actions"][0];
    assert_eq!(action["action"], "rewrite");
    assert!(
        action["formatted"].as_str().unwrap().contains("[defaults]"),
        "{action}"
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), collapsed);

    let output = world.tix_with_input(
        &["config", "init", "--dry-run", "--force", "-o", "json"],
        "/tmp/t\n/tmp/c\n\n\n\n\n",
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let action = &json(&output)["actions"][0];
    assert_eq!(action["action"], "overwrite");
    assert_eq!(action["content"]["cli"]["tickets_directory"], "/tmp/t");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), collapsed);
}
