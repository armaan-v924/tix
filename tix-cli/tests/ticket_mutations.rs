mod common;

use common::{World, git, stderr, ticket_document};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Output};
use std::sync::{Barrier, mpsc};
use std::time::Duration;

fn report(output: &Output, code: i32) -> Value {
    assert_eq!(output.status.code(), Some(code), "{}", stderr(output));
    serde_json::from_slice(&output.stdout).expect("one JSON result")
}

fn lock_document(ticket: &Path) -> File {
    let file = File::options()
        .write(true)
        .open(ticket.join(".tix/ticket.toml.lock"))
        .unwrap();
    file.lock().unwrap();
    file
}

// Wait for the CLI to report Git's success just before it tries to record it.
fn wait_for_worktree(child: &mut Child, message: &str) {
    let stderr = child.stderr.take().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            if send.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    loop {
        let line = receive
            .recv_timeout(Duration::from_secs(15))
            .expect(message);
        if line.contains(message) {
            return;
        }
    }
}

fn add_web_repo(world: &World) {
    git(world.root(), &["clone", "-q", "origin.git", "code/web"]);
    let mut config = OpenOptions::new()
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
}

#[test]
fn concurrent_setup_claims_the_root_once() {
    let world = World::new();
    fs::remove_dir(world.root().join("tickets")).unwrap();
    let start = Barrier::new(2);
    let outputs = std::thread::scope(|scope| {
        let setup = || {
            start.wait();
            world.tix(&["setup", "K", "api", "-o", "json"])
        };
        let first = scope.spawn(setup);
        let second = setup();
        [first.join().unwrap(), second]
    });
    let (winner, loser) = if outputs[0].status.success() {
        (&outputs[0], &outputs[1])
    } else {
        (&outputs[1], &outputs[0])
    };
    assert_eq!(report(winner, 0)["outcome"], "succeeded");
    assert_eq!(report(loser, 1)["error"]["code"], "ticket-conflict");
    assert!(ticket_document(&world.ticket("K")).contains("[ticket.worktrees.api]"));
    assert_eq!(world.registered_worktrees().len(), 1);
}

#[test]
fn add_and_destroy_cannot_mutate_the_same_ticket_concurrently() {
    let world = World::new();
    let ticket = world.setup("K");
    let lock = lock_document(&ticket);
    let mut add = world.spawn_tix(&[
        "add", "--ticket", "K", "api", "--as", "extra", "--branch", "extra", "-o", "json",
    ]);
    wait_for_worktree(&mut add, "added worktree extra");
    assert!(!ticket_document(&ticket).contains("[ticket.worktrees.extra]"));

    let destroy = ["destroy", "K", "--yes", "-o", "json"];
    assert_eq!(
        report(&world.tix(&destroy), 1)["error"]["code"],
        "ticket-conflict"
    );
    assert!(ticket.join("api").is_dir());
    assert!(ticket.join("extra").is_dir());
    world.setup("INDEPENDENT");
    assert_eq!(world.registered_worktrees().len(), 3);

    drop(lock);
    assert_eq!(
        report(&add.wait_with_output().unwrap(), 0)["outcome"],
        "succeeded"
    );
    assert!(ticket_document(&ticket).contains("[ticket.worktrees.extra]"));
    assert_eq!(report(&world.tix(&destroy), 0)["outcome"], "succeeded");
    assert!(!ticket.exists());
    let registered = world.registered_worktrees();
    assert_eq!(registered.len(), 1);
    assert!(
        registered[0].ends_with("INDEPENDENT/api") || registered[0].ends_with("INDEPENDENT\\api")
    );
}

