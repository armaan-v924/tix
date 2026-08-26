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
