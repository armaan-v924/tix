"""The stubs against the extension they describe.

`py.typed` and the `.pyi` files are hand-written, because a compiled module
offers a checker nothing to infer from. That makes them the one part of this
package that can be wrong without anything failing: a stale stub type-checks
code that raises at runtime, which is worse than shipping no types at all.

So the compiled module is the source of truth and these tests are the drift
guard. They read the stubs as text — `ast`, never `import` — because a `.pyi`
is not importable and because parsing is what a type checker does too.

Signatures come from pyo3's `__text_signature__`, which is generated from the
`#[pyo3(signature = ...)]` attributes, so argument names and defaults are
compared as declared in Rust rather than as remembered here.
"""

from __future__ import annotations

import ast
import inspect
from pathlib import Path
from typing import Any

import pytest
import pytix.host

import pytix

#: The two namespaces, each with the stub that describes it. `pytix.host` is a
#: submodule registered from Rust rather than a file, so its stub sits beside
#: the package's own instead of under it.
STUBS = {
    "pytix": "__init__.pyi",
    "pytix.host": "host.pyi",
}

#: Names bound in `pytix` that are not API. `pytix` is the inner extension
#: module the generated shim leaves behind as a side effect of
#: `from .pytix import *`; `host` is the submodule, stubbed as its own file.
NOT_API = {"pytix", "host"}


def stub_path(module_name: str) -> Path:
    """The `.pyi` describing `module_name`, beside the installed package."""
    return Path(pytix.__file__).parent / STUBS[module_name]


def parse_stub(module_name: str) -> tuple[dict[str, ast.stmt], dict[str, set[str]]]:
    """The stub's top-level names and each class's public members.

    Returns the top level as name → node so a test can ask what *kind* of
    declaration a name got, and the classes as plain name sets, since that is
    all the membership comparison needs.
    """
    tree = ast.parse(stub_path(module_name).read_text())
    top: dict[str, ast.stmt] = {}
    classes: dict[str, set[str]] = {}
    for node in tree.body:
        match node:
            case ast.FunctionDef(name=name) | ast.ClassDef(name=name):
                top[name] = node
            case ast.AnnAssign(target=ast.Name(id=name)):
                top[name] = node
            case _:
                continue
        if isinstance(node, ast.ClassDef):
            classes[name] = {
                member.name
                for member in node.body
                if isinstance(member, ast.FunctionDef)
                and not member.name.startswith("_")
            }
    return top, classes


def stub_only_declarations(top: dict[str, ast.stmt]) -> set[str]:
    """The names that describe the API rather than belonging to it.

    Two spellings: a `TypeAlias` assignment, and a class deriving from
    `TypedDict` or `Protocol`. Both are things a checker reads and the
    interpreter never sees, so neither can be matched against `dir()`.
    """
    stub_only = set()
    for name, node in top.items():
        match node:
            case ast.AnnAssign(annotation=ast.Name(id="TypeAlias")):
                stub_only.add(name)
            case ast.ClassDef(bases=bases) if any(
                isinstance(base, ast.Name) and base.id in {"TypedDict", "Protocol"}
                for base in bases
            ):
                stub_only.add(name)
    return stub_only


def public_names(obj: Any) -> set[str]:
    """The public attributes of a module or class, dunders excluded."""
    return {name for name in dir(obj) if not name.startswith("_")}


def runtime_classes(module: Any) -> dict[str, type]:
    """The binding classes a namespace exports, exceptions excluded.

    `TixError` is created by `pyo3::create_exception!` rather than as a
    `#[pyclass]`; it has no members of its own and the stub declares it as a
    bare `Exception` subclass, so there is nothing to compare member-wise.
    """
    return {
        name: attribute
        for name in public_names(module) - NOT_API
        if isinstance(attribute := getattr(module, name), type)
        and not issubclass(attribute, BaseException)
    }


@pytest.mark.parametrize("module_name", STUBS)
def test_stub_declares_every_exported_name(module_name: str) -> None:
    """Every public name in the namespace is declared in its stub.

    The direction that catches an addition: a new `#[pymodule_export]` that
    nobody stubbed is invisible to a type checker, so callers get an
    attribute error from the checker on code that runs fine.
    """
    module = {"pytix": pytix, "pytix.host": pytix.host}[module_name]
    top, _ = parse_stub(module_name)
    undeclared = public_names(module) - NOT_API - set(top)
    assert not undeclared, f"{module_name} exports {sorted(undeclared)}, absent from the stub"


@pytest.mark.parametrize("module_name", STUBS)
def test_stub_declares_nothing_the_module_lacks(module_name: str) -> None:
    """The stub promises nothing the namespace does not export.

    The direction that catches a removal or a typo, and the one that actually
    breaks callers: a stub naming something absent type-checks an
    `AttributeError` into existence.

    Type declarations are exempt, because they describe the surface rather
    than being part of it: `DeltaTarget` names the `Literal` that
    `Delta(target)` accepts, and `DeltaOp` the shape `Delta.ops()` returns.
    Neither has a runtime counterpart to compare against.
    """
    module = {"pytix": pytix, "pytix.host": pytix.host}[module_name]
    top, _ = parse_stub(module_name)
    phantom = set(top) - stub_only_declarations(top) - public_names(module)
    assert not phantom, f"the {module_name} stub declares {sorted(phantom)}, which it does not export"


