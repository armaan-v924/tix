//! Git helpers built on `git2` for worktree management and safety checks.

use anyhow::{Context, Result};
use git2::build::CheckoutBuilder;
use git2::{
    BranchType, Commit, Cred, RemoteCallbacks, Repository, StatusOptions, WorktreeAddOptions,
};
use log::{debug, warn};
use std::path::Path;
use std::process::{Command, Stdio};
use std::io::Write;

/// Return `true` if the repository at `repo_path` has no modified/staged/untracked files.
pub fn is_clean(repo_path: &Path) -> Result<bool> {
    // Open the repo
    let repo =
        Repository::open(repo_path).context("Failed to open repository to check the status")?;

    // Configure status options (include untracked, exclude ignored)
    let mut options = StatusOptions::new();
    options.include_untracked(true);

    let statuses = repo
        .statuses(Some(&mut options))
        .context("Failed to read repository status.")?;

    // If there are 0 entries in the status list, it's clean.
    Ok(statuses.is_empty())
}

/// Create a git worktree at `target_path`, using `branch_name`, optionally created from `base_ref`.
pub fn create_worktree(
    repo_path: &Path,
    target_path: &Path,
    branch_name: &str,
    base_ref: Option<&str>,
) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open source repository")?;

    // 1. Resolve the branch
    let branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => {
            // Case A: Branch already exists, we'll use it.
            debug!("Found existing branch '{}'", branch_name);
            b
        }
        Err(_) => {
            // Case B: Branch does not exist. Create it from HEAD.
            debug!("Branch '{}' not found. Creating from base...", branch_name);

            let default = resolve_default_branch(&repo);
            let base = base_ref.or(default.as_deref());
            if base_ref.is_none() && default.is_none() {
                warn!(
                    "No base_ref provided and no default branch detected; falling back to HEAD for {}",
                    repo_path.display()
                );
            }
            let commit = get_base_commit(&repo, base)?;

            repo.branch(branch_name, &commit, false)
                .context(format!("Failed to create branch '{}'", branch_name))?
        }
    };

    // 2. Add the worktree
    let mut worktree_options = WorktreeAddOptions::new();

    // We convert the Branch object into a Reference to pass to the options
    let branch_ref = branch.into_reference();
    worktree_options.reference(Some(&branch_ref));

    let worktree_name = branch_name.replace('/', "_");

    repo.worktree(
        &worktree_name, // metadata name for the worktree
        target_path,    // disk path
        Some(&mut worktree_options),
    )
    .context("Failed to create a worktree")?;

    Ok(())
}

fn get_base_commit<'a>(repo: &'a Repository, base: Option<&str>) -> Result<Commit<'a>> {
    let obj = match base {
        Some(rev) => {
            // revparse_single handles `main`, `origin/master`, `HEAD^`, etc.
            repo.revparse_single(rev)
                .context(format!("Could not find base reference '{}'", rev))?
        }
        None => {
            // Default to HEAD if no base provided
            repo.head()
                .context("Repo has no HEAD")?
                .peel_to_commit()
                .context("HEAD is not a commit")?
                .into_object()
        }
    };

    obj.peel_to_commit()
        .context("Base reference did not point to a commit")
}

/// Remove worktree metadata by name from the repository at `repo_path`.
pub fn remove_worktree(repo_path: &Path, worktree_name: &str) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open repository")?;

    // Attempt to find the worktree by name
    let worktree = match repo.find_worktree(worktree_name) {
        Ok(wt) => wt,
        Err(_) => {
            return Ok(());
        }
    };

    worktree
        .prune(None)
        .context("Failed to prune worktree metadata")?;

    Ok(())
}

