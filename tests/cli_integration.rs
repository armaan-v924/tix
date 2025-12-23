use git2::{BranchType, Repository, Signature};
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tix::core::ticket::Ticket;
use toml::Value;

fn bin() -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("tix"));
    cmd.env("RUST_LOG", "info");
    cmd
}

fn get_completions_output(shell: &str) -> String {
    let mut cmd = bin();
    let output = cmd
        .args(["completions", shell])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).unwrap()
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
    if repo.find_branch("main", BranchType::Local).is_err() {
        repo.branch("main", &commit, false).unwrap();
    }
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

fn write_config_with_urls(
    root: &TempDir,
    code: &Path,
    tickets: &Path,
    repos: &[(&str, &Path, &Path)],
) -> PathBuf {
    let config_root = root.path().join("tix");
    fs::create_dir_all(&config_root).unwrap();

    let mut repos_toml = String::from("[repositories]\n");
    for (alias, url, path) in repos {
        repos_toml.push_str(&format!(
            r#"[repositories.{alias}]
url = "{url}"
path = "{path}"

"#,
            alias = alias,
            url = url.display(),
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

#[test]
fn setup_without_repos_stamps_metadata() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["setup", "JIRA-EMPTY"])
        .assert()
        .success();

    let ticket_dir = tickets.join("JIRA-EMPTY");
    let ticket = Ticket::load(&ticket_dir).unwrap();
    assert_eq!(ticket.metadata.id, "JIRA-EMPTY");
    assert!(ticket.metadata.repos.is_empty());
}

#[test]
fn setup_with_invalid_repo_stamps_metadata() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["setup", "JIRA-INVALID", "missing"])
        .assert()
        .success();

    let ticket_dir = tickets.join("JIRA-INVALID");
    let ticket = Ticket::load(&ticket_dir).unwrap();
    assert_eq!(ticket.metadata.id, "JIRA-INVALID");
    assert!(ticket.metadata.repos.is_empty());
}

#[test]
fn destroy_prunes_worktree_metadata() {
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
        .args(["setup", "JIRA-4", "api"])
        .assert()
        .success();

    let ticket_dir = tickets.join("JIRA-4");
    let ticket = Ticket::load(&ticket_dir).unwrap();
    let worktree_name = ticket.metadata.repo_worktrees.get("api").unwrap();
    let worktree_meta_dir = api_repo.join(".git").join("worktrees").join(worktree_name);
    assert!(worktree_meta_dir.exists());

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["destroy", "JIRA-4"])
        .current_dir(temp.path())
        .assert()
        .success();

    assert!(!ticket_dir.exists());
    assert!(!worktree_meta_dir.exists());
}

#[test]
fn destroy_prunes_when_worktree_dir_missing() {
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
        .args(["setup", "JIRA-5", "api"])
        .assert()
        .success();

    let ticket_dir = tickets.join("JIRA-5");
    let ticket = Ticket::load(&ticket_dir).unwrap();
    let worktree_name = ticket.metadata.repo_worktrees.get("api").unwrap();
    let worktree_meta_dir = api_repo.join(".git").join("worktrees").join(worktree_name);

    fs::remove_dir_all(ticket_dir.join("api")).unwrap();

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["destroy", "JIRA-5"])
        .current_dir(temp.path())
        .assert()
        .success();

    assert!(!ticket_dir.exists());
    assert!(!worktree_meta_dir.exists());
}

#[test]
fn setup_repos_clones_missing_repo() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&tickets).unwrap();

    let origin_repo = temp.path().join("origin-api");
    init_repo_with_origin(&origin_repo);

    write_config_with_urls(
        &temp,
        &code,
        &tickets,
        &[("api", &origin_repo, &code.join("api"))],
    );

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .arg("setup-repos")
        .assert()
        .success();

    assert!(code.join("api/.git").exists());
}

#[test]
fn setup_repos_skips_existing_repo() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let origin_repo = temp.path().join("origin-web");
    init_repo_with_origin(&origin_repo);

    let existing_repo = code.join("web");
    init_repo_with_origin(&existing_repo);

    write_config_with_urls(
        &temp,
        &code,
        &tickets,
        &[("web", &origin_repo, &existing_repo)],
    );

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .arg("setup-repos")
        .assert()
        .success();

    assert!(existing_repo.join("README.md").exists());
}

