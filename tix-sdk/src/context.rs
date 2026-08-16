//! Frontend context: paths resolved once at startup and handed to every
//! subcommand.
//!
//! Config path resolution is stage 1 of the read path (`design/spec.md`
//! §3.3) and belongs to the frontend/SDK — the engine never reads env vars
//! or flags. Written in `tix-cli` first (#52); promoted here (#96) so plugins
//! resolve identically.
//!
//! The default location is deliberately isolated in [`default_config_path`]
//! so it can be revisited without touching resolution or call sites.

use std::ffi::OsString;
use std::path::PathBuf;
use crate::error::SdkError;
use tracing::debug;

/// The environment variable overriding the global config location, beaten
/// only by the `--config` flag.
pub const CONFIG_PATH_ENV: &str = "TIX_CONFIG_PATH";

/// Paths every subcommand receives, resolved once at startup.
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    /// The resolved global config path. Resolution picks the *location*; the
    /// file may not exist yet (`tix config init` creates it).
    pub config_path: PathBuf,
}

impl Context {
    /// Builds the context from the parsed top-level arguments.
    ///
    /// # Errors
    ///
    /// [`SdkError::ConfigNotFound`] if no flag or env var is set and the
    /// platform config directory cannot be determined.
    pub fn resolve(config_flag: Option<PathBuf>) -> Result<Self, SdkError> {
        Ok(Self {
            config_path: resolve_config_path(config_flag)?,
        })
    }

    /// The config path, verified to exist on disk.
    ///
    /// Subcommands that require config call this; `tix config init` and
    /// `tix cli completions` use [`Self::config_path`] directly.
    ///
    /// # Errors
    ///
    /// [`SdkError::ConfigNotFound`] with the resolved path and a pointer at
    /// `tix config init` when the file does not exist.
    pub fn require_config_path(&self) -> Result<&PathBuf, SdkError> {
        if !self.config_path.is_file() {
            return Err(SdkError::ConfigNotFound(format!(
                "no config file at {} — run `tix config init` to create one",
                self.config_path.display()
            )));
        }
        Ok(&self.config_path)
    }
}

/// Resolves the global config path with the precedence
/// `--config` flag > `TIX_CONFIG_PATH` > platform default.
///
/// Resolution picks a location; it does not require the file to exist —
/// existence is enforced per-subcommand via [`Context::require_config_path`],
/// since `tix config init` must be able to run *before* the file exists.
///
/// # Errors
///
/// [`SdkError::ConfigNotFound`] if neither flag nor env var is set and the
/// platform config directory cannot be determined.
pub fn resolve_config_path(flag: Option<PathBuf>) -> Result<PathBuf, SdkError> {
    resolve_from(
        flag,
        std::env::var_os(CONFIG_PATH_ENV),
        default_config_path(),
    )
}

/// The platform default config location: `dirs::config_dir()/tix/config.toml`
/// (`~/Library/Application Support/tix/config.toml` on macOS,
/// `$XDG_CONFIG_HOME/tix/config.toml` on Linux).
///
/// The single place the default lives — change it here, nowhere else.
pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("tix").join("config.toml"))
}

/// Pure precedence logic, separated from ambient env access for testability.
///
/// An env var that is set but empty is ignored — `TIX_CONFIG_PATH= tix ...`
/// falls through to the default rather than resolving to an empty path.
fn resolve_from(
    flag: Option<PathBuf>,
    env: Option<OsString>,
    default: Option<PathBuf>,
) -> Result<PathBuf, SdkError> {
    if let Some(path) = flag {
        debug!(path = %path.display(), "config path from --config flag");
        return Ok(path);
    }
    if let Some(value) = env
        && !value.is_empty()
    {
        let path = PathBuf::from(value);
        debug!(path = %path.display(), "config path from {CONFIG_PATH_ENV}");
        return Ok(path);
    }
    default.ok_or_else(|| {
        SdkError::ConfigNotFound(
            "cannot determine the platform config directory; \
             pass --config or set TIX_CONFIG_PATH"
                .to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(s: &str) -> Option<PathBuf> {
        Some(PathBuf::from(s))
    }

    /// The `--config` flag beats the env var and the default.
    #[test]
    fn test_flag_wins() {
        let resolved = resolve_from(
            flag("/from/flag.toml"),
            Some(OsString::from("/from/env.toml")),
            Some(PathBuf::from("/from/default.toml")),
        )
        .unwrap();
        assert_eq!(resolved, PathBuf::from("/from/flag.toml"));
    }

    /// The env var beats the default when no flag is given.
    #[test]
    fn test_env_beats_default() {
        let resolved = resolve_from(
            None,
            Some(OsString::from("/from/env.toml")),
            Some(PathBuf::from("/from/default.toml")),
        )
        .unwrap();
        assert_eq!(resolved, PathBuf::from("/from/env.toml"));
    }

    /// An empty env var is ignored rather than resolving to an empty path.
    #[test]
    fn test_empty_env_ignored() {
        let resolved = resolve_from(
            None,
            Some(OsString::new()),
            Some(PathBuf::from("/from/default.toml")),
        )
        .unwrap();
        assert_eq!(resolved, PathBuf::from("/from/default.toml"));
    }

    /// With neither flag nor env var, the platform default applies.
    #[test]
    fn test_default_applies() {
        let resolved =
            resolve_from(None, None, Some(PathBuf::from("/from/default.toml"))).unwrap();
        assert_eq!(resolved, PathBuf::from("/from/default.toml"));
    }

    /// No flag, no env var, no determinable platform directory is an error.
    #[test]
    fn test_no_source_errors() {
        assert!(matches!(
            resolve_from(None, None, None),
            Err(SdkError::ConfigNotFound(_))
        ));
    }

    /// The platform default ends in tix/config.toml.
    #[test]
    fn test_default_shape() {
        if let Some(default) = default_config_path() {
            assert!(default.ends_with("tix/config.toml"));
        }
    }

    /// require_config_path errors for a missing file and passes for a
    /// present one.
    #[test]
    fn test_require_config_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let ctx = Context {
            config_path: path.clone(),
        };

        assert!(matches!(
            ctx.require_config_path(),
            Err(SdkError::ConfigNotFound(_))
        ));

        std::fs::write(&path, "").unwrap();
        assert_eq!(ctx.require_config_path().unwrap(), &path);
    }
}