/// Attempt to retrieve credentials using `git credential fill` command.
/// This uses the same credential system as command-line git, which can access
/// OS keychains and other credential stores that libgit2 might not be able to access directly.
fn get_credentials_via_git_command(url: &str) -> Option<(String, String)> {
    debug!("Attempting to get credentials via 'git credential fill' for {}", url);
    
    // Parse URL to extract protocol and host
    let (protocol, host) = if let Some(https_start) = url.strip_prefix("https://") {
        ("https", https_start.split('/').next()?)
    } else if let Some(http_start) = url.strip_prefix("http://") {
        ("http", http_start.split('/').next()?)
    } else {
        return None;
    };
    
    // Prepare input for git credential fill
    let input = format!("protocol={}\nhost={}\n\n", protocol, host);
    
    // Spawn git credential fill command
    let mut child = Command::new("git")
        .arg("credential")
        .arg("fill")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    
    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(input.as_bytes()).is_err() {
            debug!("Failed to write to git credential fill stdin");
            return None;
        }
    }
    
    // Read output
    let output = child.wait_with_output().ok()?;
    
    if !output.status.success() {
        debug!("git credential fill failed with status: {}", output.status);
        return None;
    }
    
    // Parse output to extract username and password
    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut username = None;
    let mut password = None;
    
    for line in output_str.lines() {
        if let Some(user) = line.strip_prefix("username=") {
            username = Some(user.to_string());
        } else if let Some(pass) = line.strip_prefix("password=") {
            password = Some(pass.to_string());
        }
    }
    
    match (username, password) {
        (Some(u), Some(p)) => {
            debug!("Successfully retrieved credentials via git credential fill");
            Some((u, p))
        }
        _ => {
            debug!("git credential fill did not return username and password");
            None
        }
    }
}

/// Create callbacks for git operations that use system credentials.
///
/// This function creates a `RemoteCallbacks` instance configured to authenticate
/// with private repositories using the system's git credentials. It attempts multiple
/// authentication methods based on what git requests:
/// 1. SSH key from ssh-agent (for SSH URLs)
/// 2. Username/password from git credential helpers via git2
/// 3. Username/password from git credential helpers via git command (fallback)
///
/// For HTTPS authentication, this relies on git's credential helper system.
/// The git command fallback allows access to OS keychains and other credential stores.
fn create_git_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();
    let mut tried_sshkey = false;
    let mut tried_cred_helper = false;
    let mut tried_git_command = false;
    
    callbacks.credentials(move |url, username_from_url, allowed_types| {
        debug!(
            "Git credential callback: url={}, username={:?}, allowed_types={:?}",
            url, username_from_url, allowed_types
        );

        // Try SSH key from agent
        if allowed_types.is_ssh_key() && !tried_sshkey {
            tried_sshkey = true;
            debug!("Attempting SSH key authentication");
            match Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
                Ok(cred) => {
                    debug!("Successfully using SSH key from agent");
                    return Ok(cred);
                }
                Err(e) => {
                    debug!("SSH key authentication failed: {}", e);
                    // Fall through to try other methods
                }
            }
        }

        // Try username/password from credential helper via git2
        if (allowed_types.is_user_pass_plaintext() || allowed_types.is_username()) && !tried_cred_helper {
            tried_cred_helper = true;
            debug!("Attempting to retrieve credentials from git2 credential helper");
            if let Ok(config) = git2::Config::open_default() {
                if let Ok(cred) = Cred::credential_helper(&config, url, username_from_url) {
                    debug!("Successfully retrieved credentials from git2 credential helper");
                    return Ok(cred);
                } else {
                    debug!("git2 credential helper did not provide credentials");
                }
            } else {
                debug!("Could not open git config");
            }
            
            // Try using git credential fill command as fallback
            if !tried_git_command {
                tried_git_command = true;
                if let Some((username, password)) = get_credentials_via_git_command(url) {
                    match Cred::userpass_plaintext(&username, &password) {
                        Ok(cred) => {
                            debug!("Successfully created credentials from git command");
                            return Ok(cred);
                        }
                        Err(e) => {
                            debug!("Failed to create userpass credential: {}", e);
                        }
                    }
                }
            }
        }

        // If all attempts failed, return a helpful error
        Err(git2::Error::from_str(
            &format!(
                "Failed to authenticate to {}.\n\
                 \n\
                 The repository requires authentication, but no valid credentials were found.\n\
                 \n\
                 Please try one of the following:\n\
                 1. Configure git credential helper to cache your credentials:\n\
                    git config --global credential.helper cache\n\
                    Then run 'git fetch' manually in the repository to cache credentials\n\
                 \n\
                 2. Use SSH instead of HTTPS by updating the repository URL:\n\
                    git remote set-url origin git@github.com:USER/REPO.git\n\
                 \n\
                 3. For GitHub, create a personal access token and use it as your password\n\
                 \n\
                 The command-line 'git fetch' may work because it can prompt for credentials,\n\
                 but programmatic access requires pre-configured authentication.",
                url
            )
        ))
    });
    callbacks
}

