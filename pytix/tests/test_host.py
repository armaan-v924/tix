"""The `pytix.host` namespace: the plugin's handle on the invoking host."""

from __future__ import annotations

import datetime as dt
import json
from pathlib import Path

import pytest
import pytix.host

import pytix

TICKET_DOCUMENT = """\
# a comment the host must never lose
[ticket]
key = "JIRA-123"
description = "Fix the login bug"

[ticket.worktrees.api]
repo = "api"
branch = "feature/JIRA-123-fix-login"

[myplugin]
enabled = true
retries = 3
ratio = 1.5
tags = ["a", "b"]
started = 2026-07-19T09:00:00Z
due = 2026-07-20
"""


@pytest.fixture
def ticket_root(tmp_path: Path) -> Path:
    """A ticket root carrying a document with a plugin table."""
    root = tmp_path / "JIRA-123"
    (root / ".tix").mkdir(parents=True)
    (root / ".tix" / "ticket.toml").write_text(TICKET_DOCUMENT)
    return root


@pytest.fixture
def config_path(tmp_path: Path) -> Path:
    """A global config carrying a plugin table."""
    path = tmp_path / "config.toml"
    path.write_text('[myplugin]\nmode = "fast"\n')
    return path


def context(config_path: Path, *extra: str) -> pytix.host.HostContext:
    """Builds a context the way the host would invoke a plugin."""
    return pytix.host.HostContext.from_args(
        [
            "--tix-protocol",
            str(pytix.host.PROTOCOL),
            "--tix-config",
            str(config_path),
            *extra,
        ]
    )


# --- flag parsing ---


def test_host_flags_are_stripped_and_user_args_survive(config_path: Path) -> None:
    """The `--tix-*` prefix is the host's; everything else is the plugin's."""
    parsed = context(config_path, "--tix-repo", "api", "sync", "--verbose", "file.txt")

    assert parsed.config_path == config_path
    assert parsed.repo == "api"
    assert parsed.user_args == ["sync", "--verbose", "file.txt"]


def test_unknown_host_flags_are_ignored(config_path: Path) -> None:
    """Additions never break older plugins, so no protocol bump is needed."""
    parsed = context(config_path, "--tix-shiny-new-flag", "whatever", "mine")

    assert parsed.user_args == ["mine"]


def test_protocol_mismatch_says_rebuild(config_path: Path) -> None:
    """A mismatch is a rebuild situation, reported as one."""
    with pytest.raises(pytix.TixError, match="rebuild"):
        pytix.host.HostContext.from_args(
            ["--tix-protocol", "999", "--tix-config", str(config_path)]
        )


def test_running_outside_the_host_points_back_at_it() -> None:
    """`--tix-config` rides every host invocation, so its absence is diagnostic."""
    with pytest.raises(pytix.TixError, match="tix plugin"):
        pytix.host.HostContext.from_args(["sync"])


def test_ticket_context_is_optional(config_path: Path, ticket_root: Path) -> None:
    """`tix ticket setup` creates tickets, so it runs without one."""
    without = context(config_path)
    assert without.ticket_root is None
    with pytest.raises(pytix.TixError, match="requires ticket context"):
        without.require_ticket()

    within = context(config_path, "--tix-ticket", str(ticket_root))
    assert within.require_ticket() == ticket_root


# --- document reads ---


def test_config_section_reads_the_plugin_table(config_path: Path) -> None:
    """A plugin reads its own table out of the global config."""
    assert context(config_path).config_section("myplugin") == {"mode": "fast"}


def test_absent_section_is_none_not_empty(config_path: Path) -> None:
    """A first run and an emptied table are different answers."""
    assert context(config_path).config_section("never-configured") is None


def test_ticket_section_maps_toml_types_like_tomllib(
    config_path: Path, ticket_root: Path
) -> None:
    """Values arrive as the types `tomllib` would have produced."""
    section = context(config_path, "--tix-ticket", str(ticket_root)).ticket_section(
        "myplugin"
    )

    assert section == {
        "enabled": True,
        "retries": 3,
        "ratio": 1.5,
        "tags": ["a", "b"],
        "started": dt.datetime(2026, 7, 19, 9, 0, tzinfo=dt.timezone.utc),
        "due": dt.date(2026, 7, 20),
    }


