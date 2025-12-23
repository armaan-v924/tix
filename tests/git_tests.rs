use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use git2::{BranchType, Commit, Repository, Signature};
use tix::git::{clone_repo, create_worktree, fetch_and_fast_forward, is_clean, remove_worktree};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn unique_path(prefix: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp-tests");
    fs::create_dir_all(&base).unwrap();
    base.join(format!("tix-git-test-{}-{}", prefix, id))
}

fn empty_dir(prefix: &str) -> PathBuf {
    let path = unique_path(prefix);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn init_repo_with_commit(path: &Path) -> Result<Repository, git2::Error> {
    let repo = Repository::init(path)?;
    add_commit(&repo, "README.md", "init")?;
    Ok(repo)
}

fn add_commit(repo: &Repository, filename: &str, contents: &str) -> Result<git2::Oid, git2::Error> {
    let workdir = repo.workdir().unwrap().to_path_buf();
    let file_path = workdir.join(filename);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&file_path, contents).unwrap();

    let mut index = repo.index()?;
    index.add_path(Path::new(filename))?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = Signature::now("Test", "test@example.com")?;

    let mut parents = Vec::new();
    if let Ok(head) = repo.head() {
        if let Some(oid) = head.target() {
            parents.push(repo.find_commit(oid)?);
        }
    }
    let parent_refs: Vec<&Commit> = parents.iter().collect();

    repo.commit(Some("HEAD"), &sig, &sig, "commit", &tree, &parent_refs)
        .map_err(Into::into)
}

fn head_oid(repo_path: &Path) -> git2::Oid {
    let repo = Repository::open(repo_path).unwrap();
    let head = repo.head().unwrap();
    head.target().unwrap()
}

fn skip_if_xdev<T, E: std::fmt::Display + std::fmt::Debug, F: FnOnce() -> Result<T, E>>(
    op: F,
) -> Option<T> {
    match op() {
        Ok(v) => Some(v),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Invalid cross-device link") {
                eprintln!("Skipping test due to filesystem limitation: {}", msg);
                None
            } else {
                panic!("{:?}", e)
            }
        }
    }
}

#[test]
fn is_clean_detects_dirty_and_clean_states() {
    let repo_path = empty_dir("clean");
    let Some(repo) = skip_if_xdev(|| init_repo_with_commit(&repo_path)) else {
        return;
    };

    assert!(is_clean(&repo_path).unwrap());

    // Make working tree dirty
    fs::write(repo_path.join("README.md"), "modified").unwrap();
    assert!(!is_clean(&repo_path).unwrap());

    let Some(_) = skip_if_xdev(|| add_commit(&repo, "README.md", "after")) else {
        return;
    };
    assert!(is_clean(&repo_path).unwrap());
}

#[test]
fn create_and_remove_worktree() {
    let repo_path = empty_dir("worktree-src");
    let Some(_) = skip_if_xdev(|| init_repo_with_commit(&repo_path)) else {
        return;
    };

    let worktree_root = empty_dir("worktree-root");
    let worktree_path = worktree_root.join("dst");
    let branch_name = "feature/test";
    let worktree_name = branch_name.replace('/', "_");

    let Some(_) = skip_if_xdev(|| create_worktree(&repo_path, &worktree_path, branch_name, None))
    else {
        return;
    };
    assert!(worktree_path.exists());

    fs::remove_dir_all(&worktree_path).unwrap();
    remove_worktree(&repo_path, &worktree_name).unwrap();
}

#[test]
fn clone_repo_from_local_path() {
    let origin_path = empty_dir("origin");
    let Some(_) = skip_if_xdev(|| init_repo_with_commit(&origin_path)) else {
        return;
    };

    let clone_path = empty_dir("clone");
    let Some(_) = skip_if_xdev(|| clone_repo(origin_path.to_str().unwrap(), &clone_path)) else {
        return;
    };
    assert!(clone_path.join(".git").exists());
}

#[test]
fn fetch_and_fast_forward_updates_clone() {
    let origin_path = empty_dir("origin-ff");
    let Some(origin_repo) = skip_if_xdev(|| init_repo_with_commit(&origin_path)) else {
        return;
    };

    let clone_path = empty_dir("clone-ff");
    let Some(_) = skip_if_xdev(|| clone_repo(origin_path.to_str().unwrap(), &clone_path)) else {
        return;
    };

    let before = head_oid(&clone_path);

    let Some(_) = skip_if_xdev(|| add_commit(&origin_repo, "README.md", "upstream")) else {
        return;
    };

    let Some(_) = skip_if_xdev(|| fetch_and_fast_forward(&clone_path, "origin")) else {
        return;
    };

    let after = head_oid(&clone_path);
    assert_ne!(before, after, "clone should fast-forward to new commit");
}

#[test]
fn create_worktree_sets_upstream_when_remote_branch_exists() {
    let repo_path = empty_dir("worktree-upstream");
    let Some(repo) = skip_if_xdev(|| init_repo_with_commit(&repo_path)) else {
        return;
    };

    repo.remote("origin", repo_path.to_str().unwrap()).unwrap();
    let head = repo.head().unwrap();
    let head_oid = head.target().unwrap();
    repo.reference(
        "refs/remotes/origin/feature/upstream",
        head_oid,
        true,
        "test remote ref",
    )
    .unwrap();

    let worktree_root = empty_dir("worktree-upstream-root");
    let worktree_path = worktree_root.join("dst");
    let branch_name = "feature/upstream";

    let Some(_) = skip_if_xdev(|| create_worktree(&repo_path, &worktree_path, branch_name, None))
    else {
        return;
    };

    let repo_after = Repository::open(&repo_path).unwrap();
    let local = repo_after
        .find_branch(branch_name, BranchType::Local)
        .unwrap();
    let upstream = local.upstream().unwrap();
    assert_eq!(upstream.name().unwrap().unwrap(), "origin/feature/upstream");
}
