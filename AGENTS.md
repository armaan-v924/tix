# tix: quick orientation for agents

- **Purpose**: Rust CLI to manage ticket-scoped git worktrees across multiple repos. Only `tix setup` and completions are wired; the rest are stubs.

- **Global behavior**:
  - Uses clap + clap-verbosity-flag; `env_logger` drives output (`-q` quiet, `-v/-vv` for debug).
  - Config is read/written at the OS config dir (e.g., `~/.config/tix/config.toml`) via `Config::load/save` (no CLI editing yet).

- **Setup flow (implemented)**:
  - Creates or reuses `tickets_directory/<ticket>` and stamps `.tix/info.toml` with `{id, description, created_at}`.
  - Branch name: `<branch_prefix>/<ticket>-<sanitized-description>` (sanitization = lowercase, alnum, single hyphens). Current code already uses the slash form; confirm prefix contents.
  - Repo selection: `--all` picks every configured alias; explicit aliases are filtered with warnings; none selected exits after stamping.
  - Worktrees: for each target alias, call `git::create_worktree(repo.path, ticket_dir/alias, branch_name, None)`; base defaults to `HEAD`, branch created if missing.

- **Config model (`src/config.rs`)**:
  - Fields: `branch_prefix`, `github_base_url`, `default_repository_owner`, `code_directory`, `tickets_directory`, `repositories` (alias → `{ url, path }`).
  - Missing config yields `Config::default()` (empty paths/strings). REQS expects interactive `init` to populate via `dialoguer`.

- **Git helpers (`src/git.rs`)**:
  - `create_worktree` handles branch lookup/creation (optional `base_ref`), then adds a worktree.
  - `is_clean` checks uncommitted/untracked changes.
  - `remove_worktree` prunes by worktree name; `clone_repo` wraps cloning.

- **Command expectations from REQS vs current code**:
  - `init`: interactive config bootstrap (dialoguer). **Not implemented.**
  - `add-repo`: implemented. Parses url/owner+name/name-only, builds URL from config defaults, stores alias → `{url, path}` (path under `code_directory`).
  - `setup`: implemented as above; needs branch format alignment and better config defaults/validation.
  - `setup-repos`: implemented. Clones any missing repos from `repositories` into `code_directory`.
  - `config`: implemented. View/set core config keys (`branch_prefix`, `github_base_url`, `default_repository_owner`, `code_directory`, `tickets_directory`).
  - `destroy`: implemented. Checks you’re not inside the ticket dir, verifies worktrees clean unless `--force`, removes dirs, prunes worktree metadata using computed branch name.
  - `add`: add a single repo worktree to an existing ticket; infer ticket via `.tix` when omitted. **Not implemented.**
  - `remove`: remove one repo worktree with clean-check + prune. **Not implemented.**
  - `config`: view/edit individual config values. **Not implemented.**
  - `setup-repos`: clone all registered repos into `code_directory` when missing. **Not implemented.**

- **Testing/validation gaps**:
  - No automated tests; branch-name sanitization and repo selection logic need coverage.
  - Config defaults are unchecked (empty paths), so add validation before enabling other commands.
