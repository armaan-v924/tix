//! Non-interactive CLI options work with closed stdin.

mod common;

use common::{World, stderr};
use serde_json::Value;

#[test]
fn test_destroy_yes_preserves_dirty_worktree_checks() {
    let world = World::new();
    let ticket = world.setup("K");
    let path = ticket.to_str().unwrap();
    let dirty = ticket.join("api/wip");
    std::fs::write(&dirty, "keep me").unwrap();

    let output = world.tix(&["destroy", path, "--yes"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("1 uncommitted change"));
    assert!(!stderr(&output).contains("[y/N]"));
    assert!(dirty.is_file());

    let output = world.tix(&["destroy", path, "--yes", "--force"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(dirty.is_file());

    std::fs::remove_file(dirty).unwrap();
    let output = world.tix(&["ticket", "destroy", path, "--yes", "-o", "json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stderr(&output).contains("[y/N]"));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "succeeded");
    assert!(!ticket.exists());
}

#[test]
fn test_config_init_explicit_inputs_preview_and_write() {
    let world = World::new();
    let path = world.root().join("new/config.toml");
    let mut args = vec![
        "--config",
        path.to_str().unwrap(),
        "config",
        "init",
        "--tickets-directory",
        "my tickets",
        "--code-directory",
        "my code",
        "--branch-prefix",
        "feature",
        "--github-base-url",
        "https://example.com/",
        "--default-repository-owner",
        "team",
        "--repositories",
        "api,web",
        "--repositories",
        "infra",
    ];
    let mut preview_args = args.clone();
    preview_args.extend(["--dry-run", "-o", "json"]);
    let preview = world.tix(&preview_args);
    assert!(preview.status.success(), "{}", stderr(&preview));
    assert!(preview.stderr.is_empty(), "{}", stderr(&preview));
    assert!(!path.parent().unwrap().exists());
    let plan: Value = serde_json::from_slice(&preview.stdout).unwrap();
    let content = &plan["actions"][0]["content"];
    assert_eq!(content["cli"]["tickets_directory"], "my tickets");
    assert_eq!(content["cli"]["code_directory"], "my code");
    assert_eq!(content["defaults"]["branch_prefix"], "feature");
    assert_eq!(
        content["defaults"]["github_base_url"],
        "https://example.com/"
    );
    assert_eq!(content["defaults"]["default_repository_owner"], "team");
    assert_eq!(
        content["defaults"]["repositories"],
        serde_json::json!(["api", "web", "infra"])
    );

    let apply = world.tix(&args);
    assert!(apply.status.success(), "{}", stderr(&apply));
    assert!(!stderr(&apply).contains("Tickets directory"));
    let show = world.tix(&[
        "--config",
        path.to_str().unwrap(),
        "config",
        "show",
        "-o",
        "json",
    ]);
    let saved: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(&saved, content);

    std::fs::write(&path, "# existing document\n").unwrap();
    let refused = world.tix(&args);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# existing document\n"
    );
    args.push("--force");
    let overwritten = world.tix(&args);
    assert!(overwritten.status.success(), "{}", stderr(&overwritten));
    assert_ne!(
        std::fs::read_to_string(&path).unwrap(),
        "# existing document\n"
    );
}

#[test]
fn test_config_init_incomplete_inputs_never_prompt() {
    let world = World::new();
    let path = world.root().join("new.toml");
    for options in [
        ["--tickets-directory", "tickets"],
        ["--branch-prefix", "feature"],
    ] {
        let mut args = vec!["--config", path.to_str().unwrap(), "config", "init"];
        args.extend(options);
        let output = world.tix(&args);
        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
        assert!(!stderr(&output).contains("Tickets directory"));
        assert!(!path.exists());
    }
}

#[cfg(unix)]
#[test]
fn test_no_plugins_suppresses_help_handshakes_and_dispatch() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    let world = World::new();
    let marker = world.root().join("invocations");
    let plugin = world.root().join("tix-fixture");
    std::fs::write(
        &plugin,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$TIX_PLUGIN_MARKER\"\nprintf 'fixture plugin\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&plugin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tix"))
            .args(args)
            .current_dir(world.root())
            .env("PATH", world.root())
            .env("TIX_CONFIG_PATH", world.root().join("config.toml"))
            .env("TIX_PLUGIN_MARKER", &marker)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };

    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("fixture plugin"));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "print-cli-help\n"
    );
    std::fs::remove_file(&marker).unwrap();

    for args in [["--no-plugins", "--help"], ["--help", "--no-plugins"]] {
        let help = run(&args);
        assert!(help.status.success(), "{}", stderr(&help));
        assert!(!String::from_utf8_lossy(&help.stdout).contains("fixture plugin"));
    }
    for args in [["--no-plugins", "fixture"], ["fixture", "--no-plugins"]] {
        let rejected = run(&args);
        assert_eq!(rejected.status.code(), Some(2), "{}", stderr(&rejected));
        assert!(stderr(&rejected).contains("external commands are disabled"));
    }
    let builtin = run(&["config", "show", "--no-plugins", "-o", "json"]);
    assert!(builtin.status.success(), "{}", stderr(&builtin));
    assert!(!marker.exists());
}
