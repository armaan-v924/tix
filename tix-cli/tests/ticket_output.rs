//! Run the JSON ticket workflows as a caller with piped input and output.

mod common;

use common::{World, git, stderr};
use serde_json::{Value, json};
use std::io::Write;
use std::process::Output;

fn report(output: &Output, code: i32) -> Value {
    assert_eq!(output.status.code(), Some(code), "{}", stderr(output));
    serde_json::from_slice(&output.stdout).expect("one JSON result on stdout")
}

#[test]
fn test_setup_partial_and_add_only_the_failed_repository() {
    let world = World::new();
    git(world.root(), &["clone", "-q", "origin.git", "code/web"]);
    git(&world.source(), &["branch", "api-only"]);
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(world.root().join("config.toml"))
        .unwrap();
    writeln!(
        config,
        "\n[engine.configured_repositories.web]\nremote = {:?}\ncode_path = {:?}",
        world.root().join("origin.git").to_str().unwrap(),
        world.root().join("code/web").to_str().unwrap(),
    )
    .unwrap();

    let output = world.tix(&[
        "setup",
        "K",
        "api",
        "web",
        "--base",
        "refs/heads/api-only",
        "-o",
        "json",
    ]);
    let partial = report(&output, 1);
    let path = partial["ticket_path"].as_str().unwrap();
    assert_eq!(path, world.ticket("K").to_str().unwrap());
    assert_eq!(partial["outcome"], "partial");
    assert_eq!(partial["worktrees"][0]["outcome"], "created");
    assert_eq!(partial["worktrees"][0]["name"], "api");
    assert_eq!(partial["worktrees"][1]["outcome"], "failed");
    let failed_repo = partial["worktrees"][1]["repo"].as_str().unwrap();
    assert_eq!(failed_repo, "web");

    let output = world.tix(&["ticket", "add", failed_repo, "--ticket", path, "-o", "json"]);
    let added = report(&output, 0);
    assert_eq!(added["outcome"], "succeeded");
    assert_eq!(added["worktrees"][0]["name"], "web");
    assert_eq!(added["worktrees"][0]["outcome"], "created");

    let output = world.tix(&["list", "-o", "json"]);
    let tickets = report(&output, 0);
    assert_eq!(tickets[0]["path"], path);
    assert_eq!(tickets[0]["worktrees"], 2);
}

#[test]
fn test_teardown_reports_failures_decline_and_success() {
    let world = World::new();
    let path = world.setup("K");
    let path = path.to_str().unwrap();
    let output = world.tix(&[
        "ticket",
        "remove",
        "unknown",
        "--ticket",
        path,
        "--dry-run",
        "-o",
        "json",
    ]);
    let preview = report(&output, 1);
    assert_eq!(preview["would_succeed"], false);
    assert_eq!(preview["ticket_path"], path);
    assert_eq!(preview["actions"], json!([]));
    assert_eq!(preview["error"]["code"], "worktree-not-found");

    std::fs::write(world.ticket("K").join("api/wip"), "keep me").unwrap();
    let output = world.tix(&["remove", "api", "--ticket", path, "-o", "json"]);
    let failed = report(&output, 1);
    assert_eq!(failed["outcome"], "failed");
    assert_eq!(failed["error"]["code"], "dirty-worktree");
    assert_eq!(failed["worktrees"][0]["outcome"], "failed");
    std::fs::remove_file(world.ticket("K").join("api/wip")).unwrap();

    let output = world.tix(&["destroy", path, "-o", "json"]);
    assert_eq!(report(&output, 0)["outcome"], "declined");
    assert!(world.ticket("K").exists());

    let output = world.tix_with_input(&["destroy", path, "-o", "json"], "y\n");
    let destroyed = report(&output, 0);
    assert_eq!(destroyed["outcome"], "succeeded");
    assert_eq!(destroyed["worktrees"][0]["outcome"], "removed");
    assert!(!world.ticket("K").exists());
}

#[test]
fn test_missing_config_reports_json_for_mutation_and_preview() {
    let world = World::new();
    let config = world.root().join("missing.toml");
    let config = config.to_str().unwrap();

    let output = world.tix(&["--config", config, "setup", "K", "-o", "json"]);
    let failed = report(&output, 1);
    assert_eq!(failed["outcome"], "failed");
    assert_eq!(failed["error"]["code"], "config-not-found");

    let output = world.tix(&[
        "--config",
        config,
        "destroy",
        "K",
        "--dry-run",
        "-o",
        "json",
    ]);
    let preview = report(&output, 1);
    assert_eq!(preview["would_succeed"], false);
    assert_eq!(preview["error"]["code"], "config-not-found");
}

#[test]
fn creation_reports_the_resolved_commit_and_fetch_outcome() {
    let world = World::new();
    let revision = |repo, rev| {
        String::from_utf8(git(repo, &["rev-parse", rev]).stdout)
            .unwrap()
            .trim()
            .to_string()
    };
    let source = world.source();
    let seed = world.root().join("seed");
    let original = revision(&source, "main");
    git(
        &seed,
        &["commit", "-q", "--allow-empty", "-m", "advance origin"],
    );
    git(&seed, &["push", "-q", "../origin.git", "main"]);
    let tip = revision(&seed, "main");
    assert_ne!(tip, original);
    assert_eq!(revision(&source, "origin/main"), original);

    let output = world.tix(&["setup", "K", "api", "-o", "json"]);
    let fetched = report(&output, 0);
    assert_eq!(fetched["worktrees"][0]["base"], "origin/main");
    assert_eq!(fetched["worktrees"][0]["base_commit"], tip);
    assert_eq!(fetched["worktrees"][0]["base_fetch"], "fetched");

    git(&source, &["remote", "set-url", "origin", "unavailable.git"]);
    let add = |name, branch, base| {
        let mut args = vec![
            "add", "--ticket", "K", "api", "--as", name, "--branch", branch, "-o", "json",
        ];
        if let Some(base) = base {
            args.extend(["--base", base]);
        }
        report(&world.tix(&args), 0)["worktrees"][0].clone()
    };
    let fallback = add("offline", "offline", Some("origin/main"));
    assert_eq!(fallback["base"], "origin/main");
    assert_eq!(fallback["base_commit"], tip);
    assert_eq!(fallback["base_fetch"], "local-fallback");

    let literal = add("literal", "literal", Some(original.as_str()));
    assert_eq!(literal["base"], original);
    assert_eq!(literal["base_commit"], original);
    assert_eq!(literal["base_fetch"], "not-needed");

    git(&source, &["branch", "existing", "origin/main"]);
    let reused = add("existing", "existing", None);
    for field in ["base", "base_commit", "base_fetch"] {
        assert!(reused.get(field).is_none());
    }

    git(&source, &["branch", "checked", "origin/main"]);
    let checked = add("checked", "checked", Some(original.as_str()));
    assert_eq!(checked["base_commit"], original);
    assert_eq!(checked["base_fetch"], "not-needed");
    assert_eq!(revision(&world.ticket("K").join("checked"), "HEAD"), tip);
}
