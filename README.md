# tix

A ticket-scoped workspace manager built on git worktrees. Each ticket gets
its own directory with per-repository worktrees and branches, so every piece
of work lives in an isolated context you can enter, leave, and destroy
without touching your main checkouts.

## Install

Download a prebuilt binary from the
[latest release](https://github.com/armaan-v924/tix/releases/latest)
(Linux x86_64, macOS aarch64, Windows x86_64), unpack it, and put `tix` on
your `PATH`. Later, `tix cli update` self-updates from GitHub releases.

Or build from source:

```bash
cargo install --path tix-cli
```

## Quickstart

```bash
# Create the global config (~/.config/tix/config.toml or platform equivalent)
tix config init

# Register repositories tix should manage
tix repo add my-org/api
tix repo clone --all      # clone every registered repo not already on disk

# Start work on a ticket: creates the workspace and per-repo worktrees
tix setup JIRA-123

# See what exists
tix list
tix info JIRA-123

# Add or remove a repo's worktree in an existing ticket
tix add api
tix remove api

# Tear the whole ticket down (refuses if worktrees are dirty)
tix destroy JIRA-123
```

`setup`, `list`, `info`, `add`, `remove`, and `destroy` are top-level
aliases for the `tix ticket …` subcommands. Shell completions come from
`tix completions <shell>`.

## Configuration

The global config lives at the platform config directory
(`~/.config/tix/config.toml` on Linux, `~/Library/Application Support/tix/`
on macOS), overridable with `TIX_CONFIG_PATH` or the global `--config` flag.

Read and edit it through the CLI — writes are format-preserving, so your
comments and layout survive:

```bash
tix config show
tix config get <key>
tix config set <key> <value>
```

Defaults set in config are **creation-time seeds**: they apply when a
ticket or worktree is created, and changing them later never rewrites
existing git state.

## Plugins

A tix plugin is an ordinary executable named `tix-<name>` on `PATH`;
`tix <name>` execs it with the workspace context forwarded. Python plugins
ship a `console_scripts` entry point with the same name. See
[PLUGIN_SPEC.md](PLUGIN_SPEC.md) for the full contract, and the crate docs
for `tix-sdk` if you're writing one in Rust.

## Python bindings

`pytix` exposes tix to Python: `pytix.*` binds the engine (repositories,
tickets, worktrees), `pytix.host` binds the SDK (the plugin's handle on the
invoking host). Both ship in every wheel — there are no extras to pick.

Wheels are attached to each [release](https://github.com/armaan-v924/tix/releases/latest),
one per platform. They are `abi3` for CPython 3.11, so a single wheel per
platform installs on any Python 3.11 or newer:

```bash
# Substitute the version and your platform tag; see the release page for the
# exact filenames (macosx_11_0_arm64, manylinux_2_34_x86_64, win_amd64).
pip install https://github.com/armaan-v924/tix/releases/download/v3.1.0/pytix-3.1.0-cp311-abi3-macosx_11_0_arm64.whl
```

Not on PyPI yet. To build one from a checkout:

```bash
just build-wheel aarch64-apple-darwin    # lands in ./dist
```

```python
import pytix

repo = pytix.RepositoryConfig("https://github.com/my-org/api", "/home/me/code/api").ensure("api")
repo.create_worktree("api", "feature/JIRA-123", "/home/me/tickets/JIRA-123/api")
```

## Workspace layout

| Crate | Role |
|-------|------|
| [`tix-engine`](tix-engine) | Domain operations over already-resolved paths; no IO policy, no UI |
| [`tix-sdk`](tix-sdk) | Context and consistency layer shared by the CLI and plugins: config discovery and parsing, ticket discovery, the invocation contract |
| [`tix-cli`](tix-cli) | The `tix` binary — the canonical frontend |
| [`pytix`](pytix) | PyO3 bindings for the engine and SDK |

The design rationale and normative spec live in [design/](design).

## Development

```bash
just ci    # everything CI runs: fmt, clippy, lints, license/version checks, tests
```

See the [justfile](justfile) for individual recipes.

## License

[MIT](LICENSE)
