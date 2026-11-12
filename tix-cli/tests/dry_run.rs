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
    assert!(plan.contains("would force-prune worktree api"), "{plan}");
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
