//! End-to-end `tix add` and `tix setup` when the base cannot be used: each
//! refusal comes from the engine as a bare fact, and the CLI words the way
//! past it with the flags the command takes.

mod common;

use common::{World, git, stderr};

/// Moves origin's `main` on by one commit, so a branch cut before it no
/// longer contains `origin/main`.
fn advance_origin_main(world: &World) {
    let seed = world.root().join("seed");
    git(
        &seed,
        &["commit", "-q", "--allow-empty", "-m", "main moved on"],
    );
    git(&seed, &["push", "-q", "../origin.git", "main"]);
}

/// `tix add --base` onto an existing branch that lacks it offers both of
/// add's ways out: dropping `--base`, or `--branch`.
#[test]
fn test_add_branch_lacks_base() {
    let world = World::new();
    let output = world.tix(&["setup", "K"]);
    assert!(output.status.success(), "{}", stderr(&output));
    git(&world.source(), &["branch", "old"]);
    advance_origin_main(&world);

    let output = world.tix(&["add", "-t", "K", "--branch", "old", "--base", "main", "api"]);
    assert!(!output.status.success());
    let stderr = stderr(&output);
    assert!(
        stderr.contains(
            "branch 'old' already exists and does not contain 'origin/main' — \
             drop --base to reuse it as it is, or pick another branch with --branch"
        ),
        "{stderr}"
    );
    assert!(!world.ticket("K").join("api").exists());
}

/// `tix setup` derives its branch, so it offers no `--branch` it lacks.
#[test]
fn test_setup_branch_lacks_base() {
    let world = World::new();
    git(&world.source(), &["branch", "K"]);
    advance_origin_main(&world);

    let output = world.tix(&["setup", "K", "--base", "main", "api"]);
    assert!(!output.status.success());
    let stderr = stderr(&output);
    assert!(
        stderr.contains(
            "branch 'K' already exists and does not contain 'origin/main' — \
             drop --base to reuse it as it is"
        ),
        "{stderr}"
    );
    assert!(!stderr.contains("--branch"), "{stderr}");
}

/// A bare `--base` whose local and origin branches diverged names both
/// qualified spellings as `--base` values.
#[test]
fn test_setup_ambiguous_base() {
    let world = World::new();
    let seed = world.root().join("seed");
    git(&seed, &["switch", "-q", "-c", "dev"]);
    git(
        &seed,
        &["commit", "-q", "--allow-empty", "-m", "origin's dev"],
    );
    git(&seed, &["push", "-q", "../origin.git", "dev"]);
    let source = world.source();
    git(&source, &["switch", "-q", "-c", "dev"]);
    git(
        &source,
        &["commit", "-q", "--allow-empty", "-m", "local dev"],
    );
    git(&source, &["switch", "-q", "main"]);

    let output = world.tix(&["setup", "K", "--base", "dev", "api"]);
    assert!(!output.status.success());
    let stderr = stderr(&output);
    assert!(
        stderr.contains(
            "'dev' has diverged from 'origin/dev', so either could be the base — \
             pass --base refs/heads/dev or --base origin/dev to pick one"
        ),
        "{stderr}"
    );
}

/// No cached default branch and no origin to ask: `--base` is the way out.
#[test]
fn test_setup_unknown_default_branch() {
    let world = World::new();
    let source = world.source();
    git(&source, &["remote", "set-head", "origin", "-d"]);
    let nowhere = world.root().join("nowhere.git");
    git(
        &source,
        &["remote", "set-url", "origin", nowhere.to_str().unwrap()],
    );

    let output = world.tix(&["setup", "K", "api"]);
    assert!(!output.status.success());
    let stderr = stderr(&output);
    assert!(
        stderr.contains("cannot determine the default branch of 'api' ("),
        "{stderr}"
    );
    assert!(stderr.contains(") — pass --base"), "{stderr}");
}

/// A default branch that is known but not fetched names the fetch to run.
#[test]
fn test_add_default_branch_not_fetched() {
    let world = World::new();
    let output = world.tix(&["setup", "K"]);
    assert!(output.status.success(), "{}", stderr(&output));
    git(
        &world.source(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );

    let output = world.tix(&["add", "-t", "K", "api"]);
    assert!(!output.status.success());
    let stderr = stderr(&output);
    assert!(
        stderr.contains(
            "origin's default branch is 'trunk', but refs/remotes/origin/trunk does not exist — \
             run `git fetch origin trunk` in the source checkout, or pass --base"
        ),
        "{stderr}"
    );
}