#[test]
fn config_updates_and_reads_value() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    let config_root = write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["config", "branch_prefix", "hotfix"])
        .assert()
        .success();

    let raw = fs::read_to_string(config_root.join("config.toml")).unwrap();
    let parsed: Value = toml::from_str(&raw).unwrap();
    assert_eq!(parsed["branch_prefix"].as_str().unwrap(), "hotfix");

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["config", "branch_prefix"])
        .assert()
        .success()
        .stderr(predicate::str::contains("branch_prefix = hotfix"));
}

#[test]
fn config_without_key_prints_full_config() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    let output = cmd
        .env("XDG_CONFIG_HOME", temp.path())
        .arg("config")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parsed: Value = toml::from_slice(&output).unwrap();
    assert_eq!(parsed["branch_prefix"].as_str().unwrap(), "feature");
    assert_eq!(
        parsed["tickets_directory"].as_str().unwrap(),
        tickets.display().to_string()
    );
}

#[test]
fn config_rejects_unknown_key() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["config", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown config key"));
}

#[test]
fn config_rejects_empty_path_value() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();
    write_config(&temp, &code, &tickets, &[]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["config", "code_directory", " "])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be empty"));
}

#[test]
fn doctor_warns_but_succeeds() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let missing_repo_path = temp.path().join("missing-repo");
    write_config(&temp, &code, &tickets, &[("api", &missing_repo_path)]);

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .arg("doctor")
        .assert()
        .success()
        .stderr(predicate::str::contains("will be cloned by setup-repos"));
}

#[test]
fn destroy_force_skips_dirty_check() {
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
        .args(["setup", "JIRA-6", "api"])
        .assert()
        .success();

    // dirty the worktree
    fs::write(tickets.join("JIRA-6/api/new.txt"), "dirty").unwrap();

    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["destroy", "JIRA-6", "--force"])
        .current_dir(temp.path())
        .assert()
        .success();
}

#[test]
fn completions_zsh_works_with_eval() {
    // Test that zsh completions output is eval-friendly
    let completions_script = get_completions_output("zsh");

    // The first line should be a comment (not an active #compdef directive)
    let first_line = completions_script.lines().next().unwrap();
    assert_eq!(
        first_line, "# compdef tix",
        "First line should be commented #compdef"
    );

    // Should contain the _tix function definition
    assert!(
        completions_script.contains("_tix() {"),
        "Should define _tix function"
    );

    // Should contain the compdef call for eval context
    assert!(
        completions_script.contains("compdef _tix tix"),
        "Should contain compdef call for eval"
    );

    // Should contain the conditional that handles both file and eval contexts
    assert!(
        completions_script.contains(r#"if [ "$funcstack[1]" = "_tix" ]; then"#),
        "Should contain funcstack conditional"
    );
}

#[test]
fn completions_bash_unchanged() {
    // Test that bash completions still work as before
    let completions_script = get_completions_output("bash");

    // Bash completions should define the _tix function
    assert!(
        completions_script.contains("_tix() {"),
        "Should define _tix function"
    );

    // Bash completions should have complete -F command
    assert!(
        completions_script.contains("complete -F _tix"),
        "Should contain complete command"
    );
}

#[test]
fn info_displays_ticket_information() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let api_repo = code.join("api");
    init_repo_with_origin(&api_repo);

    write_config(&temp, &code, &tickets, &[("api", &api_repo)]);

    // setup ticket with description
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args([
            "setup",
            "JIRA-123",
            "api",
            "--description",
            "Add new feature",
        ])
        .assert()
        .success();

    // test info from within ticket directory
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .arg("info")
        .current_dir(tickets.join("JIRA-123"))
        .assert()
        .success()
        .stdout(predicate::str::contains("[JIRA-123] Add new feature"));

    // test info with explicit ticket parameter
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["info", "--ticket", "JIRA-123"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[JIRA-123] Add new feature"));
}

#[test]
fn info_works_without_description() {
    let temp = TempDir::new().unwrap();
    let code = temp.path().join("code");
    let tickets = temp.path().join("tickets");
    fs::create_dir_all(&code).unwrap();
    fs::create_dir_all(&tickets).unwrap();

    let api_repo = code.join("api");
    init_repo_with_origin(&api_repo);

    write_config(&temp, &code, &tickets, &[("api", &api_repo)]);

    // setup ticket without description
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .args(["setup", "JIRA-456", "api"])
        .assert()
        .success();

    // test info displays ticket ID with empty description
    let mut cmd = bin();
    cmd.env("XDG_CONFIG_HOME", temp.path())
        .arg("info")
        .current_dir(tickets.join("JIRA-456"))
        .assert()
        .success()
        .stdout(predicate::str::contains("[JIRA-456]"));
}