def test_document_preserves_the_source(config_path: Path, ticket_root: Path) -> None:
    """The document layer is format-preserving; the comment must still be there."""
    document = context(config_path, "--tix-ticket", str(ticket_root)).ticket_document()

    assert str(document) == TICKET_DOCUMENT
    assert document.path == ticket_root / ".tix" / "ticket.toml"


def test_ticket_config_bridges_into_the_engine(
    config_path: Path, ticket_root: Path
) -> None:
    """The host does the IO; the engine type describes what came back."""
    config = context(config_path, "--tix-ticket", str(ticket_root)).ticket_config()

    assert isinstance(config, pytix.TicketConfig)
    assert config.key == "JIRA-123"
    assert config.worktrees["api"].branch == "feature/JIRA-123-fix-login"


# --- deltas ---


def test_delta_tags_datetimes_automatically(tmp_path: Path) -> None:
    """JSON cannot express a TOML datetime, and the caller should not have to care."""
    delta = pytix.host.Delta("ticket")
    delta.set("myplugin.branch", "main")
    delta.set("myplugin.retries", 3)
    delta.set("myplugin.ratio", 1.5)
    delta.set("myplugin.enabled", True)
    delta.set("myplugin.tags", ["a", "b"])
    delta.set(
        "myplugin.started", dt.datetime(2026, 7, 19, 9, 0, tzinfo=dt.timezone.utc)
    )
    delta.set("myplugin.due", dt.date(2026, 7, 20))

    wire = json.loads(delta.to_json())

    assert wire["target"] == "ticket"
    assert wire["ops"] == [
        {"set": "myplugin.branch", "value": "main"},
        {"set": "myplugin.retries", "value": 3},
        {"set": "myplugin.ratio", "value": 1.5},
        {"set": "myplugin.enabled", "value": True},
        {"set": "myplugin.tags", "value": ["a", "b"]},
        {
            "set": "myplugin.started",
            "value": {"$datetime": "2026-07-19T09:00:00+00:00"},
        },
        {"set": "myplugin.due", "value": {"$datetime": "2026-07-20"}},
    ]


def test_delta_rejects_values_toml_cannot_hold() -> None:
    """TOML has no null; "absent" is spelled by not setting the key."""
    delta = pytix.host.Delta("ticket")

    with pytest.raises(pytix.TixError, match="None is not representable"):
        delta.set("myplugin.nothing", None)


def test_delta_rejects_unknown_targets() -> None:
    """There are exactly two documents, so a typo is a mistake, not an extension."""
    with pytest.raises(pytix.TixError, match="unknown delta target"):
        pytix.host.Delta("somewhere-else")


def test_write_delta_uses_the_host_supplied_path(
    config_path: Path, tmp_path: Path
) -> None:
    """The host reads the file it named in `--tix-delta` after a clean exit."""
    delta_path = tmp_path / "delta.json"
    delta = pytix.host.Delta("global")
    delta.set("myplugin.mode", "slow")

    context(config_path, "--tix-delta", str(delta_path)).write_delta(delta)

    assert json.loads(delta_path.read_text()) == {
        "target": "global",
        "ops": [{"set": "myplugin.mode", "value": "slow"}],
    }


def test_write_delta_without_a_host_path_points_back_at_tix(config_path: Path) -> None:
    """Hand-running a plugin has no delta channel; say so rather than fail obscurely."""
    delta = pytix.host.Delta("global")
    delta.set("myplugin.mode", "slow")

    with pytest.raises(pytix.TixError, match="--tix-delta"):
        context(config_path).write_delta(delta)


# --- state ---


def test_state_dir_is_created_lazily(config_path: Path, ticket_root: Path) -> None:
    """State is not config: plain files, created only when a plugin asks."""
    parsed = context(config_path, "--tix-ticket", str(ticket_root))
    assert not (ticket_root / ".tix" / "plugins").exists()

    state = parsed.state_dir("myplugin")

    assert state == ticket_root / ".tix" / "plugins" / "myplugin"
    assert state.is_dir()


def test_state_dir_requires_a_ticket(config_path: Path) -> None:
    """A plugin cannot locate a ticket it was not told about."""
    with pytest.raises(pytix.TixError, match="requires ticket context"):
        context(config_path).state_dir("myplugin")
