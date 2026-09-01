"""Type stubs for the `pytix` namespace: bindings for `tix-engine`.

Hand-written, because the surface they describe is compiled: there is no
Python source for a checker to infer from, and `from .pytix import *` in the
shim tells it nothing at all. The `pytix.host` namespace is stubbed separately
in `host.pyi`, mirroring the module tree.

Kept in step with `pytix/src/engine/` by hand. Signatures — argument names and
defaults included — come from the `#[pymethods]` blocks and their
`#[pyo3(signature = ...)]` attributes.
"""

from os import PathLike
from pathlib import Path
from typing import TypeAlias

# What pyo3 accepts wherever a `PathBuf` argument is declared: it calls
# `os.fspath`, so anything path-like will do. Private to the stub, since the
# runtime module exports no such name.
_StrPath: TypeAlias = str | PathLike[str]

class TixError(Exception):
    """Raised by every failing `pytix` operation, engine or host.

    Rust splits its failures between `tix_engine::TixError` and
    `tix_sdk::SdkError`; Python has no such layering to preserve, so both
    collapse here carrying the Rust `Display` message.
    """

class RepositoryConfig:
    """A source repository registered in the global config: a remote and the
    local path its clone lives at.

    The three resolution methods differ only in what they do when the clone is
    missing: `resolve` fails, `clone_remote` always clones, and `ensure`
    clones only as a fallback.
    """

    def __init__(self, remote: str, code_path: _StrPath) -> None:
        """Registers `remote` as living at `code_path`.

        Neither is touched here — nothing is read, cloned, or validated until
        a resolution method runs.
        """

    @property
    def remote(self) -> str:
        """The remote URL of the repository."""

    @property
    def code_path(self) -> Path:
        """The local path the clone lives at."""

    def resolve(self, alias: str) -> Repository:
        """Opens the already-cloned repository at `code_path` under `alias`.

        Raises `TixError` if the path is not a git repository.
        """

    def clone_remote(self, alias: str) -> Repository:
        """Clones `remote` into `code_path` and opens the result under `alias`."""

    def ensure(self, alias: str) -> Repository:
        """Opens the clone if it exists, cloning it first if it does not."""

class Repository:
    """A live git repository: an open clone, ready for worktree and sync
    operations.

    Construct one through a `RepositoryConfig` resolution method.
    """

    @property
    def alias(self) -> str:
        """The alias this repository is registered under."""

    @property
    def config(self) -> RepositoryConfig:
        """The config this repository was resolved from."""

    def create_worktree(
        self,
        name: str,
        branch: str,
        path: _StrPath,
        force: bool = False,
    ) -> Worktree:
        """Creates a worktree directory named `name` at `path`, checked out on
        `branch`, creating the branch at the synced head if it does not exist.

        `path` is the full, already-resolved directory — the engine derives no
        paths of its own. Syncs first; pass `force=True` to discard local
        changes before syncing.
        """

    def remove_worktree(self, path: _StrPath, force: bool = False) -> None:
        """Prunes the worktree recorded at `path` from this repository.

        Pass `force=True` to remove one that is dirty or structurally broken.
        """

    def sync(self, force: bool = False) -> None:
        """Fetches and fast-forwards `main`. See `sync_base`."""

    def sync_base(self, branch: str, force: bool = False) -> None:
        """Fetches and fast-forwards `branch` from `origin`.

        Pass `force=True` to discard local changes and reset to the remote
        state.
        """

class TicketConfig:
    """The `[ticket]` section of a ticket document: the ticket's identity plus
    one entry per worktree.

    Engine types do no IO — reading and writing the document is `pytix.host`'s
    job, and this type only describes the shape once it has been read. There
    is likewise no single `branch`: worktrees in one ticket share a branch
    *prefix*, not a branch, so each entry carries its own.
    """

    def __init__(
        self,
        key: str,
        description: str,
        worktrees: dict[str, WorktreeConfig] | None = None,
    ) -> None:
        """Describes a ticket `key` with `description` and, optionally, its
        recorded worktrees keyed by directory name.
        """

    @property
    def key(self) -> str:
        """The ticket's unique key, e.g. `JIRA-123`."""

    @property
    def description(self) -> str:
        """The human-readable description of the ticket."""

    @property
    def worktrees(self) -> dict[str, WorktreeConfig]:
        """The recorded worktrees, keyed by directory name under the ticket root."""

    def resolve(self, path: _StrPath) -> Ticket:
        """Validates this config against the ticket workspace at `path` and
        returns a live `Ticket`.

        `path` is the already-resolved ticket directory (the parent of
        `.tix/`). This is the only way to obtain a `Ticket`, and it is a
        *validation*, not a document read.
        """

class Ticket:
    """A live, validated ticket: its directory existed and every recorded
    worktree was present on disk at the moment it was resolved.

    Resolve per operation, use, and discard — holding one across arbitrary
    time says nothing about the state of disk now.
    """

    @property
    def key(self) -> str:
        """The ticket's unique key, e.g. `JIRA-123`."""

    @property
    def description(self) -> str:
        """The human-readable description of the ticket."""

    @property
    def path(self) -> Path:
        """The ticket workspace directory."""

    @property
    def worktrees(self) -> list[Worktree]:
        """The verified live worktrees, in worktree-name order."""

class WorktreeConfig:
    """One worktree's entry in a ticket document: which repository it belongs
    to and which branch it has checked out.

    Frozen because a config value that has already been handed to the engine
    must not mutate underneath it — build a new one instead.
    """

    def __init__(self, repo: str, branch: str) -> None:
        """Records a worktree of `repo` sitting on `branch`."""

    @property
    def repo(self) -> str:
        """The alias of the repository this worktree belongs to."""

    @property
    def branch(self) -> str:
        """The branch checked out in this worktree."""

class Worktree:
    """A live git worktree: one that existed on disk at the moment it was
    produced, by resolving a ticket or by creating it.

    There is deliberately no constructor — a `Worktree` is evidence, and
    Python code must not be able to forge it.
    """

    @property
    def name(self) -> str:
        """The worktree directory name under the ticket root."""

    @property
    def repo_alias(self) -> str:
        """The alias of the repository this worktree belongs to."""

    @property
    def path(self) -> Path:
        """The full path to the worktree directory."""

    @property
    def branch(self) -> str:
        """The branch checked out in this worktree."""
