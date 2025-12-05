# tix: quick orientation for agents

- **Purpose**: Rust CLI to manage ticket-scoped git worktrees across multiple repos. All core commands are wired (setup, add, remove, destroy, add-repo, setup-repos, config, doctor, completions).

- **Global behavior**:
  - clap + clap-verbosity-flag + env_logger (`-q` quiet, `-v/-vv` debug).
  - Config at `~/.config/tix/config.toml` (or `XDG_CONFIG_HOME`), loaded/saved via `Config::load/save`.

- **Setup flow**:
  - Creates/reuses `tickets_directory/<ticket>`; stamps `.tix/info.toml` with `{id, description, created_at, branch, repos, repo_branches, repo_worktrees}` (per-repo branch/worktree).
  - Branch: `<branch_prefix>/<ticket>-<sanitized-description>` (lowercase, alnum, single hyphens).
  - Repo selection: `--all` picks all; explicit aliases filtered with warnings; none exits after stamping.
  - Worktrees: `git::create_worktree(repo.path, ticket_dir/alias, branch_name, base)` with base from `--branch` or default branch (prefers `origin/HEAD`, warns when falling back).

- **Config model (`src/core/config.rs`)**:
  - Fields: `branch_prefix`, `github_base_url`, `default_repository_owner`, `code_directory`, `tickets_directory`, `repositories` (alias → `{ url, path }`).
  - `init` prompts via dialoguer; otherwise edit TOML.

- **Git helpers (`src/core/git.rs`)**:
  - `create_worktree` resolves/creates branch (base ref optional, default branch preferred, warns on HEAD fallback) and adds a worktree.
  - `is_clean` checks status; `remove_worktree` prunes by worktree name; `clone_repo` clones; `fetch_and_fast_forward` updates from remote.

- **Commands (implemented)**:
  - `init`: interactive config bootstrap.
  - `add-repo`: parse url/owner+name/name-only, store alias → `{url, path}` under `code_directory`.
  - `setup`: branch + worktrees + metadata stamp; updates metadata when reusing.
  - `add`: infer ticket from `.tix` or flag, refuse overwrite, use stored branch/worktree when present (warn on fallback), optional `--branch` base.
  - `remove`: infer ticket, require clean, delete worktree dir, prune using stored worktree (warn on fallback), update metadata.
  - `destroy`: ensure not inside ticket, clean-check unless `--force`, remove dirs, prune using stored per-repo branch/worktree (warn on fallback).
  - `config`: view/set config keys.
  - `setup-repos`: clone missing repos into `code_directory`.
  - `doctor`: validate config and repo defs; reports warnings/errors.

- **Testing**:
  - Unit tests across helpers.
  - Integration tests (`tests/cli_integration.rs`) cover doctor, setup, add, remove flows with temp git repos (no network).
