//! Credential wiring for authenticated remotes.
//!
//! libgit2 does not consult git's credential helpers on its own. Without an
//! explicit callback, any remote that asks for authentication fails with
//! "remote authentication required but no callback set" — including every
//! private repository and every SSH remote. This module supplies the
//! callback and the [`git2`] option structs that carry it.
//!
//! Credentials themselves are never read, stored, or logged by tix: the
//! callback delegates to the ssh-agent and to whatever credential helper the
//! user's git config already declares, so tix inherits the machine's
//! existing setup rather than defining one of its own.

use git2::{Cred, CredentialType, Error, FetchOptions, RemoteCallbacks};
use tracing::debug;

/// How many times a single remote may be asked for credentials.
///
/// libgit2 re-invokes the callback when the server rejects what it was
/// given. A helper that keeps returning the same wrong answer would loop
/// forever, so refuse once the plausible options are exhausted.
const MAX_ATTEMPTS: usize = 3;

/// Remote callbacks that answer credential challenges.
///
/// Resolution order follows what the server offers: an ssh-agent key for SSH
/// remotes, then the git credential helper for HTTPS, then the default
/// (which covers negotiated schemes and unauthenticated remotes that still
/// raise a challenge).
pub(crate) fn remote_callbacks<'cb>() -> RemoteCallbacks<'cb> {
    let mut callbacks = RemoteCallbacks::new();
    let mut attempts = 0usize;

    callbacks.credentials(move |url, username_from_url, allowed| {
        attempts += 1;
        if attempts > MAX_ATTEMPTS {
            return Err(Error::from_str(
                "authentication failed: exhausted available credentials",
            ));
        }
        debug!(attempt = attempts, ?allowed, "credential challenge");

        if allowed.contains(CredentialType::SSH_KEY) {
            return Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            let config = git2::Config::open_default()?;
            return Cred::credential_helper(&config, url, username_from_url);
        }
        if allowed.contains(CredentialType::DEFAULT) {
            return Cred::default();
        }
        Err(Error::from_str(
            "authentication failed: no supported credential type offered",
        ))
    });

    callbacks
}

/// Fetch options carrying the credential callbacks.
///
/// Use for every network operation — a bare `None` skips authentication
/// entirely and fails on any remote that challenges.
pub(crate) fn fetch_options<'cb>() -> FetchOptions<'cb> {
    let mut options = FetchOptions::new();
    options.remote_callbacks(remote_callbacks());
    options
}
