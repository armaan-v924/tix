"""The `pytix.*` namespace: engine values and operations across the boundary."""

from __future__ import annotations

from pathlib import Path

import pytest

import pytix


def test_repository_config_round_trips_its_fields(tmp_path: Path) -> None:
    """Values handed in come back out with Python types, not opaque handles."""
    config = pytix.RepositoryConfig(
        "https://github.com/owner/repo.git", tmp_path / "code"
    )

    assert config.remote == "https://github.com/owner/repo.git"
    assert config.code_path == tmp_path / "code"
    assert config == pytix.RepositoryConfig(
        "https://github.com/owner/repo.git", tmp_path / "code"
    )
    assert "owner/repo" in repr(config)


def test_resolving_a_non_repository_raises_tix_error(tmp_path: Path) -> None:
    """A git failure arrives as `TixError`, carrying the engine's message."""
    config = pytix.RepositoryConfig(
        "https://example.invalid/x.git", tmp_path / "nothing-here"
    )

    with pytest.raises(pytix.TixError):
        config.resolve("missing")


def test_ticket_config_records_per_worktree_branches() -> None:
    """Worktrees share a branch prefix, not a branch — so each carries its own."""
    config = pytix.TicketConfig(
        "JIRA-123",
        "Fix the login bug",
        {
            "backend": pytix.WorktreeConfig("backend", "feature/JIRA-123-fix-login"),
            "frontend": pytix.WorktreeConfig(
                "frontend", "feature/JIRA-123-fix-login-ui"
            ),
        },
    )

    assert config.key == "JIRA-123"
    assert config.description == "Fix the login bug"
    assert config.worktrees["backend"].branch == "feature/JIRA-123-fix-login"
    assert config.worktrees["frontend"].repo == "frontend"
    assert not hasattr(config, "branch")


def test_ticket_config_defaults_to_no_worktrees() -> None:
    """A freshly created ticket has none yet."""
    assert pytix.TicketConfig("JIRA-456", "Nothing started").worktrees == {}


def test_ticket_config_has_no_document_io() -> None:
    """Engine types do no IO; reading `.tix/ticket.toml` is `pytix.host`'s job."""
    assert not hasattr(pytix.TicketConfig, "load_from")


def test_resolving_a_missing_ticket_directory_raises(tmp_path: Path) -> None:
    """`resolve` promises validity, so a missing workspace must fail."""
    config = pytix.TicketConfig("JIRA-123", "Fix the login bug")

    with pytest.raises(pytix.TixError):
        config.resolve(tmp_path / "no-such-ticket")


def test_worktree_lifecycle(tmp_path: Path, clone: Path) -> None:
    """The full crossing: resolve a clone, create a worktree, resolve a ticket.

    One test rather than several because the steps only exist in sequence — a
    `Worktree` cannot be forged from Python, by design.
    """
    ticket_root = tmp_path / "JIRA-123"
    ticket_root.mkdir()

    repository = pytix.RepositoryConfig(str(clone), clone).resolve("api")
    assert repository.alias == "api"
    assert repository.config.code_path == clone

    worktree = repository.create_worktree(
        "api", "feature/JIRA-123-fix-login", ticket_root / "api"
    )
    assert worktree.name == "api"
    assert worktree.repo_alias == "api"
    assert worktree.branch == "feature/JIRA-123-fix-login"
    assert worktree.path == ticket_root / "api"
    assert worktree.path.is_dir()

    ticket = pytix.TicketConfig(
        "JIRA-123",
        "Fix the login bug",
        {"api": pytix.WorktreeConfig("api", "feature/JIRA-123-fix-login")},
    ).resolve(ticket_root)

    assert ticket.key == "JIRA-123"
    assert ticket.description == "Fix the login bug"
    assert ticket.path == ticket_root
    assert [w.name for w in ticket.worktrees] == ["api"]
    assert not hasattr(ticket, "branch")

    repository.remove_worktree(ticket_root / "api")
    with pytest.raises(pytix.TixError):
        repository.remove_worktree(ticket_root / "api")
