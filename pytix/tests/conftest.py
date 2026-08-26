"""Shared fixtures for the pytix binding tests.

The tests here exercise the *boundary*, not the engine: that values survive the
crossing with the right Python types, that Rust failures arrive as `TixError`,
and that the two namespaces are wired the way the spec says. Engine and SDK
behaviour is already covered by the Rust suites and is not re-tested.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest


def git(*args: str, cwd: Path) -> None:
    """Runs a git command, failing the test loudly if git does."""
    subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True)


@pytest.fixture
def remote(tmp_path: Path) -> Path:
    """A bare repository with one commit on `main`, usable as an origin.

    The engine syncs before creating a worktree, so a repository the tests can
    fetch from is the minimum setup for any worktree test.
    """
    bare = tmp_path / "remote.git"
    bare.mkdir()
    git("init", "--bare", "--initial-branch=main", ".", cwd=bare)

    seed = tmp_path / "seed"
    seed.mkdir()
    git("init", "--initial-branch=main", ".", cwd=seed)
    git("config", "user.email", "tests@example.invalid", cwd=seed)
    git("config", "user.name", "pytix tests", cwd=seed)
    (seed / "README.md").write_text("seed\n")
    git("add", "README.md", cwd=seed)
    git("commit", "-m", "initial commit", cwd=seed)
    git("remote", "add", "origin", str(bare), cwd=seed)
    git("push", "origin", "main", cwd=seed)
    return bare


@pytest.fixture
def clone(tmp_path: Path, remote: Path) -> Path:
    """A working clone of `remote`, checked out on `main`."""
    code = tmp_path / "code"
    git("clone", str(remote), str(code), cwd=tmp_path)
    return code
