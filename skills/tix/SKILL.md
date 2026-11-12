---
name: tix
description: Use the tix CLI to create, inspect, extend, and tear down ticket workspaces — a directory per ticket holding a git worktree (and its own branch) per repository. Use when the user mentions tix, asks to start or set up work on a ticket (e.g. "set up JIRA-123"), wants an isolated workspace or worktrees across several repos, asks to add or drop a repo from a ticket, wants to see or clean up their tickets, or when the working directory sits inside a directory containing `.tix/ticket.toml`.
---

# tix

tix is a ticket-scoped workspace manager built on git worktrees. One ticket
is one directory; inside it, each repository the work touches is a git
worktree on its own branch. The user's main checkouts are never switched,
stashed, or moved.

Written against tix 3.3. Check with `tix --version`; when a flag here is
rejected, `tix <command> --help` is the authority.

## Mental model

```
~/code/api                         the user's checkout, untouched
~/tickets/JIRA-123/                a ticket (key JIRA-123)
├── .tix/ticket.toml               the file that makes it a ticket
├── api/                           worktree of ~/code/api, own branch
└── web/                           worktree of ~/code/web, own branch
~/tickets/acme/                    a project (has .tix/project.toml)
└── JIRA-200/                      a ticket inside project "acme"
```

- **Ticket**: a directory with `.tix/ticket.toml`. Not a branch, not a git
  repo. The ticket root itself is not under version control; each worktree
  inside it is its own checkout with its own index.
- **Worktree name vs. repo alias**: worktrees are keyed by directory name,
  which defaults to the repo alias. They differ only when `--as` was used
  (e.g. a second worktree of the same repo). `tix remove` takes the name.
- **Branches are per worktree.** Siblings share a derived name
  (`<branch_prefix>/<key>-<sanitized description>`) but are separate branches
  in separate repos, and nothing keeps them in step.
- **tix never pushes, rebases, renames, or deletes branches.** Committing,
  pushing, and opening PRs are ordinary git work done inside each worktree.
  `tix destroy` removes directories; branches and commits survive.
- **`[defaults]` are creation-time seeds.** They're read once at
  `setup`/`add` and copied into the ticket. Changing config never rewrites an
  existing ticket. A worktree added later derives its branch from the
  *current* `branch_prefix`; pass `--branch` to match its siblings.
- **New branches are cut from fresh `origin/<default>`** (fetched first),
  not from the user's local checkout. `--base` overrides.

## Orient before acting

Run these first; they're read-only and cheap:

```bash
tix --version
tix info --output json      # exit 1 => not inside a ticket
tix list --output json      # every ticket, with project and worktree count
tix config show             # dirs, defaults, registered repositories
```

If every command errors with `config not found`, tix isn't set up. See
[First-time setup](#first-time-setup). Don't improvise a config.

## Rules that matter for agents

1. **Confirmation prompts read stdin, and EOF means "no", with exit 0.**
   `tix destroy` with no TTY and empty stdin prints `aborted` and exits **0**
   while deleting nothing. `tix config add` (duplicate value) and
   `tix config fmt` do the same. Never take exit 0 from these as success.
   - To destroy *safely* (still refusing on uncommitted work):
     `echo y | tix destroy <ticket>`, then confirm the directory is gone.
   - `--force` skips the prompt **and discards uncommitted changes**, and
     deletes the directory even if pruning fails. Use it only when the user
     has explicitly said to throw that work away.
2. **Don't run `tix config init` unattended.** With empty stdin it silently
   accepts every default (`~/tickets`, `~/code`, no seeds) and exits 0. Ask
   the user, or pipe every answer explicitly (see below).
3. **Discovery follows the working directory, and agent shells often reset
   it between calls.** For anything that must hit a specific ticket, pass
   `--ticket <absolute path>` (path form, deterministic), or `cd` in the same
   command: `cd ~/tickets/JIRA-123/api && tix info`.
4. **`--ticket` is read by shape.** A bare name (`JIRA-123`) is *always* an
   id under the tickets directory, or under the current project when you're
   inside one. Anything with a separator, `.`, or `..` is a path. A ticket in
   the current directory must be written `./JIRA-123`. Both forms must point
   at the ticket root; a subdirectory is an error.
5. **Inside a project, bare ids and keys resolve in that project.**
   `tix setup JIRA-9` run from inside `~/tickets/acme/...` creates
   `~/tickets/acme/JIRA-9`. To reach a loose ticket from there, use its path.
6. **stdout carries results only; stderr carries status, warnings, and
   errors.** `tix setup` prints the ticket root. `tix add` prints one path
   per new worktree. So `root=$(tix setup JIRA-1 api)` and
   `cd "$(tix setup JIRA-1 api)"` work. JSON output stays parseable at any
   log level. Use `-q` to keep only warnings and errors on stderr.
7. **"Dirty" includes untracked files.** `destroy` and `remove` refuse on
   any of them. Commit, stash, or ask; don't delete the user's files to
   get past the check, and don't pass `--force` (both commands have it; it
   discards the changes) unless the user said to.
