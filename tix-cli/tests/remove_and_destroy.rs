//! End-to-end `tix remove` and `tix destroy` against real repositories:
//! worktrees deleted by hand, and the wording of their refusals.
//!
//! Each test builds its own [`World`] and drives the built `tix` binary
//! through it.

mod common;

use common::{World, git, stderr, ticket_document};

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
