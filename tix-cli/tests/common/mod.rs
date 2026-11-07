//! The world the CLI integration tests run `tix` in: a bare "origin", a
//! clone registered as `api`, and a global config pointing at both, all in a
//! temp directory.
//!
//! Each test binary uses only some of these helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub struct World {
    pub dir: TempDir,
}

impl World {
    /// A registered, cloned `api` repository and an empty tickets directory.
    pub fn new() -> Self {
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

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn ticket(&self, key: &str) -> PathBuf {
        self.dir.path().join("tickets").join(key)
    }

    pub fn source(&self) -> PathBuf {
        self.dir.path().join("code/api")
    }

    /// Runs `tix <args>` from the world's root, with `stdin` fed to it.
    pub fn tix_with_input(&self, args: &[&str], stdin: &str) -> Output {
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

    pub fn tix(&self, args: &[&str]) -> Output {
        self.tix_with_input(args, "")
    }

    /// `tix setup <key> api`, asserting it worked.
    pub fn setup(&self, key: &str) -> PathBuf {
        let output = self.tix(&["setup", key, "api"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let ticket = self.ticket(key);
        assert!(ticket.join("api").is_dir());
        ticket
    }

    /// The worktree paths git has registered for the source repository,
    /// besides the source checkout itself.
    pub fn registered_worktrees(&self) -> Vec<String> {
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

pub fn git(cwd: &Path, args: &[&str]) -> Output {
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

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn ticket_document(ticket: &Path) -> String {
    std::fs::read_to_string(ticket.join(".tix/ticket.toml")).unwrap()
}
