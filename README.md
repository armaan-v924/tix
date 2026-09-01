# tix

A ticket-scoped workspace manager built on git worktrees. Each ticket gets
its own directory with per-repository worktrees and branches, so every piece
of work lives in an isolated context you can enter, leave, and destroy
without touching your main checkouts.

**Documentation: [tix.armaanv.dev](https://tix.armaanv.dev)**

## Install

Download the binary for your platform from the
[latest release](https://github.com/armaan-v924/tix/releases/latest) and put
it on your `PATH`; `tix cli update` keeps it current. Or build from source
with `cargo install --path tix-cli`.

Full instructions, including shell completions, are in
[Install](https://tix.armaanv.dev/latest/start/install/).

## Quickstart

```bash
tix config init                 # create the global config
tix repo add my-org/api         # register a repository
tix repo clone --all            # clone what is not already on disk

tix setup JIRA-123              # a workspace, with a worktree per repo
cd ~/tickets/JIRA-123/api       # an ordinary checkout on its own branch

tix list                        # every ticket
tix destroy JIRA-123            # tear it down; refuses if anything is dirty
```

[Getting started](https://tix.armaanv.dev/latest/start/getting-started/)
walks this through properly. Every command and flag is in the
[CLI reference](https://tix.armaanv.dev/latest/reference/cli/), generated
from the CLI's own argument definition.

## Extending it

A plugin is an executable named `tix-<name>` on your `PATH`; `tix <name>`
runs it with the workspace context forwarded. There are SDKs for Rust and
Python — see
[writing a plugin](https://tix.armaanv.dev/latest/plugins/writing-a-plugin/)
and the [specification](https://tix.armaanv.dev/latest/plugins/specification/).

`pytix` also exposes tix to Python directly, for scripting rather than
plugins: [pytix](https://tix.armaanv.dev/latest/concepts/pytix/).

## Repository layout

| Crate | Role |
|-------|------|
| [`tix-engine`](tix-engine) | Domain operations over already-resolved paths; no IO policy, no UI |
| [`tix-sdk`](tix-sdk) | Context and consistency layer shared by the CLI and plugins: config discovery and parsing, ticket discovery, the invocation contract |
| [`tix-cli`](tix-cli) | The `tix` binary — the canonical frontend |
| [`pytix`](pytix) | PyO3 bindings for the engine and SDK |
| [`xtask`](xtask) | Generates the CLI and configuration references from the source |

API documentation for every crate is published at
[tix.armaanv.dev/latest/crates](https://tix.armaanv.dev/latest/crates/).

## Development

```bash
just ci         # everything CI runs: fmt, clippy, lints, license/version checks, tests
just docs-cli   # regenerate the CLI reference and man pages
just docs-serve # serve the documentation site locally
```

See the [justfile](justfile) for individual recipes. Anything under
`docs/man/`, `docs/src/content/docs/reference/`, or `docs/src/data/` is
generated — edit the generator in [`xtask/`](xtask), and `just check-docs`
will tell you when they disagree.

## License

[MIT](LICENSE)
