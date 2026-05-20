use git2;

use std::path::PathBuf;

use crate::types::repository;

pub fn clone(repo: &repository::Repository) -> Result<PathBuf, git2::Error> {
    let git_repo = git2::Repository::clone(&repo.remote, &repo.code)?;
    git_repo
        .workdir()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| git2::Error::from_str("no workdir"))
}
