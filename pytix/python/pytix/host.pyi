"""Type stubs for the `pytix.host` namespace: bindings for `tix-sdk`.

`pytix.host` is a submodule of an extension module — an attribute of its
parent registered into `sys.modules` from Rust — so a checker has nothing to
read but this file. The module tree mirrors the crate graph, and so do the
stubs: engine types in `__init__.pyi`, the host's own in here.

Kept in step with `pytix/src/host/` by hand. Signatures — argument names and
defaults included — come from the `#[pymethods]` blocks and their
`#[pyo3(signature = ...)]` attributes.
"""

from os import PathLike
from pathlib import Path
from typing import Any, Literal, TypeAlias

from pytix import TicketConfig

# See the note in `__init__.pyi`: pyo3 calls `os.fspath` on every `PathBuf`
# argument, so anything path-like is accepted.
_StrPath: TypeAlias = str | PathLike[str]

DeltaTarget: TypeAlias = Literal["global", "ticket"]
"""The two documents a delta can target.

There are exactly two, so a typo is a mistake rather than an extension point.
`Delta` re-checks at runtime and raises `TixError`; this alias is what lets a
type checker catch it first.
"""

PROTOCOL: int
"""The invocation-contract version this build speaks.

Monotonic and independent of the crate version. Bumped only when an existing
flag or document is removed, renamed, or changes meaning — never for
additions, since unknown `--tix-*` flags are ignored by contract.
"""

PROTOCOL_MISMATCH_EXIT: int
"""The exit code reserved for a protocol mismatch.

The established tool-layer-error slot, excluded from the range the host
propagates, so "rebuild this plugin" never reads as "the plugin failed".
"""

def cache_dir(plugin: str) -> Path:
    """The global cache directory for `plugin`, created on call.

    A convenience only. Per-*ticket* state is `HostContext.state_dir`, which
    does need the host to say where the ticket is.

    Raises `TixError` when the platform cache directory cannot be determined,
    or if creation fails.
    """

class HostContext:
    """The settled values the host forwarded, plus the user's own arguments.

    A plugin's first act is to build one of these. It strips every `--tix-*`
    flag out of the argument list, answers the bare `print-cli-help`
    handshake, and checks the protocol — all before the plugin has looked at a
    single argument of its own.
    """

    @staticmethod
    def from_env(description: str | None = None) -> HostContext:
        """Parses `sys.argv[1:]`.

        Pass `description` to answer the `print-cli-help` handshake: when the
        host invokes the plugin with that single argument and nothing else,
        the description is printed and the process exits 0.

        Raises `TixError` on a protocol mismatch, or when `--tix-config` is
        absent — the one flag every host invocation carries, so its absence
        means the binary was run directly rather than through `tix <name>`.
        """

    @staticmethod
    def from_args(args: list[str], description: str | None = None) -> HostContext:
        """`from_env` over an explicit argument list, for tests and for plugins
        that manage their own argv.
        """

    @staticmethod
    def from_env_or_exit(description: str) -> HostContext:
        """`from_env`, turning failures into the contract's process exits
        instead of exceptions — the convenience entry point for a plugin's
        `main`.

        A protocol mismatch exits `PROTOCOL_MISMATCH_EXIT`; anything else
        prints to stderr and exits 1. Both raise `SystemExit`, so `finally`
        blocks and context managers still run.
        """

    @property
    def config_path(self) -> Path:
        """The global config file."""

    @property
    def ticket_root(self) -> Path | None:
        """The ticket directory, or `None` when the host ran outside a ticket.

        The absence is load-bearing rather than exceptional: `tix ticket setup`
        creates tickets, so it necessarily runs without one.
        """

    @property
    def delta_path(self) -> Path | None:
        """The host-created file this plugin's delta should be written to."""

    @property
    def repo(self) -> str | None:
        """The alias of the repository worktree the cwd is inside, if any."""

    @property
    def repo_dir(self) -> Path | None:
        """The path of that worktree."""

    @property
    def log_level(self) -> str | None:
        """The host's resolved log level."""

    @property
    def output(self) -> str | None:
        """The host's resolved output format: `json`, `toml`, or `default`."""

    @property
    def color(self) -> bool | None:
        """The host's resolved color decision for this process."""

    @property
    def user_args(self) -> list[str]:
        """Everything that was not a `--tix-*` flag, in order — the plugin's
        own arguments, untouched. Feed this to `argparse`.
        """

    def require_ticket(self) -> Path:
        """The ticket directory, for plugins that require ticket context.

        Raises `TixError` with a user-facing "run inside a ticket" message
        when the host forwarded none.
        """

    def config_document(self) -> Document:
        """Parses the global config document.

        Raises `TixError` if the file is missing or is not valid TOML.
        """

    def ticket_document(self) -> Document:
        """Parses the ticket document at `<ticket_root>/.tix/ticket.toml`."""

    def config_section(self, name: str) -> Any | None:
        """The section `name` of the global config, or `None` when absent.

        Re-reads the file on every call, deliberately: the host may have
        rewritten it since. Hold the `Document` yourself for a fixed snapshot.
        """

    def ticket_section(self, name: str) -> Any | None:
        """The section `name` of the ticket document, or `None` when absent."""

    def ticket_config(self) -> TicketConfig:
        """The typed `[ticket]` section of the ticket document.

        The bridge between the two namespaces: the host does the IO, the
        engine type describes the result.
        """

    def state_dir(self, plugin: str) -> Path:
        """This plugin's per-ticket state directory,
        `<ticket_root>/.tix/plugins/<plugin>/`, created on call.

        State is not config: caches and derived data of any shape, part of no
        document, no delta, and no protocol.
        """

    def write_delta(self, delta: Delta) -> None:
        """Writes `delta` to the path the host passed in `--tix-delta`.

        Raises `TixError` if the host forwarded no delta path — which happens
        only when the plugin was hand-run rather than invoked through `tix`.
        """

