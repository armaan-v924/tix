---
name: tix
description: Use the tix CLI to create, inspect, extend, and tear down ticket workspaces — a directory per ticket holding a git worktree (and its own branch) per repository. Use when the user mentions tix, asks to start or set up work on a ticket (e.g. "set up JIRA-123"), wants an isolated workspace or worktrees across several repos, asks to add or drop a repo from a ticket, wants to see or clean up their tickets, or when the working directory sits inside a directory containing `.tix/ticket.toml`.
---

# tix

tix is a ticket-scoped workspace manager built on git worktrees: one ticket is
one directory, and each repository the work touches is a git worktree inside
it, on its own branch. The user's main checkouts are never switched or moved.

## Read the full guide first

The full skill (mental model, workflows, the error table) is versioned with
tix and published on its docs site. Fetch the copy that matches the binary
before running tix commands:

1. `tix --version` prints e.g. `tix 3.4.1`.
2. Fetch `https://tix.armaanv.dev/v3.4/skill.md` (that major.minor), e.g.
   `curl -fsSL https://tix.armaanv.dev/v3.4/skill.md`.
3. If that fails (a release older than the hosted guide), fetch
   `https://tix.armaanv.dev/latest/skill.md` instead; for a build from `main`,
   `https://tix.armaanv.dev/dev/skill.md`. Where the guide and the binary
   disagree, `tix <command> --help` wins.

## Offline essentials

When the guide can't be fetched, these are the rules that keep you from
losing the user's work. `tix --help` and `tix <command> --help` cover the rest.

- **Confirmation prompts read stdin, and EOF declines with exit 0.**
  `tix destroy` with empty stdin prints `aborted`, deletes nothing, and
  exits 0. Never read exit 0 from a prompting command as success: check the
  result (`test ! -e <ticket>`). To confirm: `echo y | tix destroy <ticket>`.
- **`--force` discards uncommitted work** (`destroy --force` also deletes the
  ticket when pruning fails). Use it only when the user has said to throw
  that work away. "Uncommitted" includes untracked files.
- **Preview with `--dry-run`** (`destroy`, `remove`, `config
  set`/`unset`/`remove`/`init`/`fmt`, `cli update`). It changes nothing, asks
  nothing, and exits non-zero where the real run would fail; `-o json` prints
  the plan as data.
- **Pass `--ticket <absolute path>`.** Agent shells reset the working
  directory between calls, and discovery follows it. A bare name is always a
  ticket id, never a path; write `./KEY` for one in the current directory.
- **Don't `rm -rf` worktrees or hand-edit `.tix/ticket.toml`.** Use
  `tix remove <name>`, `tix destroy`, and `tix add <alias>`.
- **Don't run `tix config init` unattended**: empty stdin silently accepts
  every default.
- **Unknown subcommands run plugins** (`tix foo` execs `tix-foo` from
  `PATH`). Don't guess command names.
- stdout carries results, stderr carries status and errors; `tix info -o json`
  and `tix list -o json` report state.
