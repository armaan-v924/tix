use serde::{Deserialize, Serialize};

/// The `[defaults]` section of the global config — **seeds, not defaults**.
///
/// Values here are read **once** by `tix ticket setup` at ticket creation and
/// written into the ticket document. Changing a global value later affects
/// only new tickets; there is no runtime override/fallback resolver anywhere
/// in tix. Anything that shapes git state must be a seed — topology on disk
/// cannot be retroactively rewritten by config
/// (see [creation-time seeds](https://tix.armaanv.dev/latest/concepts/seeds/)).
///
/// Every field is optional in the document: a partial (or absent) `[defaults]`
/// section is normal, and [`Default`] gives the all-empty value for the
/// `section_or_default` read path.
///
/// The field set adopts v2's provisionally and may expand or retract as
/// `tix ticket setup` takes shape.
///
/// # Examples
///
/// A partial `[defaults]` section parses; unset fields stay `None`/empty:
///
/// ```
/// # use tix_engine::Defaults;
/// let defaults: Defaults = toml::from_str(
///     r#"
///     branch_prefix = "feature"
///     repositories = ["backend", "frontend"]
///     "#,
/// )
/// .unwrap();
///
/// assert_eq!(defaults.branch_prefix.as_deref(), Some("feature"));
/// assert_eq!(defaults.repositories, vec!["backend", "frontend"]);
/// assert!(defaults.github_base_url.is_none());
/// assert!(defaults.default_repository_owner.is_none());
/// ```
///
/// An empty document is the same as no section at all:
///
/// ```
/// # use tix_engine::Defaults;
/// let defaults: Defaults = toml::from_str("").unwrap();
/// assert_eq!(defaults, Defaults::default());
/// ```
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Seed for branch name derivation at `tix ticket setup`:
    /// `<prefix>/<key>-<sanitized-description>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_prefix: Option<String>,
    /// Seed for remote URL construction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_base_url: Option<String>,
    /// Seed for remote URL construction: the owner used when a repository is
    /// given without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_repository_owner: Option<String>,
    /// Which registered repositories a new ticket includes.
    ///
    /// Seeds a *new* ticket. The ticket document's worktree map records what
    /// a ticket actually has; changing this leaves existing tickets alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_defaults() -> Defaults {
        Defaults {
            branch_prefix: Some("feature".to_string()),
            github_base_url: Some("https://github.mycompany.com".to_string()),
            default_repository_owner: Some("my-org".to_string()),
            repositories: vec!["backend".to_string(), "frontend".to_string()],
        }
    }

    /// A fully populated `[defaults]` section deserializes correctly.
    #[test]
    fn test_deserialize_full_section() {
        let toml = r#"
branch_prefix = "feature"
github_base_url = "https://github.mycompany.com"
default_repository_owner = "my-org"
repositories = ["backend", "frontend"]
"#;
        let defaults: Defaults = toml::from_str(toml).unwrap();
        assert_eq!(defaults, sample_defaults());
    }

    /// Serializing and deserializing preserves all data.
    #[test]
    fn test_round_trip() {
        let defaults = sample_defaults();
        let restored: Defaults = toml::from_str(&toml::to_string(&defaults).unwrap()).unwrap();
        assert_eq!(restored, defaults);
    }

    /// Unset optional fields serialize to nothing rather than explicit nulls,
    /// so a default value round-trips through an empty document.
    #[test]
    fn test_default_serializes_empty() {
        assert_eq!(toml::to_string(&Defaults::default()).unwrap(), "");
    }

    /// An empty document parses to the all-empty `Default` value.
    #[test]
    fn test_empty_section() {
        let defaults: Defaults = toml::from_str("").unwrap();
        assert_eq!(defaults, Defaults::default());
    }

    /// Unknown fields in the `[defaults]` section are rejected.
    #[test]
    fn test_rejects_unknown_fields() {
        let toml = r#"
branch_prefix = "feature"
base_branch = "not-a-seed-we-know"
"#;
        assert!(toml::from_str::<Defaults>(toml).is_err());
    }
}
