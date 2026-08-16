# tix host ↔ plugin protocol

The `--tix-protocol` integer is the **only** compatibility boundary between
a `tix` host and a separately-compiled plugin. Crate versions carry no
compatibility semantics (`design/spec.md` §2.3).

Rules (spec §5.4):

- Monotonic, starting at 1. The SDK's compiled-in value is
  `tix_sdk::host::PROTOCOL`.
- Bump **only** for removal, rename, or semantic change of an existing flag
  or document. **Never for additions** — the SDK ignores unknown `--tix-*`
  flags, so additions are safe by construction, and flag presence doubles as
  capability detection.
- On mismatch the SDK fails with "built for protocol N, host speaks M —
  rebuild" and exits **125** (`host::PROTOCOL_MISMATCH_EXIT`), which the
  host excludes from the propagated exit-code range.

## Version → change table

| Protocol | Introduced | Changes |
|----------|------------|---------|
| 1 | v3.0 | Initial contract: `--tix-protocol`, `--tix-config`, `--tix-ticket` (only inside a ticket), `--tix-delta`, `--tix-repo` / `--tix-repo-dir` (only inside a repo worktree), `--tix-log-level`, `--tix-output`, `--tix-color`; JSON delta diff-back applied on exit 0; `TIX_DEPTH` fork-bomb cap; exit 125 reserved. |
