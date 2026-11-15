mod common;

use common::{World, git, stderr};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tix_sdk::document::with_write;

fn json(output: &Output) -> Value {
    assert!(output.status.success(), "{}", stderr(output));
    serde_json::from_slice(&output.stdout).expect("one JSON result")
}

fn snapshot(path: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            entries.extend(snapshot(&path));
            entries.insert(path, None);
        } else {
            entries.insert(path.clone(), Some(fs::read(path).unwrap()));
        }
    }
    entries
}

#[test]
fn inspection_and_teardown_previews_leave_the_filesystem_unchanged() {
    let world = World::new();
    let ticket = world.setup("K");
    let ticket_lock = ticket.join(".tix/ticket.toml.lock");
    let config_lock = world.root().join("config.toml.lock");
    fs::remove_file(&ticket_lock).unwrap();
    assert!(!config_lock.exists());

    for obstruct_sidecars in [false, true] {
        if obstruct_sidecars {
            // A directory at each lock path is unwritable as a sidecar,
            // even when the test runner can bypass permission bits.
            fs::create_dir(&ticket_lock).unwrap();
            fs::create_dir(&config_lock).unwrap();
        }
        let before = snapshot(world.root());
        assert_eq!(
            json(&world.tix(&["info", "--ticket", "K", "-o", "json"]))["key"],
            "K"
        );
        assert_eq!(json(&world.tix(&["list", "-o", "json"]))[0]["key"], "K");
        assert_eq!(
            json(&world.tix(&["destroy", "K", "--dry-run", "-o", "json"]))["would_succeed"],
            true
        );
        assert_eq!(
            json(&world.tix(&["remove", "api", "--ticket", "K", "--dry-run", "-o", "json"]))["would_succeed"],
            true
        );
        let config = world.tix(&["config", "show"]);
        assert!(config.status.success(), "{}", stderr(&config));
        assert_eq!(before, snapshot(world.root()));
    }
}

#[test]
fn inspection_parses_complete_snapshots_during_atomic_saves() {
    let world = World::new();
    let ticket = world.setup("K");
    let path = ticket.join(".tix/ticket.toml");
    let stop = AtomicBool::new(false);
    let start = Barrier::new(2);
    let a = "a".repeat(8192);
    let b = "b".repeat(16384);

    std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            start.wait();
            let mut saves = 0;
            while !stop.load(Ordering::Relaxed) {
                let description = [&a, &b][saves % 2];
                with_write(&path, |doc| {
                    doc.doc_mut()["ticket"]["description"] = toml_edit::value(description.as_str());
                    Ok(())
                })
                .unwrap();
                saves += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            saves
        });
        start.wait();
        // Collect outputs first so a parsing failure does not strand the writer.
        let results: Vec<_> = (0..8)
            .map(|_| {
                [
                    world.tix(&["info", "--ticket", "K", "-o", "json"]),
                    world.tix(&["list", "-o", "json"]),
                    world.tix(&["destroy", "K", "--dry-run", "-o", "json"]),
                ]
            })
            .collect();
        stop.store(true, Ordering::Relaxed);
        assert!(writer.join().unwrap() >= 2);
        for [info, list, preview] in results {
            for description in [&json(&info)["description"], &json(&list)[0]["description"]] {
                assert!(["", &a, &b].contains(&description.as_str().unwrap()));
            }
            assert_eq!(json(&preview)["would_succeed"], true);
        }
    });
}

#[test]
fn live_status_reports_changes_branch_detachment_and_local_upstream() {
    let world = World::new();
    let ticket = world.setup("K");
    let checkout = ticket.join("api");
    let add = world.tix(&[
        "add", "--ticket", "K", "api", "--as", "broken", "--branch", "broken",
    ]);
    assert!(add.status.success(), "{}", stderr(&add));
    fs::remove_dir_all(ticket.join("broken")).unwrap();
    let info = ["info", "--ticket", "K", "--status", "-o", "json"];
    let recorded = json(&world.tix(&["info", "--ticket", "K", "-o", "json"]));
    assert!(recorded["worktrees"][0].get("git").is_none());
    assert!(json(&world.tix(&info))["worktrees"][0]["git"]["upstream"].is_null());

    git(&checkout, &["switch", "-q", "-c", "handoff"]);
    fs::write(checkout.join("tracked"), "initial").unwrap();
    git(&checkout, &["add", "tracked"]);
    git(&checkout, &["commit", "-qm", "local change"]);
    fs::write(checkout.join("tracked"), "staged").unwrap();
    git(&checkout, &["add", "tracked"]);
    fs::write(checkout.join("tracked"), "unstaged").unwrap();
    fs::write(checkout.join("untracked"), "untracked").unwrap();
    fs::write(world.source().join(".git/info/exclude"), "/ignored\n").unwrap();
    fs::write(checkout.join("ignored"), "ignored").unwrap();

    let seed = world.root().join("seed");
    git(
        &seed,
        &["commit", "-q", "--allow-empty", "-m", "upstream change"],
    );
    git(&seed, &["push", "-q", "../origin.git", "main"]);
    git(&world.source(), &["fetch", "-q", "origin"]);
    git(&checkout, &["branch", "--set-upstream-to=origin/main"]);
    git(
        &world.source(),
        &["remote", "set-url", "origin", "unavailable.git"],
    );

    let head = git(&checkout, &["rev-parse", "HEAD"]);
    let live = json(&world.tix(&info));
    let worktree = &live["worktrees"][0];
    assert_eq!(worktree["branch"], "K");
    assert_eq!(worktree["git"]["state"], "available");
    assert_eq!(worktree["git"]["branch"], "handoff");
    assert_eq!(worktree["git"]["detached"], false);
    assert_eq!(
        worktree["git"]["head"],
        String::from_utf8_lossy(&head.stdout).trim()
    );
    assert_eq!(worktree["git"]["staged"], 1);
    assert_eq!(worktree["git"]["unstaged"], 1);
    assert_eq!(worktree["git"]["untracked"], 1);
    assert_eq!(
        worktree["git"]["upstream"]["reference"],
        "refs/remotes/origin/main"
    );
    assert_eq!(worktree["git"]["upstream"]["ahead"], 1);
    assert_eq!(worktree["git"]["upstream"]["behind"], 1);
    assert_eq!(live["worktrees"][1]["git"]["state"], "unavailable");

    git(
        &world.source(),
        &["update-ref", "-d", "refs/remotes/origin/main"],
    );
    let live = json(&world.tix(&info));
    assert!(live["worktrees"][0]["git"]["upstream"].is_null());
    git(&checkout, &["switch", "-q", "--detach", "HEAD"]);
    let detached = json(&world.tix(&info));
    assert_eq!(detached["worktrees"][0]["branch"], "K");
    assert!(detached["worktrees"][0]["git"]["branch"].is_null());
    assert_eq!(detached["worktrees"][0]["git"]["detached"], true);

    let toml = world.tix(&["info", "--ticket", "K", "--status", "-o", "toml"]);
    assert!(toml.status.success(), "{}", stderr(&toml));
    let value: toml::Value = toml::from_str(std::str::from_utf8(&toml.stdout).unwrap()).unwrap();
    assert_eq!(value["git"]["api"]["detached"].as_bool(), Some(true));
}
