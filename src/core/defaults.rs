//! Centralized default values used across commands and config prompts.

pub const DEFAULT_BRANCH_PREFIX: &str = "feature";
pub const DEFAULT_GITHUB_BASE_URL: &str = "https://github.com";
pub const DEFAULT_REPOSITORY_OWNER: &str = "my-org";
pub const DEFAULT_CODE_DIR_BASENAME: &str = "code";
pub const DEFAULT_TICKETS_DIR_BASENAME: &str = "tickets";
pub const DEFAULT_CODE_DIR_FALLBACK: &str = "./code";
pub const DEFAULT_TICKETS_DIR_FALLBACK: &str = "./tickets";
pub const DEFAULT_RELEASE_OWNER: &str = "armaan-v924";
pub const DEFAULT_RELEASE_REPO: &str = "worktree-manager";
pub const UPDATE_USER_AGENT: &str = concat!("tix/", env!("CARGO_PKG_VERSION"));
