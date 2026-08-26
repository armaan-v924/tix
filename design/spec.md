# tix v3 — Crate architecture & plugin system: specification

Status: **spec** — normative. Derived from the working design in
`design/2026-07-18.md`; rationale and rejected alternatives live there and
are not repeated here except where load-bearing. Requirement keywords
(MUST/SHOULD/MAY) are used in the RFC 2119 sense.

---

## 1. Scope

Specifies:

- the workspace crate structure and dependency rules (§2),
- the two-document config model and its read/write API (§3),
- ticket discovery (§4),
- the host ↔ plugin invocation contract, including the protocol version
  (§5),
- config writes via diff-back deltas (§6),
- `pytix` packaging constraints (§7).

Out of scope (deferred, tracked in §8): v2→v3 migration, the `-C` flag,
repo/cwd context beyond `--tix-repo`/`--tix-repo-dir`.

---

## 2. Crate architecture

### 2.1 Crates

| Crate        | Role | Depends on | Key external deps |
|--------------|------|------------|-------------------|
| `tix-engine` | Domain operations over **already-resolved** paths. Section types `EngineConfig`, `TicketConfig`, `Defaults`. | — | `git2`, `serde` (`toml` as dev-dep only — parsing is the SDK's job; the `ParseError(toml::de::Error)` variant in `errors.rs` migrates to the SDK) |
| `tix-sdk` (new) | The context-and-consistency layer shared by `tix-cli` and plugins: invocation-contract parsing, config discovery/parsing, typed section access, ticket discovery, delta writing, spawn helper. | `tix-engine` (re-exported) | `clap`, `dirs`, `toml_edit` (or equivalent format-preserving DOM) |
| `tix-cli`    | The canonical frontend; binary `tix`. | `tix-sdk` **only** | `clap` |
| `pytix`      | PyO3 bindings: `pytix.*` (engine) + `pytix.host` (SDK). | `tix-sdk`, `tix-engine` | `pyo3` |

### 2.2 Dependency rules

- Dependency chain is linear: `{tix-cli, pytix} → tix-sdk → tix-engine`.
- `tix-sdk` MUST re-export `tix-engine` (tokio/axum style). `tix-cli`
  MUST NOT declare a direct `tix-engine` dependency.
- Non-CLI frontends MAY depend on `tix-engine` directly (bypassing the
  SDK) when they want no tix layout policy.
- `tix-engine` MUST NOT contain discovery, path resolution, or layout
  policy, and MUST NOT have runtime deps on `clap`, `dirs`, or `toml` —
  those concerns (arg parsing, config location, document parsing) are all
  SDK-side.
- `tix-sdk` is deliberately coupled to `tix-cli`. Surface grows by
  promotion from `tix-cli` (demand direction CLI → SDK), never by
  speculation about plugin needs. Promotion criterion: annoyance decides
  *whether*; shape (resolved-path domain op vs. frontend policy) decides
  *where*.

### 2.3 Versioning

- All workspace crates version in lockstep and release together.
- Crate versions carry **no** compatibility semantics. The only
  compatibility boundary is host ↔ plugin, governed solely by the
  protocol integer (§5.4).

---

## 3. Config model

### 3.1 Documents

Two independent TOML documents. They are different documents, not layers
of one schema; there is **no** document merge and **no** runtime
default-resolution mechanism.

1. **Global config** — the environment: directories, defaults, the
   registered repository map. Located via `dirs`.
2. **Ticket document** — `<ticket_root>/.tix/ticket.toml`: ticket
   identity, description, contained repos, and a **branch per repo**.

### 3.2 Sections

Each document is a set of sections, one table per consumer:

| Document | Section | Type | Owner |
|----------|---------|------|-------|
| global | `[engine]`   | `EngineConfig` | `tix-engine` |
| global | `[cli]`      | `CliConfig`    | `tix-cli` |
| global | `[defaults]` | `Defaults`     | `tix-engine` |
| global | `[<plugin>]` | plugin's own   | that plugin |
| ticket | `[ticket]`   | `TicketConfig` | `tix-engine` |
| ticket | `[<plugin>]` | plugin's own   | that plugin |

- There is **no top-level typed struct** for either document. The current
  `Config { engine: Engine }` wrapper in
  `tix-engine/src/types/config.rs` MUST be replaced: `Engine` becomes the
  exported section type `EngineConfig`; the document level belongs to the
  SDK's generic parsed tree.
- Section types use `deny_unknown_fields`, which applies to that
  section's subtree only.
- `TicketConfig` (`tix-engine/src/types/ticket.rs`) is the `[ticket]`
  section type, not the whole ticket document. Worktree state is keyed
  by **worktree directory name**, not repo alias:
  `worktrees: HashMap<name, { repo, branch }>` — the key is the
  directory under the ticket root, the single-worktree case degenerates
  to `name == alias`, and multiple worktrees of one repo (#85) need no
  schema change. The current `Ticket.branch: String` single shared
  branch is an oversight and MUST be corrected — worktrees share a
  branch *prefix*, not a branch (v2 parity: `repo_branches` /
  `repo_worktrees`).

### 3.3 Read API (SDK)

Two stages:

1. **Locate and parse** — identical for every consumer, owned by the SDK.
   Path source: discovery (§4) for the CLI, `--tix-config`/`--tix-ticket`
   for plugins. Output: a generic, format-preserving document
   (`toml_edit::DocumentMut` or equivalent — the requirement is a generic
   DOM; verify the crate API at implementation time). No types involved.
2. **Extract typed sections on demand**, by whoever owns the type:

   ```rust
   let doc = ctx.global();                              // parsed, untyped
   let engine: EngineConfig = doc.section("engine")?;   // tix-engine's type
   let cli:    CliConfig    = doc.section("cli")?;      // tix-cli's type
   let mine:   MyPluginCfg  = doc.section("myplugin")?; // plugin's type
   ```

The SDK MUST offer both `section::<T>() -> Option<T>` and
`section_or_default::<T>() -> T where T: Default` — absent and empty are
meaningfully different for some consumers.

The same document object serves reads and writes: typed sections are
extracted from it, deltas applied against it, unknown sections ride
through untouched.

### 3.4 `[defaults]` — seeds, not defaults

- Values in `[defaults]` are **seeds**: read once by `tix ticket setup` at ticket
  creation and written into the ticket document. Later changes to global
  values affect only new tickets.
- There is **no runtime override/fallback resolver**. Anything that
  shapes git state MUST be a seed (topology on disk cannot be
  retroactively rewritten by config). Should a genuinely lazy,
  behavioral-only preference appear later, its fallback is introduced
  then, for that field alone.

**Seed field set** (resolved: adopt v2's set for now; expand/retract as
needed):

| Field | Seeds | v2 equivalent |
|-------|-------|---------------|
| `branch_prefix` | branch name derivation at `tix ticket setup` (`<prefix>/<key>-<sanitized-description>`) | `branch_prefix` |
| `github_base_url` | remote URL construction | `github_base_url` |
| `default_repository_owner` | remote URL construction | `default_repository_owner` |
| `repositories` | which repos a new ticket includes | — |

Note `defaults.repositories` vs `ticket.repositories` are different
fields, not an override pair: the former is what to seed with, the latter
is what the ticket has.

### 3.5 Plugin state vs. plugin config

- **Plugin config** — human-editable settings. Lives as a `[<plugin>]`
  table in the appropriate document. Read via section accessors, written
  via diff-back (§6).
- **Plugin state** — caches and derived data of any shape/size. Plain
  files in a directory. Not part of any document, delta, or protocol.

Locations are **SDK helpers, not contract flags** (no env vars, no new
`--tix-*` flags):

- Per-ticket state: `<ticket_root>/.tix/plugins/<name>/`, derived from
  `--tix-ticket`.
- Global state/cache: derived from plugin name + standard OS dirs
  (`dirs`); offered as convenience only.

Helpers MUST create directories lazily on first use (v2 pre-created them
every invocation, littering empty dirs).

---

## 4. Ticket discovery

Owned by `tix-cli`, promoted into `tix-sdk` so plugins resolve
identically. Not an engine concern.

**Algorithm** — walk upward from cwd:

1. Walk the **logical** `$PWD` as the shell reports it; MUST NOT
   canonicalize (symlinked ticket dirs are normal; host and SDK must
   agree; matches git).
2. Match predicate is the **file** `.tix/ticket.toml`, not the `.tix/`
   directory (projects live above tickets with their own `.tix/`; one
   pass collects nearest `ticket.toml` = ticket and nearest
   `project.toml` above = project).
3. Nearest ancestor wins.
4. Ceiling: filesystem root; stop early at a device boundary (matching
   git's `GIT_DISCOVERY_ACROSS_FILESYSTEM` default).
5. Discovery MUST NOT be bounded by `tickets_root` from global config.
   Safety net: if the resolved ticket is not under `tickets_root`, log at
   debug and proceed.

**Override** (skips the walk): a single flag, `--ticket <path | id>`,
disambiguated by shape:

- **Path form** — the argument contains a path separator, is absolute,
  or is `.`/`..`: asserts *this path is the ticket root*; MUST error if
  it is not one; leaves cwd alone.
- **Id form** — any bare name: resolved as `tickets_directory.join(id)`
  from the CLI config section (v2 parity); MUST error if the result is
  not a ticket root.

A bare name is always an id; a ticket directory in cwd must be written
`./NAME`. Both forms are assertions, not starting points — near-misses
(a subdirectory of a ticket) error rather than re-walking.

Not aliased to `-C`. `-C` (chdir-before-running) MAY be added later as
an orthogonal flag; it composes with `--ticket` with no conflict rule —
`-C` moves you, `--ticket` overrides selection.

---

## 5. Invocation contract

### 5.1 Plugin resolution

- Plugins are external binaries `tix-<name>` discovered on `PATH`,
  exec'd with user-facing args forwarded
  (`Commands::External` in `tix-cli/src/tix/mod.rs:72` →
  `plugin::run`).
- **Builtins always win**: `tix add` runs the builtin even if `tix-add`
  is on `PATH`.
- Ticket subcommands exist both namespaced (`tix ticket setup`) and as
  top-level aliases (`tix setup`) — namespaced for consistency, top-level
  for convenience. Spec text uses the namespaced form; both count as
  builtins for collision purposes.
- Plugins are **top-level commands only** — `tix <name>`, nothing under
  existing subcommand trees.

### 5.2 Host-injected flags

```
tix-<name> --tix-protocol <int> \
           --tix-config <path> \
           [--tix-ticket <path>] \
           --tix-delta <path> \
           [--tix-repo <alias>] [--tix-repo-dir <path>] \
           --tix-log-level <level> \
           --tix-output <json|toml|default> \
           --tix-color <bool> \
           <user args>
```

| Flag | Presence | Meaning |
|------|----------|---------|
| `--tix-protocol` | always | Protocol integer (§5.4). |
| `--tix-config` | always | Path to the global config file (the real file, not a staged copy). |
| `--tix-ticket` | only inside a ticket | Path to the **ticket directory** (parent of `.tix`). Its absence is load-bearing (`tix ticket setup` creates the ticket, so it runs without one); the SDK exposes the ticket document as `Option` plus a "require ticket context" helper. |
| `--tix-delta` | always | Host-created temp file for the outbound delta (§6). Stdout cannot carry it — stdout belongs to the user. No file written ⇒ no changes. |
| `--tix-repo` / `--tix-repo-dir` | when cwd is inside a repo worktree | Alias and path of that worktree (longest-prefix match of cwd against `ticket_root/<alias>`, v2's `detect_current_repo`). Both `Option`. |
| `--tix-log-level` | always | Resolved level — host collapses `-v`/`-q`/`--log-level` via `resolve_log_level()` (`tix-cli/src/tix/mod.rs:35`); plugins MUST NOT reimplement the precedence. |
| `--tix-output` | always | Resolved output format, so `tix foo --json \| jq` works through plugins. |
| `--tix-color` | always | Resolved from TTY detection + `NO_COLOR`, per-process (plugin's stdout may be piped when the host's is not). |

Rules:

- The host resolves its own globals **before** forwarding; it sends
  settled values, never raw flags.
- The entire `--tix-*` prefix is reserved for the host. The SDK strips
  `--tix-*` flags and hands user args through untouched.
- The SDK MUST ignore unknown `--tix-*` flags (makes additions safe with
  no protocol bump; flag presence doubles as capability detection).
- Paths are the real files — plugins can be hand-run against arbitrary
  paths for debugging.

### 5.3 Host pre-scan

`external_subcommand` captures everything after the plugin name raw, so
`tix foo --verbose` leaves `TixParser.verbose` false. The host MUST
pre-scan forwarded args for its own global flags before handing off. The
pre-scan and the SDK's flag parsing MUST be the same code.

**Prerequisite:** `OutputType` (`tix-cli/src/tix/utils.rs:14`) is
per-subcommand today and MUST be hoisted to a global on `TixParser`.

### 5.4 Protocol version

- `--tix-protocol <int>`: monotonic, starts at **1**, independent of
  crate versions.
- The SDK compares the received value against its compiled-in value; on
  mismatch it MUST fail with "built for protocol N, host speaks M —
  rebuild", exiting with the reserved exit code (§6.4).
- Bump **only** for removal, rename, or semantic change of an existing
  flag or document. Never for additions.
- A version → change table MUST be maintained beside the SDK.

### 5.5 Recursive invocation

Plugins MAY shell out to `tix`. Three hazards, each closed by one rule:

- **Lost updates** — closed by fresh-parse-at-apply (§6.2).
- **Fork bombs** — the host sets/increments a `TIX_DEPTH` env var and
  MUST hard-fail past a small cap (~10).
- **Context skew** — nested `tix` re-discovers from cwd, which the plugin
  may have changed. The SDK provides a spawn helper returning a `Command`
  pre-pinned with `--ticket <current ticket path>`; the supported path is
  skew-proof by construction.

### 5.6 Help and enumeration

`tix --help` appends a **Plugins** section after built-in help:

- Scan `PATH` directories for executables matching `tix-*`; deduplicate
  by name, first match on `PATH` wins (shell semantics); strip the
  `tix-` prefix for display.
- For each plugin, optionally exec `tix-<name> print-cli-help` for a
  one-line description. A plugin that doesn't implement it, errors, or
  hangs degrades to the bare name — the listing itself MUST never fail.
- Handshake output is untrusted: strip control characters, cap at one
  line before rendering.
- `print-cli-help` is invoked **bare** — no `--tix-*` flags — so the
  SDK's arg parsing MUST handle it before requiring host flags, and
  SHOULD answer it automatically from a description the plugin registers.
- A plugin's own `--help` is its own; tix does not touch it.
- Zero plugins found (or empty `PATH`) simply omits the section.

---

## 6. Config writes (diff-back)

Diff-back exists **only** because config has a single-writer constraint.
It is not an RPC channel. Everything else a plugin does (worktrees,
syncing, resolution) goes directly through `tix-sdk → tix-engine`
in-process.

### 6.1 Delta format

JSON, written by the SDK's write helper into the `--tix-delta` file:

```json
{"target": "ticket", "ops": [{"set": "myplugin.branch", "value": "main"}]}
```

- `target` — `"global"` or `"ticket"` (exactly two documents; no
  provenance needed).
- `ops` — ordered list; overlapping keys are last-writer-wins.
- Inbound config is TOML (real files; `tomllib` is stdlib ≥3.11);
  outbound delta is JSON (Python has no stdlib TOML writer). Asymmetric
  by design.

**JSON→TOML value mapping** (applied by the SDK on every supported path;
only SDK-less hand-writers think about it):

- string → string, bool → bool, `1` → integer, `1.0` → float (JSON text
  form, preserved by real parsers), arrays/tables recurse.
- Datetime — the one inexpressible type — uses the single tagged form
  `{"$datetime": "2026-07-19T09:00:00Z"}`; the SDK emits it automatically
  for typed datetimes.

### 6.2 Apply semantics (host)

- The host is the **single writer**; it applies the delta after the
  plugin exits.
- Applied against a **fresh parse at apply time**, never the startup
  snapshot — a delta is an intent, so it merges with whatever is on disk
  (nested `tix`, second terminal).
- The host applies ops as path traversals over the format-preserving
  document ("go to `myplugin.branch`, put `main` there"). It MUST NOT
  deserialize plugin tables — a typed round-trip is a data-loss bug
  (drops `[<plugin>]` tables, comments, formatting). The plugin owns its
  section's schema; the host owns the document and treats every section
  as opaque.
- **Validation:** after applying, the host re-deserializes the sections
  it has types for. If a host-owned section (`[engine]`, `[cli]`,
  `[defaults]`, `[ticket]`) no longer parses, the whole delta is rejected
  and nothing is written. Plugin sections pass unvalidated.
- Writes outside the plugin's own section are **allowed, unsupported**:
  applied if revalidation passes; consequences are the user's business.

### 6.3 Failure semantics

- stdout/stderr are inherited, never captured.
- A nonzero plugin exit propagates as tix's exit code, unmodified; the
  delta is discarded (applied only on exit 0).
- A malformed/unparseable delta, or one failing revalidation, is a
  `PluginImplementationError`: rejected wholesale, nothing written,
  reported as a bug in the plugin.

### 6.4 Reserved exit code

Exit code **125** is reserved for protocol mismatch, excluded from the
propagated range, and documented in the contract. (The established
tool-layer-error slot: `docker run`, `timeout(1)`, `git bisect run` skip.
126/127 stay meaningful for exec failures; 128+n for signals.) A plugin
exiting 125 for its own reasons will be misreported as a mismatch — a
known, accepted collision.

---

## 7. `pytix`

- PyO3 bindings, single wheel, `abi3-py311`. Always compiles bindings
  for both crates — no Cargo feature gates, no pip extras (a wheel is
  built once; extras cannot vary compiled content).
- Namespacing: `pytix.*` = engine bindings; `pytix.host` = `tix-sdk`
  bindings (the plugin's handle on the invoking host).
- A Python plugin ships a `console_scripts` entry point named
  `tix-<name>` and lands on `PATH` like any other plugin.
- Deferred until a real Python plugin exists to shape it (last in
  sequencing).

---

## 8. Deferred / open items

1. **v2 → v3 migration** — `.tix/info.toml` → `.tix/ticket.toml`; flat
   v2 `config.toml` → sectioned global config. Deferred until v3 parity.
   Constraint on the present: the discovery predicate must not foreclose
   reading both filenames.
2. **`-C` flag** — possible later, orthogonal to `--ticket` (§4).
3. **`[defaults]` set drift** — §3.4 adopts the v2 set provisionally;
   fields may be added/removed as `tix ticket setup` takes shape.

---

## 9. Implementation sequence

1. **Section types in `tix-engine`** — `EngineConfig` (from today's
   `Engine`), `TicketConfig` with per-repo branches, `Defaults` (§3.4).
   Shapes and (de)serialization only, over paths handed in.
2. **Document handling in `tix-cli`** — locate/parse into the generic
   DOM, typed section accessors, seed reads at `tix ticket setup`, and the ticket
   walk (§4).
3. **`tix-sdk`** — promote (2) out of `tix-cli`; add `--tix-*` flag
   parsing and the protocol check; re-export `tix-engine`.
4. **Host side** — pre-scan + `OutputType` hoist; wire `plugin::run`
   (§5).
5. **Diff-back application** in the host (§6).
6. **`pytix`** bindings and `pytix.host` (§7).
