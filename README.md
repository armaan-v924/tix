# tix

Rust CLI for managing ticket-scoped git worktrees across multiple repositories. Each ticket gets its own workspace with per-repo worktrees, branches, and metadata to keep contexts isolated.

## Features
- `setup <ticket>`: Create a ticket workspace, stamp metadata, compute branch `<prefix>/<ticket>-<sanitized-description>`, and create worktrees for selected/all repos (fetch/fast-forward before branching). Metadata tracks per-repo branches and sanitized worktree names.
- `add <repo>`: Add a repo worktree to an existing ticket (infers ticket from current `.tix` when omitted), reuses stored branch/worktree when present, refuses to overwrite existing worktree.
- `remove <repo>`: Clean-check, delete worktree dir, prune stored worktree metadata, update ticket metadata.
- `destroy <ticket>`: Ensure you’re not inside the ticket, clean-check unless `--force`, delete ticket dir, prune worktrees using stored per-repo branches/worktrees (warns on fallback).
- `setup-repos`: Clone missing repos from config into your code directory.
- `add-repo`: Register a repo alias (url/owner+name/name-only parsing).
- `config <key> [value]`: View/set core config fields.
- `doctor`: Validate config and report warnings/errors.
- `update`: Self-update from the latest GitHub release.
- `tix <plugin> [args...]`: Run a registered Python plugin inside the ticket workspace.
- Shell completions via `tix completions`.

## Configuration
Stored at `~/.config/tix/config.toml` (or `XDG_CONFIG_HOME`):
```toml
branch_prefix = "feature"
github_base_url = "https://github.com"
default_repository_owner = "my-org"
code_directory = "/path/to/code"
tickets_directory = "/path/to/tickets"

[repositories.api]
url = "https://github.com/my-org/api.git"
path = "/path/to/code/api"

[plugins.myplugin]
entrypoint = "/path/to/plugin.py"
description = "Do something useful"
```
Initialize interactively with `tix init`, or edit the file directly.
Supported keys: `branch_prefix`, `github_base_url`, `default_repository_owner`, `code_directory`, `tickets_directory`.

Examples:
- Show a value: `tix config branch_prefix`
- Set a value: `tix config branch_prefix hotfix`
- Show full config: `tix config`
- Edit config in `$EDITOR`: `tix config --edit`

## Metadata
Each ticket directory contains `.tix/info.toml` with:
```
id, description, created_at, branch,
repos, repo_branches (alias -> branch), repo_worktrees (alias -> sanitized name)
```
Commands prefer stored branches/worktrees and warn when falling back to computed values.

## Usage
- Create ticket with all repos: `tix setup JIRA-123 --all -d "Short summary"`
- Add another repo to an existing ticket: `cd ~/tickets/JIRA-123 && tix add web`
- Remove a repo worktree: `tix remove api`
- Destroy a ticket (force): `tix destroy JIRA-123 --force`
- Clone missing repos: `tix setup-repos`
- Doctor: `tix doctor`
- List plugins: `tix plugins list`
- Register a plugin: `tix plugins register my-plugin /path/to/plugin.py -d "Does stuff"`
- Remove a plugin (and cache): `tix plugins deregister my-plugin`
- Clear caches: `tix plugins clean` or `tix plugins clean my-plugin`

## Plugins
Plugins are registered under `[plugins.<name>]` in `config.toml` with an `entrypoint` pointing to a Python script.
Plugins are executed via `uv run` and must live inside a uv project (`pyproject.toml` present).
Your plugin should export `main(context, argv)` where `argv` is a list of CLI args.
When you run `tix <plugin>`, tix sets the working directory to the ticket root and exposes:
- `TIX_CONTEXT_PATH`: JSON file containing ticket metadata, config snapshot, and repo definitions.
- `TIX_TICKET_ROOT`: absolute path to the ticket directory.
- `TIX_PLUGIN_CACHE_DIR`: plugin-specific cache directory under `XDG_CACHE_HOME/tix/plugins/<name>` (or OS cache dir).
- `TIX_PLUGIN_STATE_DIR`: plugin-specific global state directory under `XDG_STATE_HOME/tix/plugins/<name>` (or OS state dir).
- `TIX_PLUGIN_TICKET_STATE_DIR`: per-ticket state directory under `<ticket>/.tix/plugins/<name>`.

See `plugins/PLUGINS.md` for the full API and development docs, plus a template plugin.

Quick install:
1) Install `uv` and ensure it is on your `PATH`.
2) Clone or create a plugin folder with a `pyproject.toml`.
3) Register it: `tix plugins register my-plugin /abs/path/to/plugin.py -d "Does something"`.
4) Run it: `tix my-plugin --help`.

## Installation
- Quick install script (macOS/Linux): `curl -fsSL https://raw.githubusercontent.com/armaan-v924/worktree-manager/main/install_tix.sh | bash`
- Prebuilt binaries: download the archive from the GitHub release matching your OS (`tix-<version>-linux-x86_64`, `tix-<version>-macos-aarch64`, `tix-<version>-windows-x86_64`), unpack, and place `tix`/`tix.exe` on your `PATH`.
- From source: `cargo install --path .` (requires Rust toolchain).
- Completions: `tix completions <shell>` and follow your shell’s instructions.

## Development
- Tests: `cargo test` (unit + integration). Integration tests use temp git repos; no network needed.
- Logging: `-q` to quiet, `-v/-vv` for debug.

## Notes / Caveats
- Base branch resolution prefers `origin/HEAD`; warns and falls back to `HEAD` if not configured.
- When a matching `origin/<branch>` exists, new worktrees set upstream tracking automatically.
- Destructive commands refuse to run when inside the target ticket directory.
- Safety checks: `remove` and `destroy` require clean worktrees unless `--force` (destroy).

## Update
The `tix update` command downloads the latest GitHub release for your platform and replaces the current binary.
Environment overrides:
- `TIX_UPDATE_OWNER`: release owner (default `armaan-v924`)
- `TIX_UPDATE_REPO`: release repo (default `worktree-manager`)
- `TIX_INSTALL_PATH`: explicit install destination (defaults to the current executable path)

Supported platforms:
- `linux-x86_64`
- `macos-aarch64`
- `windows-x86_64`