#[test]
fn resume_adopts_an_interrupted_checkout_and_creates_the_missing_one() {
    let world = World::new();
    add_web_repo(&world);
    let ticket = world.ticket("K");
    // Initially both worktrees fail, leaving a plan with no recorded checkouts.
    let first = world.tix(&[
        "setup",
        "K",
        "api",
        "web",
        "--base",
        "refs/heads/start",
        "-o",
        "json",
    ]);
    assert_eq!(report(&first, 1)["outcome"], "partial");
    let before = ticket_document(&ticket);
    git(&world.source(), &["branch", "start"]);
    git(&world.root().join("code/web"), &["branch", "start"]);

    let lock = lock_document(&ticket);
    let resume = ["setup", "K", "--resume", "-o", "json"];
    let mut interrupted = world.spawn_tix(&resume);
    wait_for_worktree(&mut interrupted, "created worktree api");
    interrupted.kill().unwrap();
    assert!(!interrupted.wait().unwrap().success());
    drop(lock);

    assert_eq!(ticket_document(&ticket), before);
    assert!(ticket.join("api/.git").is_file());
    assert!(!ticket.join("web").exists());
    assert_eq!(world.registered_worktrees().len(), 1);
    let info = world.tix(&["info", "--ticket", "K", "-o", "json"]);
    assert!(report(&info, 0)["worktrees"].as_array().unwrap().is_empty());

    // A changed checkout is a conflict. A fresh caller can still create web,
    // and the conflicted directory and the changes inside it are preserved.
    let checkout = ticket.join("api");
    fs::write(checkout.join("wip"), "keep my work").unwrap();
    git(&checkout, &["switch", "-q", "-c", "other"]);
    let partial = report(&world.tix(&resume), 1);
    assert_eq!(partial["worktrees"][0]["error"]["code"], "setup-conflict");
    assert_eq!(partial["worktrees"][1]["outcome"], "created");
    assert!(ticket_document(&ticket).contains("[ticket.worktrees.web]"));
    assert_eq!(
        fs::read_to_string(checkout.join("wip")).unwrap(),
        "keep my work"
    );

    git(&checkout, &["switch", "-q", "K"]);
    let resumed = report(&world.tix(&resume), 0);
    assert_eq!(resumed["worktrees"].as_array().unwrap().len(), 1);
    assert_eq!(resumed["worktrees"][0]["outcome"], "adopted");
    assert!(resumed["worktrees"][0].get("base_commit").is_none());
    let completed = ticket_document(&ticket);
    assert!(completed.contains("[ticket.worktrees.api]"));
    assert!(completed.contains("[ticket.worktrees.web]"));
    assert!(!completed.contains("[ticket_setup]"));
    assert_eq!(world.registered_worktrees().len(), 1);
    let web = git(
        &world.root().join("code/web"),
        &["worktree", "list", "--porcelain"],
    );
    assert_eq!(
        String::from_utf8_lossy(&web.stdout)
            .matches("worktree ")
            .count(),
        2
    );
    assert_eq!(
        report(&world.tix(&resume), 1)["error"]["code"],
        "setup-conflict"
    );
}

#[test]
fn resume_refuses_a_matching_branch_in_an_unrelated_checkout() {
    let world = World::new();
    add_web_repo(&world);
    let first = world.tix(&[
        "setup",
        "K",
        "api",
        "--base",
        "refs/heads/start",
        "-o",
        "json",
    ]);
    assert_eq!(report(&first, 1)["outcome"], "partial");
    let ticket = world.ticket("K");
    let checkout = ticket.join("api");
    git(
        &world.root().join("code/web"),
        &["worktree", "add", "-qb", "K", checkout.to_str().unwrap()],
    );
    let before = ticket_document(&ticket);
    let gitlink = fs::read(checkout.join(".git")).unwrap();

    let output = world.tix(&["setup", "K", "--resume", "-o", "json"]);
    assert_eq!(
        report(&output, 1)["worktrees"][0]["error"]["code"],
        "setup-conflict"
    );
    assert_eq!(ticket_document(&ticket), before);
    assert_eq!(fs::read(checkout.join(".git")).unwrap(), gitlink);
    assert!(world.registered_worktrees().is_empty());
}