@pytest.mark.parametrize("module_name", STUBS)
def test_stubbed_classes_have_the_members_they_claim(module_name: str) -> None:
    """Each stubbed class matches its `#[pymethods]` block in both directions.

    Properties count as members here: a `#[getter]` is spelled `@property` in
    the stub, and either kind going missing is the same drift.
    """
    module = {"pytix": pytix, "pytix.host": pytix.host}[module_name]
    _, stubbed = parse_stub(module_name)
    drift: dict[str, dict[str, list[str]]] = {}
    for name, cls in runtime_classes(module).items():
        declared = stubbed.get(name, set())
        actual = public_names(cls) - public_names(object)
        difference = {
            "missing from stub": sorted(actual - declared),
            "absent at runtime": sorted(declared - actual),
        }
        if any(difference.values()):
            drift[name] = difference
    assert not drift, f"{module_name} class members drifted: {drift}"


@pytest.mark.parametrize("module_name", STUBS)
def test_stubbed_signatures_match_the_pyo3_signatures(module_name: str) -> None:
    """Argument names and defaults match what pyo3 generated from Rust.

    A stub that renames an argument silently breaks every keyword call, and
    one that invents a default type-checks a `TypeError`. Comparison is on
    names and defaults only — the stub carries annotations that
    `__text_signature__` cannot, and `self`/`cls` are dropped because pyo3
    spells the receiver `$self` while Python spells it `self`.
    """
    module = {"pytix": pytix, "pytix.host": pytix.host}[module_name]
    mismatches: list[str] = []
    for qualname, runtime, stubbed in stub_signature_pairs(module, module_name):
        if runtime != stubbed:
            mismatches.append(f"{qualname}: Rust says {runtime}, stub says {stubbed}")
    assert not mismatches, "\n".join(mismatches)


def stub_signature_pairs(
    module: Any, module_name: str
) -> list[tuple[str, list[str], list[str]]]:
    """Every callable's argument list, as pyo3 declares it and as the stub does.

    Skips anything pyo3 gave no `__text_signature__` — getters have none, and
    neither does a class without a constructor, which is exactly the set with
    no signature to disagree about.
    """
    tree = ast.parse(stub_path(module_name).read_text())
    pairs: list[tuple[str, list[str], list[str]]] = []

    def render(node: ast.FunctionDef) -> list[str]:
        """The stub's argument list as `name` / `name=default` strings."""
        arguments = node.args.posonlyargs + node.args.args
        padding = [None] * (len(arguments) - len(node.args.defaults))
        rendered = []
        for argument, default in zip(arguments, [*padding, *node.args.defaults]):
            if argument.arg in {"self", "cls"}:
                continue
            if default is None:
                rendered.append(argument.arg)
            else:
                rendered.append(f"{argument.arg}={ast.unparse(default)}")
        return rendered

    def runtime_arguments(target: Any) -> list[str] | None:
        """`target`'s pyo3 argument list, or `None` when it declares none."""
        if not getattr(target, "__text_signature__", None):
            return None
        rendered = []
        for parameter in inspect.signature(target).parameters.values():
            if parameter.name in {"self", "cls"}:
                continue
            if parameter.default is inspect.Parameter.empty:
                rendered.append(parameter.name)
            else:
                rendered.append(f"{parameter.name}={parameter.default!r}")
        return rendered

    for node in tree.body:
        if isinstance(node, ast.FunctionDef):
            target = getattr(module, node.name, None)
            if target is not None and (actual := runtime_arguments(target)) is not None:
                pairs.append((node.name, actual, render(node)))
        elif isinstance(node, ast.ClassDef):
            cls = getattr(module, node.name, None)
            if cls is None or issubclass(cls, BaseException):
                continue
            for member in node.body:
                if not isinstance(member, ast.FunctionDef):
                    continue
                # `__init__` in the stub is pyo3's `#[new]`, whose signature
                # pyo3 puts on the class itself.
                target = cls if member.name == "__init__" else getattr(cls, member.name, None)
                if target is None:
                    continue
                if (actual := runtime_arguments(target)) is not None:
                    pairs.append((f"{node.name}.{member.name}", actual, render(member)))
    return pairs


def test_the_typing_marker_ships_beside_the_extension() -> None:
    """`py.typed` is installed, not merely present in the source tree.

    Without it a checker ignores the stubs entirely for an installed package,
    so this is the difference between shipping types and shipping files.
    """
    assert (Path(pytix.__file__).parent / "py.typed").is_file()
