# tix plugin specification

The authoritative reference for writing a tix plugin. The design rationale
lives in `design/spec.md` §5–6; this document is the contract as shipped.

A tix plugin is an ordinary executable named `tix-<name>` on `PATH`. That is
the entire registration story: `tix foo` execs the first `tix-foo` it finds,
git/cargo-style. Builtins always win — a `tix-add` binary never shadows
`tix add` — and plugins are top-level commands only (`tix <name>`, nothing
under existing subcommand trees). A Python plugin ships a `console_scripts`
entry point named `tix-<name>` and lands on `PATH` like any Rust plugin.

Both languages have an SDK that implements this document for you: `tix-sdk`
in Rust ([API docs](https://tix.armaanv.dev/crates/tix_sdk/)), the
`pytix.host` namespace of the `pytix` wheel in Python. Every snippet below is
given in both.

**Safety policy:** tix makes no safety guarantees about third-party plugins.
A plugin is an arbitrary program running as you; install at your own risk.

---

## 1. The exec contract

When the user runs `tix <name> <args…>`, the host execs:

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
| `--tix-protocol` | always | The protocol integer (§2). |
| `--tix-config` | always | Path to the global config file — the real file, not a staged copy. |
| `--tix-ticket` | only inside a ticket | Path to the **ticket directory** (the parent of `.tix/`). Its absence is meaningful: commands that create tickets run without one. |
| `--tix-delta` | always | Host-created temp file for your outbound config delta (§3). Stdout cannot carry it — stdout is yours, for the user. Writing no file means changing nothing. |
| `--tix-repo` / `--tix-repo-dir` | when cwd is inside a repo worktree | Alias and path of that worktree (longest-prefix match of the cwd against the ticket's worktrees). |
| `--tix-log-level` | always | The host's **resolved** level — it already collapsed `-v`/`-q`/`--log-level`. Do not reimplement that precedence. |
| `--tix-output` | always | Resolved output format, so `tix foo --json \| jq` works through you. |
| `--tix-color` | always | Resolved from TTY detection + `NO_COLOR`, per-process. |

Rules:

- The **entire `--tix-*` prefix is reserved** for the host. Everything else
  in argv is the user's args, forwarded in order and untouched.
- **Ignore unknown `--tix-*` flags.** Additions to the contract never bump
  the protocol; checking whether a flag arrived is capability detection.
- Paths are real files. You can hand-run a plugin against arbitrary paths
  for debugging: `tix-foo --tix-protocol 1 --tix-config ./test.toml …`
- stdout/stderr/stdin are inherited — you own the terminal. Prompt, color,
  and stream as you please (honoring `--tix-color`).
- Exit codes propagate to the user unmodified, except **125** (§2).
- `TIX_DEPTH`: the host sets and increments this env var on every dispatch
  and hard-fails past 10 — the fork-bomb guard for plugins that shell back
  out to `tix`. Nothing for you to do unless you clear the environment.

The Rust SDK does all of this in one call:

```rust
let host = tix_sdk::host::HostContext::from_env_or_exit("one-line description");
// host.config_path, host.ticket_root, host.delta_path, host.user_args, …
```

(`tix-sdk/examples/toy-plugin.rs` is a complete working plugin.)

## 2. Protocol version

`--tix-protocol` is a monotonic integer, starting at 1, independent of any
crate version — it is the **only** compatibility boundary between a host and
a separately-compiled plugin. The version → change table lives in
[`tix-sdk/PROTOCOL.md`](tix-sdk/PROTOCOL.md).

- It bumps only for removal, rename, or semantic change — never additions.
- On mismatch, exit with code **125** and a "built for protocol N, host
  speaks M — rebuild" message (the SDK does both for you). The host reserves
  125 for exactly this and reports a versioning problem rather than a plugin
  crash. A plugin exiting 125 for its own reasons will be misreported — a
  known, accepted collision.

## 3. Config access

Two TOML documents exist — the global config and the ticket document
(`<ticket>/.tix/ticket.toml`). Each is a set of sections, one table per
consumer; your plugin owns the `[<name>]` table in either document, and its
schema is entirely yours: tix never deserializes it.

**Reads** go through the SDK's document layer — parse generically, extract
your section by type:

```rust
let doc = tix_sdk::document::TixDocument::load(&host.config_path)?;
let mine: MyConfig = doc.section_or_default("myplugin")?;
```

```python
context = pytix.host.HostContext.from_env_or_exit("what my plugin does")
mine = context.config_section("myplugin") or {}
```

A missing section is normal (your first run has no table yet) — that is why
`section()` returns an `Option` in Rust and `None` in Python, and why
`section_or_default()` exists.

Python gets plain `dict`s, typed exactly as `tomllib` would have typed them:
tables are `dict`, arrays are `list`, and TOML datetimes are `datetime`
objects.

**Writes** never touch the files directly. The host is the single writer;
you emit a *delta* into the `--tix-delta` file and the host applies it after
you exit **0**:

```json
{"target": "ticket", "ops": [{"set": "myplugin.branch", "value": "main"}]}
```

```rust
tix_sdk::delta::Delta::new(DeltaTarget::Ticket)
    .set("myplugin.branch", "main")?
    .write_to(host.delta_path.as_deref().unwrap())?;
```

```python
delta = pytix.host.Delta("ticket")
delta.set("myplugin.branch", "main")
context.write_delta(delta)
```

- `target` is `"global"` or `"ticket"`; ops are ordered, overlapping keys
  last-writer-wins.
- The delta is JSON; values map to TOML by their JSON text form (`1` is an
  integer, `1.0` a float). Datetime — the one type JSON lacks — uses the
  tagged form `{"$datetime": "2026-07-19T09:00:00Z"}`. `pytix` applies the
  tag for you: pass a `datetime`, `date`, or `time` and it goes out tagged.
- The host applies against a **fresh parse** of the document, so your delta
  merges with anything written while you ran. It then revalidates the
  sections it owns (`[engine]`, `[cli]`, `[defaults]`, `[ticket]`): a delta
  that breaks one is rejected wholesale as a bug in your plugin and nothing
  is written. Your own section passes unvalidated.
- Writes outside your own section are allowed but unsupported: applied if
  revalidation passes, and what happens next is between you and the user.
- Nonzero exit, no file, or an empty file ⇒ nothing is written.

Diff-back is **not** an RPC channel. Everything else you do — creating
worktrees, syncing, resolving tickets — goes directly through
`tix-sdk → tix-engine` in your own process; git operations carry no
single-writer constraint.

## 4. Plugin state vs. plugin config

Two different things:

- **Config** — human-editable settings. Your `[<name>]` table, read via
  section accessors, written via diff-back (§3).
- **State** — caches and derived data of any shape or size. Plain files in
  a directory; no document, no delta, no protocol:

```rust
let dir = tix_sdk::state::ticket_state_dir(host.require_ticket()?, "myplugin")?;
let cache = tix_sdk::state::cache_dir("myplugin")?;
```

```python
directory = context.state_dir("myplugin")
cache = pytix.host.cache_dir("myplugin")
```

Directories are created lazily, on first use. Per-ticket state lives at
`<ticket>/.tix/plugins/<name>/`; the global cache location is a convenience
wrapper over the platform cache dir.

## 5. Nested `tix` invocations

You may shell out to `tix`. Know the hazards:

- The nested host re-discovers the ticket **from cwd**, which you may have
  changed. If you mean "the same ticket", use the pinned spawn helper:

  ```rust
  let mut cmd = tix_sdk::spawn::tix_command(host.require_ticket()?);
  cmd.args(["ticket", "info"]);
  ```

  There is no Python wrapper for this — the pin is just an argument, and
  spelling it out is clearer than a binding that hides it:

  ```python
  subprocess.run(["tix", "--ticket", context.require_ticket(), "ticket", "info"])
  ```

- `TIX_DEPTH` caps recursion at 10 (§1).
- Concurrent config writes are safe: deltas apply against fresh parses under
  the write lock, degrading to last-writer-wins at key granularity.

## 6. Help integration

- Your `--help` is your own; tix never touches it.
- `tix --help` lists installed plugins by scanning `PATH` for `tix-*`. To
  show a description next to your name, answer the handshake:
  `tix-<name> print-cli-help` → print **one line** to stdout, exit 0.
  - It is invoked **bare** — no `--tix-*` flags — so handle it before
    requiring host flags. The SDK answers it automatically from the
    description you pass to `HostContext::from_env_or_exit`.
  - Not implementing it is fine: you appear by name. A handshake that
    errors or hangs is cut off (the listing itself never fails), and its
    output is sanitized — control characters stripped, one line, capped.