class Document:
    """A parsed tix document — the global config or a ticket's
    `.tix/ticket.toml`.

    Read-only by construction: config has a single writer, the host. A plugin
    that wants a change writes a `Delta` instead.

    Sections come back as plain Python values, mapped exactly as `tomllib`
    maps them — tables to `dict`, arrays to `list`, TOML datetimes to
    `datetime` objects.
    """

    @property
    def path(self) -> Path:
        """The file this document was parsed from."""

    def section(self, name: str) -> Any | None:
        """The top-level section `name`, or `None` when the document has no
        such section.

        Absent and empty are different answers: a plugin's first run sees
        `None`, and a plugin whose table exists but is empty sees `{}`.
        """

    def to_dict(self) -> dict[str, Any]:
        """The whole document as a nested `dict`."""

class Delta:
    """A config delta: the ordered set of changes a plugin asks the host to
    make.

    Config has one writer — the host — so a plugin never edits a document in
    place. It records what it wants at dotted key paths, writes the result to
    the path the host passed in `--tix-delta`, and exits; the host applies the
    ops against a fresh parse afterwards.

    Ops are applied in order and overlapping keys are last-writer-wins.
    Values are ordinary Python objects; `datetime`, `date`, and `time`
    instances are tagged for the wire automatically.
    """

    def __init__(self, target: DeltaTarget) -> None:
        """An empty delta against `target`, either `"global"` (the global
        config) or `"ticket"` (the ticket document).

        Raises `TixError` for any other target — the runtime check the
        `DeltaTarget` annotation lets a checker anticipate.
        """

    @property
    def target(self) -> DeltaTarget:
        """The document this delta targets."""

    def ops(self) -> list[dict[str, Any]]:
        """The ops recorded so far, in order, as `{"set": path, "value": value}`
        dicts — the wire form, for tests and debugging.
        """

    def set(self, path: str, value: object) -> None:
        """Records `value` at the dotted key `path`, e.g. `myplugin.branch`.

        Raises `TixError` if `value` has no TOML representation — `None` most
        commonly, since TOML has no null and "absent" is spelled by not
        setting the key at all.
        """

    def to_json(self) -> str:
        """Serializes the delta to its JSON wire form."""

    def write_to(self, path: _StrPath) -> None:
        """Writes the delta to `path`.

        Plugins normally call `HostContext.write_delta` instead; this is for
        hand-running a plugin against an arbitrary file while debugging.
        """