8. **Don't `rm -rf` worktree directories.** Use `tix remove <name>` /
   `tix destroy`, which check for uncommitted work first. If a directory is
   already gone, `tix info` flags it `missing`; `tix remove <name>` drops
   the entry and prunes git's stale registration, with a warning (no
   `--force` needed). `tix add <alias>` can re-create it afterwards. A
   worktree flagged `not-a-git-repository` is dropped too but its directory
   is left in place, and `destroy` refuses it without `--force`.
9. **Unknown subcommands run plugins.** `tix foo` execs `tix-foo` from
   `PATH` as an arbitrary program. Don't guess command names; `tix --help`
   lists built-ins and every plugin it can find.
10. **Don't hand-edit `.tix/ticket.toml` to add or remove worktrees.** Use `tix add`/`tix remove`, which
    create and prune the worktrees too. Unknown keys are errors. Plugin
    tables (`[<plugin>]`) are fine to leave alone; tix preserves them.
11. **Preview anything destructive with `--dry-run`.** `destroy`, `remove`,
    `config set`/`unset`/`remove`/`init`/`fmt`, and `cli update` take it. A
    dry run asks nothing and changes nothing, prints its plan (`would …`
    lines) on stdout, and **exits non-zero where the real run would fail**,
    so its exit code answers "would this work?". It runs the same checks as
    the real command, dirty-worktree refusals included; what it can't
    foresee is git itself failing mid-prune. Combine with `--force` to see
    what a forced run would throw away (`discarding N uncommitted changes`)
    before asking the user for it. `-o json` (or `toml`) prints the plan as
    data, the same shape from every command:
    `{dry_run, command, would_succeed, actions: [{action, summary, …}]}`.
    A plan that would fail still prints, up to the step it would stop at,
    with `would_succeed: false`; the error itself is on stderr.

## Workflows

### Start a ticket

```bash
tix setup JIRA-123 --description "Fix the login bug" api web
tix setup JIRA-123 -d "Fix the login bug"            # repos from [defaults].repositories
tix setup JIRA-123 -d "Fix the login bug" --all      # every registered repo
tix setup JIRA-123 --project acme api                # inside project acme (created on first use)
tix setup ./elsewhere/acme-login --key JIRA-123 api  # out-of-tree; parent dir must exist
tix setup JIRA-123 --base release/2.4 api            # cut branches from somewhere else
```

The key becomes a directory name: `/` and `\` are rejected, everything else
(`REL-1.2.3`) is fine. `--description` feeds the branch name (lowercased,
non-alphanumerics collapsed to `-`, capped at 40 chars). Then work in
`<ticket>/<repo>` with plain git.

### Add or drop a repository

```bash
tix add web --ticket JIRA-123                          # derived branch, from current config
tix add web --ticket JIRA-123 --branch feature/JIRA-123-fix-the-login-bug  # match siblings exactly
tix remove web --ticket JIRA-123                       # prune that worktree; branch survives
```

Removing the last worktree leaves a valid, empty ticket. Re-adding a repo
whose branch already exists checks that branch out as-is and records no
`base`.

### Two worktrees of one repo / stacked changes

```bash
# side-by-side spike: both --as and --branch are required
tix add api --as api-spike --branch spike/JIRA-123-auth

# stacked change: base is a sibling worktree's branch, unpushed commits included
tix add api --as api-part2 --branch stack/JIRA-123-part-2 \
    --base feature/JIRA-123-fix-the-login-bug
