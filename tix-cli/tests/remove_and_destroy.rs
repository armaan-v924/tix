//! End-to-end `tix remove` and `tix destroy` against real repositories:
//! worktrees deleted by hand, and the wording of their refusals.
//!
//! Each test builds its own world in a temp directory — a bare "origin", a
//! clone registered as `api`, and a global config pointing at both — and
//! drives the built `tix` binary through it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

struct World {
    dir: TempDir,
}

impl World {
    /// A registered, cloned `api` repository and an empty tickets directory.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("tickets")).unwrap();

        git(root, &["init", "-q", "-b", "main", "seed"]);
        git(
            &root.join("seed"),
            &["commit", "-q", "--allow-empty", "-m", "initial commit"],
        );
        git(root, &["clone", "-q", "--bare", "seed", "origin.git"]);
        git(root, &["clone", "-q", "origin.git", "code/api"]);

        let config = format!(
            "[cli]\ntickets_directory = {:?}\ncode_directory = {:?}\n\n\
             [engine.configured_repositories.api]\nremote = {:?}\ncode_path = {:?}\n",
            root.join("tickets").to_str().unwrap(),
            root.join("code").to_str().unwrap(),
            root.join("origin.git").to_str().unwrap(),
            root.join("code/api").to_str().unwrap(),
        );
        std::fs::write(root.join("config.toml"), config).unwrap();
        Self { dir }
    }

    fn ticket(&self, key: &str) -> PathBuf {
        self.dir.path().join("tickets").join(key)
    }

    fn source(&self) -> PathBuf {
        self.dir.path().join("code/api")
    }

    /// Runs `tix <args>` from the world's root, with `stdin` fed to it.
    fn tix_with_input(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_tix"))
            .args(args)
            .current_dir(self.dir.path())
            .env("PWD", self.dir.path())
            .env("TIX_CONFIG_PATH", self.dir.path().join("config.toml"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn tix(&self, args: &[&str]) -> Output {
        self.tix_with_input(args, "")
    }

    /// `tix setup <key> api`, asserting it worked.
    fn setup(&self, key: &str) -> PathBuf {
        let output = self.tix(&["setup", key, "api"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let ticket = self.ticket(key);
        assert!(ticket.join("api").is_dir());
        ticket
    }

    /// The worktree paths git has registered for the source repository,
    /// besides the source checkout itself.
    fn registered_worktrees(&self) -> Vec<String> {
        let output = git(&self.source(), &["worktree", "list", "--porcelain"]);
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .skip(1)
            .map(str::to_string)
            .collect()
    }
}

fn git(cwd: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@test.com"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn ticket_document(ticket: &Path) -> String {
    std::fs::read_to_string(ticket.join(".tix/ticket.toml")).unwrap()
}

/// Whether the git CLI should prune its registrations before tix runs.
#[derive(Clone, Copy)]
enum Prune {
    Before,
    No,
}

/// `tix remove` on a worktree whose directory was deleted by hand drops the
/// entry and warns, whether or not git still has it registered.
fn remove_missing(prune: Prune) {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::remove_dir_all(ticket.join("api")).unwrap();
    if let Prune::Before = prune {
        git(&world.source(), &["worktree", "prune"]);
    }

    let output = world.tix(&["remove", "api", "--ticket", ticket.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("missing on disk"),
        "{}",
        stderr(&output)
    );
    assert!(!ticket_document(&ticket).contains("api"));
    assert!(world.registered_worktrees().is_empty());
    assert!(ticket.join(".tix/ticket.toml").is_file());

    let info = world.tix(&["info", "--ticket", ticket.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&info.stdout).contains("no worktrees"));
}

#[test]
fn test_remove_missing_worktree_still_registered() {
    remove_missing(Prune::No);
}

#[test]
fn test_remove_missing_worktree_after_git_prune() {
    remove_missing(Prune::Before);
}

/// `tix destroy`, without `--force`, tears down a ticket whose worktree was
/// deleted by hand, and prunes git's registration on the way.
fn destroy_missing(prune: Prune) {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::remove_dir_all(ticket.join("api")).unwrap();
    if let Prune::Before = prune {
        git(&world.source(), &["worktree", "prune"]);
    }

    let output = world.tix_with_input(&["destroy", ticket.to_str().unwrap()], "y\n");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("missing on disk"),
        "{}",
        stderr(&output)
    );
    assert!(!ticket.exists());
    assert!(world.registered_worktrees().is_empty());
}

#[test]
fn test_destroy_missing_worktree_still_registered() {
    destroy_missing(Prune::No);
}

#[test]
fn test_destroy_missing_worktree_after_git_prune() {
    destroy_missing(Prune::Before);
}

/// A worktree directory that is no longer a checkout loses its entry and
/// registration on `remove`, but its contents stay on disk.
#[test]
fn test_remove_not_a_git_repository_keeps_directory() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::remove_file(ticket.join("api/.git")).unwrap();
    std::fs::write(ticket.join("api/notes.txt"), "keep me").unwrap();

    let output = world.tix(&["remove", "api", "--ticket", ticket.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("left"), "{}", stderr(&output));
    assert!(ticket.join("api/notes.txt").is_file());
    assert!(!ticket_document(&ticket).contains("api"));
    assert!(world.registered_worktrees().is_empty());
}

/// `destroy` would delete such a directory unchecked, so it refuses without
/// `--force` — before asking, and without touching anything.
#[test]
fn test_destroy_not_a_git_repository_needs_force() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::remove_file(ticket.join("api/.git")).unwrap();

    let output = world.tix_with_input(&["destroy", ticket.to_str().unwrap()], "y\n");
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("not a git repository"), "{message}");
    assert!(message.contains("--force"), "{message}");
    assert!(!message.contains("[y/N]"), "{message}");
    assert!(ticket.join("api").is_dir());

    let output = world.tix(&["destroy", "--force", ticket.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!ticket.exists());
}

/// A dirty worktree: `remove` says how to get past it in its own terms,
/// and `--force` does.
#[test]
fn test_remove_dirty_worktree_wording_and_force() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::write(ticket.join("api/wip.txt"), "work in progress").unwrap();
    let ticket_arg = ticket.to_str().unwrap();

    let output = world.tix(&["remove", "api", "--ticket", ticket_arg]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("1 uncommitted change"), "{message}");
    assert!(message.contains("commit or stash"), "{message}");
    assert!(message.contains("pass --force"), "{message}");
    assert!(!message.contains("use force"), "{message}");
    assert!(ticket.join("api/wip.txt").is_file());
    assert!(ticket_document(&ticket).contains("api"));

    let output = world.tix(&["remove", "api", "--force", "--ticket", ticket_arg]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!ticket.join("api").exists());
    assert!(!ticket_document(&ticket).contains("api"));
    assert!(world.registered_worktrees().is_empty());
}

/// A dirty worktree stops `destroy` with its own wording, and nothing is
/// deleted.
#[test]
fn test_destroy_dirty_worktree_wording() {
    let world = World::new();
    let ticket = world.setup("K");
    std::fs::write(ticket.join("api/wip.txt"), "work in progress").unwrap();

    let output = world.tix_with_input(&["destroy", ticket.to_str().unwrap()], "y\n");
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("could not prune worktree 'api'"),
        "{message}"
    );
    assert!(
        message.contains("commit or stash the changes (or pass --force)"),
        "{message}"
    );
    assert!(!message.contains("use force"), "{message}");
    assert!(ticket.join("api/wip.txt").is_file());
}
