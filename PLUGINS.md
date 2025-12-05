# Python plugin support (design sketch)

## Goal
Allow users to author Python-based plugins that register as subcommands under `tix`, run within a ticket directory, and receive sufficient context (ticket metadata, repo paths, config) to be useful.

## UX surface
- Discoverable subcommands: `tix <plugin-name> [args...]` where `<plugin-name>` comes from registered plugins.
- Plugin registry: e.g., `~/.config/tix/plugins.d/*.toml` or a section in `config.toml` with entries like:
  ```toml
  [plugins.myplugin]
  entrypoint = "/path/to/my_plugin.py"
  description = "Does something"
  ```
- Command help: `tix help` lists plugin subcommands with description pulled from registration metadata; `tix <plugin> --help` delegates to plugin-provided help text.

## Execution model (embedded interpreter)
- Embed Python via `pyo3`/`cpython` crates; load plugins as Python modules (module path + entry function, e.g., `my_plugin:run`).
- Working directory: the ticket directory (or specific repo worktree), so file operations are natural.
- Context delivery: pass a Rust-constructed context object into Python directly (no env/stdin):
  - Expose a `tix` Python module with a `PluginContext` class containing ticket metadata (`id`, `description`, `branch`, `repo_branches`, `repo_worktrees`), config paths, repo aliases/paths, and helpers (e.g., read/write `.tix`, logging).
  - Pass CLI args as a Python list to the plugin entry function.
- Python runtime: require a system Python (via `python3-sys`/`cpython`) or bundle a minimal interpreter; decide on policy per platform.

## Safety and validation
- Ensure execution occurs inside a ticket directory or a valid repo worktree (or error with guidance).
- Validate plugin registration: entrypoint exists, is executable, and is not a directory.
- Optional allowlist: execute only from a configured plugin directory to avoid arbitrary path execution.

## Implementation outline
- Add a `plugins` module:
  - Registry loader: read plugin definitions from config (`config.plugins` map) or `plugins.d/*.toml`.
  - Resolver: map subcommand name → plugin metadata (path, description, optional python interpreter).
  - Context builder: construct a JSON serializable struct with ticket metadata, config, and repo paths.
  - Runner: spawn `python3 <entrypoint> -- <args...>` with cwd set to ticket dir and env var pointing to context JSON.
- CLI integration:
  - Extend `Cli` subcommands to include a dynamic `Plugin { name: String, args: Vec<String> }`.
  - Before clap parsing, load plugin names to register them as known subcommands (or use a custom subcommand handler for unknown names).
  - `tix plugins list` to show registered plugins with descriptions.
- Testing:
  - Unit tests for registry parsing and context building.
  - Integration test with a temp plugin script that echoes context to stdout; verify execution under a temp ticket dir.

## Open questions
- Should plugins be isolated (virtualenv) or just use system Python?
- How to handle plugin help text? (Option: `tix <plugin> --help` invokes plugin with `--help` and relays output.)
- Do we support plugin-defined flags/completions? (We can skip completions initially.)
- Security posture: warn on running plugins outside trusted directories; add a config flag to disable plugins entirely.
