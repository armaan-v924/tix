# Plugins

This document describes the Python plugin API for tix, how plugins are loaded, and how to develop
and run them locally.

## Overview
- Plugins are Python scripts registered under `[plugins.<name>]` in `config.toml`.
- Plugins are executed via `uv run` and must live inside a uv project (a `pyproject.toml` must be
  present in the entrypoint's parent directory tree).
- Each plugin must export a `main(context, argv)` function.

## Configuration
Register a plugin in `~/.config/tix/config.toml`:

```toml
[plugins.myplugin]
entrypoint = "/absolute/path/to/plugin.py"
description = "Does something useful"
python = "3.12"
```

Notes:
- `entrypoint` may be absolute or relative to the config directory.
- `python` is optional; when set, tix passes it to `uv run --python`.

## Execution model
tix runs:

```
uv run --project <pyproject-root> -- python -c <shim> <entrypoint> [args...]
```

Your plugin should provide:

```python
def main(context, argv):
    ...
```

`argv` is a list of CLI arguments passed after the plugin name.

## Context API
`context` is a `TixPluginContext` dataclass created by tix and populated from JSON.

Fields:
- `plugin_name` (str): registered plugin name.
- `ticket_root` (str): absolute ticket root path.
- `current_working_dir` (str): working directory when tix was invoked.
- `current_repo_alias` (str | None): repo alias if invoked from a repo worktree.
- `current_repo_path` (str | None): repo worktree path if invoked from a repo worktree.
- `ticket` (dict): parsed `.tix/info.toml` metadata.
- `config` (dict): full config snapshot at invocation time (read-only by convention).
- `code_directory` (str): configured code directory.
- `tickets_directory` (str): configured tickets directory.
- `plugin_cache_dir` (str): global cache directory for this plugin.
- `plugin_state_dir` (str): global state directory for this plugin.
- `plugin_ticket_state_dir` (str): per-ticket state directory for this plugin.
- `repositories` (dict): repo definitions from config (`alias -> { url, path }`).

Ticket schema (`context.ticket`):
- `id` (str)
- `description` (str | None)
- `created_at` (str, ISO 8601)
- `branch` (str)
- `repos` (list[str])
- `repo_branches` (dict[str, str])
- `repo_worktrees` (dict[str, str])

## Environment variables
tix sets:
- `TIX_CONTEXT_PATH`: JSON file path containing the context payload.
- `TIX_TICKET_ROOT`: ticket root directory.
- `TIX_PLUGIN_CACHE_DIR`: global cache directory for this plugin.
- `TIX_PLUGIN_STATE_DIR`: global state directory for this plugin.
- `TIX_PLUGIN_TICKET_STATE_DIR`: per-ticket state directory for this plugin.

## Storage conventions
- Global cache: `XDG_CACHE_HOME/tix/plugins/<name>` (or OS cache dir).
- Global state: `XDG_STATE_HOME/tix/plugins/<name>` (or OS state dir).
- Per-ticket state: `<ticket>/.tix/plugins/<name>`.

Use cache for regeneratable data, global state for durable preferences, and per-ticket state for
ticket-scoped outputs.

## Development workflow
1) Copy the template plugin:
   - `cp -R plugins/template /path/to/my-plugin`
2) Rename `pyproject.toml` project name.
3) Edit `plugin.py` and implement `main(context, argv)`.
4) Register in `config.toml`.
5) Run with `tix <plugin-name> [args...]`.

## Installing plugins
Prerequisites:
- Install `uv` and ensure it is on your `PATH`.

Install steps:
1) Create or clone a plugin folder that contains a `pyproject.toml`.
2) (Optional) Add dependencies to `pyproject.toml`.
3) Register the plugin entrypoint:
   - `tix plugins register my-plugin /absolute/path/to/plugin.py -d "Does something"`
4) Verify registration:
   - `tix plugins list`
5) Run it:
   - `tix my-plugin --help`

## Troubleshooting
- "No pyproject.toml found": your entrypoint must live within a uv project.
- "Plugin must define a main(context, argv)": export a `main` function in your entrypoint.
