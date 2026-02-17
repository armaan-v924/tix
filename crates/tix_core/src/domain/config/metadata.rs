use crate::domain::ids::{PluginName, RepoAlias};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEFAULT_BRANCH_PREFIX: &str = "feature";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Where to put tickets (ticket roots live under here).
    pub tickets_directory: PathBuf,

    /// Where your repos live (optional, but useful for defaults).
    pub code_directory: PathBuf,

    /// E.g. `feature` -> default branch becomes `feature/ABC-123...`
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,

    /// Repo definitions keyed by alias.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repositories: BTreeMap<RepoAlias, RepoDefinition>,

    /// Plugin definitions keyed by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<PluginName, PluginDefinition>,

    /// Optional GitHub base URL (enterprise) and default owner/org.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_repository_owner: Option<String>,

    /// Optional JIRA base URL (if you want link generation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira_base_url: Option<String>,
}

fn default_branch_prefix() -> String {
    DEFAULT_BRANCH_PREFIX.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoDefinition {
    /// Clone URL (https or ssh)
    pub url: String,

    /// Local path to repo (optional; can be derived)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDefinition {
    /// Script path or executable.
    pub entrypoint: PathBuf,

    /// Human description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,

    /// Optional python selection (uv `--python` style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,
}