/// Clone a repository to `target`.
///
/// Supports cloning both public and private repositories by using system git credentials.
/// Authentication is handled automatically through SSH keys, credential helpers, or default credentials.
pub fn clone_repo(url: &str, target: &Path) -> Result<()> {
    let mut builder = git2::build::RepoBuilder::new();
    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(create_git_callbacks());
    builder.fetch_options(fetch_options);

    builder
        .clone(url, target)
        .context("Failed to clone repository")?;
    Ok(())
}

/// Fetch from `remote_name` and fast-forward the current branch to its upstream if possible.
pub fn fetch_and_fast_forward(repo_path: &Path, remote_name: &str) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open repository for fetch")?;

    let mut remote = repo
        .find_remote(remote_name)
        .context(format!("Remote '{}' not found", remote_name))?;

    debug!(
        "Fetching all branches from '{}' in repo {:?}",
        remote_name, repo_path
    );
    let refspec = format!("refs/heads/*:refs/remotes/{}/*", remote_name);

    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(create_git_callbacks());

    remote
        .fetch(&[&refspec], Some(&mut fetch_options), None)
        .context("Fetch failed")?;

    let head = match repo.head() {
        Ok(h) if h.is_branch() => h,
        _ => return Ok(()), // Detached or no head; nothing to fast-forward
    };

    let head_name = head.name().map(|n| n.to_string()).unwrap_or_default();
    if head_name.is_empty() {
        return Ok(());
    }

    let shorthand = head.shorthand().unwrap_or_default().to_string();
    let local_branch = repo
        .find_branch(&shorthand, BranchType::Local)
        .context("Failed to find local branch for HEAD")?;

    let upstream = match local_branch.upstream() {
        Ok(u) => u,
        Err(_) => return Ok(()), // No upstream configured
    };

    let upstream_oid = upstream
        .into_reference()
        .target()
        .context("Upstream reference had no target")?;
    let annotated = repo.find_annotated_commit(upstream_oid)?;
    let (analysis, _pref) = repo.merge_analysis(&[&annotated])?;

    if analysis.is_up_to_date() {
        debug!("Branch '{}' already up to date with upstream", shorthand);
        return Ok(());
    }

    if analysis.is_fast_forward() {
        debug!(
            "Fast-forwarding branch '{}' to upstream ({})",
            shorthand, upstream_oid
        );
        let mut reference = repo
            .find_reference(&head_name)
            .context("Failed to find HEAD reference for fast-forward")?;
        reference
            .set_target(upstream_oid, "Fast-forward to upstream")
            .context("Failed to set reference target during fast-forward")?;
        repo.set_head(&head_name)?;
        repo.checkout_head(Some(
            CheckoutBuilder::default().force(), // ensure worktree matches new commit
        ))?;
    }

    Ok(())
}

/// Resolve the default branch reference (e.g., origin/HEAD) to a revspec string.
pub fn resolve_default_branch(repo: &Repository) -> Option<String> {
    // Try remote HEAD first
    if let Ok(remote) = repo.find_remote("origin") {
        if let Ok(head) = remote.default_branch() {
            if let Some(name) = head.as_str() {
                return Some(name.to_string());
            }
        }
    }

    // Fallback to symbolic reference of HEAD
    if let Ok(head) = repo.head() {
        if let Ok(resolved) = head.resolve() {
            if let Some(name) = resolved.name() {
                warn!("Using HEAD ({}) as base; origin/HEAD not configured", name);
                return Some(name.to_string());
            }
        }
    }

    None
}
