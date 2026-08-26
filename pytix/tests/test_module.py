"""The module tree: two namespaces, one extension, no optional pieces."""

from __future__ import annotations

import pytix.host

import pytix


def test_host_is_importable_as_a_submodule() -> None:
    """`import pytix.host` works, not merely attribute access on `pytix`.

    A submodule of an extension module is an attribute by default, so this is
    the assertion that the `sys.modules` registration is in place — and the
    spec's promise that both namespaces ship in every wheel.
    """
    from pytix import host

    assert host is pytix.host


def test_classes_report_their_namespace() -> None:
    """Engine types live in `pytix`, host types in `pytix.host`."""
    assert pytix.RepositoryConfig.__module__ == "pytix"
    assert pytix.Ticket.__module__ == "pytix"
    assert pytix.host.HostContext.__module__ == "pytix.host"
    assert pytix.host.Delta.__module__ == "pytix.host"


def test_error_is_a_single_exception_type() -> None:
    """Engine and SDK failures both surface as `pytix.TixError`."""
    assert issubclass(pytix.TixError, Exception)
    assert pytix.TixError.__module__ == "pytix"