```

The recorded `base` is what a PR's base branch should be.

`--base` semantics: a bare branch name is fetched and resolved to whichever
of local `<b>` / `origin/<b>` is ahead (diverged is an error; qualify it).
`origin/<b>`, `refs/heads/<b>`, tags, and SHAs resolve literally. Offline,
tix warns and uses existing refs.

### Inspect

```bash
tix info --ticket JIRA-123 --output json
tix list --output json
tix list --project acme
```

`tix info --output json`:

```json
{
  "description": "Fix the login bug",
  "key": "JIRA-123",
  "path": "/home/you/tickets/JIRA-123",
  "project": null,
  "worktrees": [
    {
      "base": "origin/main",
      "branch": "feature/JIRA-123-fix-the-login-bug",
      "name": "api",
      "path": "/home/you/tickets/JIRA-123/api",
      "repo": "api",
      "status": "ok"
    }
  ]
}
```

`status` is `ok`, `missing`, or `not-a-git-repository`; check it rather
than assuming. `base` is `null` when tix didn't cut the branch. `tix list
--output json` gives `[{key, description, project, worktrees: <count>}]`.

To check whether a ticket exists: `tix info --ticket "$key" -o json
>/dev/null 2>&1` (exit 0 means it exists).

### Tear down

Before destroying, check each worktree for uncommitted or unpushed work
(`git -C <path> status --short`, `git -C <path> log @{u}..` when there's an
upstream) and tell the user what's there. Then:

```bash
tix destroy --dry-run JIRA-123       # the plan; exit 1 names the dirty worktree
echo y | tix destroy JIRA-123        # refuses if any worktree is dirty
test ! -e ~/tickets/JIRA-123 && echo destroyed
```

`tix remove <name> --dry-run` previews a single worktree the same way. The
dry run checks for uncommitted work only, not unpushed commits; that check
is still yours.

`destroy` takes the ticket positionally (id or path) *or* via `--ticket`,
not both. A run that stops partway can be retried; already-pruned worktrees
stay pruned. Branches remain in the source repos afterwards; deleting them is
plain git and the user's call.

### Register repositories

```bash
tix repo add my-org/payments                 # alias "payments"
tix repo add https://github.com/my-org/api --as backend
tix repo add payments --owner my-org         # bare name + owner
tix repo clone --all                         # clone anything not on disk yet
tix repo clone payments
```

A repo has to be registered (and on disk) before any ticket can use it.
The error for an unknown alias lists the registered ones.

### Config

```bash
tix config get defaults.branch_prefix
tix config set defaults.branch_prefix feature
tix config set defaults.branch_prefix fix --dry-run   # validate; print what it replaces
tix config unset defaults.branch_prefix
tix config set defaults.repositories '["api", "web"]'
tix config add defaults.repositories infra      # a duplicate prompts; EOF declines it
tix config remove defaults.repositories api
tix config fmt --check                          # exit non-zero if not in house style
tix config fmt --dry-run                        # print what fmt would write
```

`set`/`unset`/`add`/`remove` preserve comments and layout. `tix config fmt`
rewrites the whole document and may drop comments, so show the user the
`--dry-run` output before running it with `--force`. Values parse as TOML when they can,
otherwise as strings. The config lives at `--config`, else
`$TIX_CONFIG_PATH`, else `~/.config/tix/config.toml` (Linux) /
`~/Library/Application Support/tix/config.toml` (macOS).

### First-time setup

Prefer handing `tix config init` to the user. If they've given you every
answer, pipe them in this order (blank lines leave a seed unset):

1. tickets directory
2. code directory (where repos are cloned)
3. branch prefix
4. GitHub base URL
5. default repository owner
6. repositories a new ticket includes, comma-separated

```bash
printf '%s\n' "$HOME/tickets" "$HOME/code" "feature" "" "my-org" "api,web" | tix config init
```

Then `tix config show` to verify. `--force` overwrites an existing config;
confirm with the user first.

## Errors

| Error (stderr) | Fix |
| --- | --- |
| `config not found` | No config. See first-time setup, or check `--config` / `TIX_CONFIG_PATH`. |
| `not inside a ticket — cd into one or pass --ticket` | cwd isn't in a ticket (or was reset). Pass `--ticket <path\|id>`. Next to a ticket? Use `./KEY`. |
| `not a ticket root` | `--ticket` points at a subdirectory. Point at the ticket root. |
| `'x' is not a registered repository (registered: …)` | `tix repo add`, then `tix repo clone`. |
| `'x' is not a tracked worktree of ticket …` | `remove` takes worktree names; read them from `tix info`. |
| `ticket 'K' already exists` | Use it, or destroy it first (with the user's consent). |
| `the branch this derives, B, is already checked out in worktree 'w'` | Pass `--branch` (and `--as` if the name is taken). |
| `could not prune worktree 'w': … has N uncommitted changes` (from `destroy`) | Commit/stash or ask the user. Retry; it resumes. |
| `could not remove worktree 'w': … has N uncommitted changes` (from `remove`) | Same. `remove --force` discards them; only with the user's say-so. |
| `destroy would stop at worktree 'w': …` / `would refuse to remove worktree 'w': …` (from `--dry-run`) | The real run would fail the same way. Nothing was touched. Fix it, or ask. |
| `warning: worktree 'w' is missing on disk; dropping it …` | Expected after a hand-deleted directory. Nothing to do. |
| `worktree 'w' at … is not a git repository, so tix cannot check it …` (from `destroy`) | Ask the user what to keep from that directory, then `--force`. |
| `left dangling: …` warning after `destroy --force` | Run `git worktree prune` in the named source repo. |
| `unknown command 'x': … no 'tix-x' executable on PATH` | Typo, or the plugin isn't installed. |
| `'a/b' is not a valid ticket key` (from `--key`) | Keys can't contain `/` or `\`. A positional `a/b` is read as a *path* instead, and fails with `… does not exist — create it first`. |
| `<parent> does not exist — create it first` | Path-form `setup` doesn't create parents. Did you mean a bare key? |

Argument-parsing errors (unknown flag, conflicting arguments) exit `2`;
everything else exits `1`. `-v` (trace) or `--log-level debug` shows every step. Pass only one of
`-v`, `-q`, `--log-level`.

## Scripting beyond the CLI

For programmatic access without shelling out, the `pytix` wheel binds the
engine and SDK in Python (`import pytix`). Plugins (`tix-<name>` executables)
receive the resolved ticket context as `--tix-*` flags; `tix-sdk` (Rust) and
`pytix.host` (Python) implement that contract. Docs:
https://tix.armaanv.dev/latest/
