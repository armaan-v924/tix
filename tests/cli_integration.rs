use git2::{Repository, Signature};
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tix::core::ticket::Ticket;

fn bin() -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("tix"));
    cmd.env("RUST_LOG", "info");
    cmd
}

fn init_repo_with_origin(path: &Path) {
    let repo = Repository::init(path).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    fs::write(path.join("README.md"), "init").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("README.md")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let commit_id = repo
        .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();
    let commit = repo.find_commit(commit_id).unwrap();
    repo.branch("main", &commit, true).unwrap();
    repo.set_head("refs/heads/main").unwrap();
    let origin = path.to_str().unwrap();
    if repo.find_remote("origin").is_err() {
        repo.remote("origin", origin).unwrap();
    }
}

fn write_config(root: &TempDir, code: &Path, tickets: &Path, repos: &[(&str, &Path)]) -> PathBuf {
    let config_root = root.path().join("tix");
    fs::create_dir_all(&config_root).unwrap();

    let mut repos_toml = String::from("[repositories]\n");
    for (alias, path) in repos {
        repos_toml.push_str(&format!(
            r#"[repositories.{alias}]
url = "{url}"
path = "{path}"

"#,
            alias = alias,
            url = path.display(),
            path = path.display()
        ));
    }

    let config = format!(
        r#"
branch_prefix = "feature"
github_base_url = "https://github.com"
default_repository_owner = "my-org"
code_directory = "{code}"
tickets_directory = "{tickets}"

{repos}
"#,
        code = code.display(),
        tickets = tickets.display(),
        repos = repos_toml
    );
    fs::write(config_root.join("config.toml"), config).unwrap();
    config_root
}

#[test]
fn doctor_fails_on_missing_config() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    let config_root = write_config(&temp, &code, &tickets, &[]);
    fs::remove_file(config_root.join("config.toml")).unwrap();

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .arg("doctor")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Doctor found"));
}

#[test]
fn add_repo_updates_config() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["add-repo", "myrepo"])
        .assert()
        .success();
}

#[test]
fn setup_creates_metadata_and_worktrees() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let api_repo = code.join("api");
    init_repo_with_origin(&api_repo);

    write_config(&temp, &code, &tickets, &[("api", &api_repo)]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["setup", "JIRA-1", "api", "--description", "My Feature"])
        .assert()
        .success();

    let ticket_dir = tickets.join("JIRA-1");
    assert!(ticket_dir.join("api").exists());
    let ticket = Ticket::load(&ticket_dir).unwrap();
    assert!(ticket.metadata.repo_branches.contains_key("api"));
    assert!(ticket.metadata.repo_worktrees.contains_key("api"));
}

#[test]
fn add_adds_repo_and_metadata() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let api_repo = code.join("api");
    let web_repo = code.join("web");
    init_repo_with_origin(&api_repo);
    init_repo_with_origin(&web_repo);

    write_config(
        &temp,
        &code,
        &tickets,
        &[("api", &api_repo), ("web", &web_repo)],
    );

    // setup ticket with api
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["setup", "JIRA-2", "api"])
        .assert()
        .success();

    // add web repo
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["add", "web"])
        .current_dir(tickets.join("JIRA-2"))
        .assert()
        .success();

    let ticket = Ticket::load(&tickets.join("JIRA-2")).unwrap();
    assert!(ticket.metadata.repo_branches.contains_key("web"));
    assert!(tickets.join("JIRA-2/web").exists());
}

#[test]
fn remove_respects_clean_check() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let api_repo = code.join("api");
    init_repo_with_origin(&api_repo);

    write_config(&temp, &code, &tickets, &[("api", &api_repo)]);

    // setup ticket with api
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["setup", "JIRA-3", "api"])
        .assert()
        .success();

    // dirty the worktree
    fs::write(tickets.join("JIRA-3/api/new.txt"), "dirty").unwrap();

    // remove should fail due to dirty
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["remove", "api"])
        .current_dir(tickets.join("JIRA-3"))
        .assert()
        .failure();

    // clean and retry
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["remove", "api"])
        .current_dir(tickets.join("JIRA-3"))
        .assert()
        .failure(); // still dirty because file exists

    fs::remove_file(tickets.join("JIRA-3/api/new.txt")).unwrap();

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["remove", "api"])
        .current_dir(tickets.join("JIRA-3"))
        .assert()
        .success();
}
