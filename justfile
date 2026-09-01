# lists available just recipes
default:
    @just --list

# ── dev ───────────────────────────────────────────────────────────────────────

# run all checks and tests (local pre-flight before pushing)
ci: check test
    @echo "all good"

# upgrade all lockfiles and commit
upgrade: upgrade-rust upgrade-python
    #!/usr/bin/env bash
    set -euo pipefail
    git add Cargo.lock pytix/uv.lock
    git commit -m "chore: upgrade lockfiles"

# upgrade the rust lockfile
upgrade-rust:
    cargo update --workspace

# The uv project is pytix, not the repo root — there is no root
# pyproject.toml, so uv run from the root fails outright.

# upgrade the python lockfile
[working-directory("pytix")]
upgrade-python:
    uv sync --upgrade

# ── build ─────────────────────────────────────────────────────────────────────

# build the CLI binary for a given target triple
build-cli target:
    cargo build --release -p tix-cli --target {{ target }}

# abi3-py311 makes this one wheel per *platform*, not per Python version:
# `pytix-<version>-cp311-abi3-<platform>.whl` installs on any Python >= 3.11.
# uvx rather than the pytix dev group, so the wheel build does not first
# install the extension it is about to build.

# build the pytix wheel for a given target triple
build-wheel target:
    uvx --from 'maturin>=1.13.3,<2' maturin build --release --strip --target {{ target }} --out dist -m pytix/Cargo.toml

# build rustdoc
build-docs:
    cargo doc --no-deps

# ── docs ──────────────────────────────────────────────────────────────────────

# regenerate the CLI reference and man pages from the clap definition
docs-cli:
    cargo run -q -p xtask

# install the docs site's node dependencies
docs-deps:
    npm --prefix docs ci

# serve the docs site locally with live reload
docs-serve: docs-cli
    npm --prefix docs run dev

# build the documentation site into docs/dist, rustdoc included
#
# Two generators, one output tree: Astro renders the site, and `cargo doc`
# output is grafted on at /crates afterwards so the API reference is served
# from the same origin as everything linking to it.
docs-build: docs-cli build-docs
    #!/usr/bin/env bash
    set -euo pipefail
    npm --prefix docs run build
    rm -rf docs/dist/crates
    mkdir -p docs/dist/crates
    # `/.` rather than `/*`: the glob would miss rustdoc's dotfiles.
    cp -R target/doc/. docs/dist/crates/
    # cargo doc emits per-crate index pages but no root, so /crates/ alone
    # would 404.
    cp docs/crates-index.html docs/dist/crates/index.html
    echo "site built at docs/dist"

# import and exercise a built wheel from ./dist in a throwaway venv
#
# The wheels job builds an artifact it never loads, so a wheel that links
# against the build machine's libraries — or that the platform's loader
# rejects outright — ships green. Installing it away from the source tree is
# the point: `import pytix` here can only resolve to the wheel, because the
# repo has no importable `pytix` package of its own.
smoke-wheel:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    wheels=(dist/*.whl)
    if [[ ${#wheels[@]} -ne 1 ]]; then
        echo "error: expected exactly one wheel in dist/, found ${#wheels[@]}"
        exit 1
    fi
    venv=$(mktemp -d)/venv
    uv venv "$venv" --python 3.11
    uv pip install --python "$venv" "${wheels[0]}" pytest
    python="$venv/bin/python"
    [[ -x "$python" ]] || python="$venv/Scripts/python.exe"
    # Fails loudly and early on a loader-level rejection, where a bare pytest
    # run would bury it in a collection error.
    "$python" -c 'import pytix, pytix.host; print("loaded", pytix.__file__)'
    "$python" -m pytest pytix/tests -q

# ── test ──────────────────────────────────────────────────────────────────────

# run all tests
test: test-rust test-python

# run rust unit and integration tests
test-rust:
    cargo test --workspace

# run python unit tests
[working-directory("pytix")]
test-python:
    uv sync --dev && uv run pytest tests

# ── lint ──────────────────────────────────────────────────────────────────────

# run all lint and formatting checks
lint: lint-rust lint-python

# run rust formatting and clippy
lint-rust: check-rust-format check-rust-clippy

# run python lint and formatting checks (ruff)
lint-python: check-python-lint

# verify rust is formatted
check-rust-format:
    cargo fmt --check

# verify rust code passes clippy
check-rust-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# verify python code in ./pytix is formatted and linted
check-python-lint:
    uvx ruff check pytix

# ── checks ────────────────────────────────────────────────────────────────────

# run all non-lint checks (CI entrypoint)
check: check-license check-versions check-docs lint
    @echo "all checks passed"

# verify the generated CLI reference and man pages match the CLI
check-docs:
    #!/usr/bin/env bash
    set -euo pipefail
    generated="docs/man docs/src/content/docs/reference docs/src/data"
    just docs-cli
    # --porcelain rather than `git diff`: a newly added command shows up as an
    # untracked file, which a diff against the index would not see at all.
    drift=$(git status --porcelain -- $generated)
    if [[ -n "$drift" ]]; then
        echo "error: generated docs are stale — run \`just docs-cli\` and commit the result"
        echo "$drift"
        exit 1
    fi
    echo "generated docs match the CLI (ok)"

# verify LICENSE copyright year is current
check-license:
    #!/usr/bin/env bash
    set -euo pipefail
    current_year=$(date +%Y)
    license_year=$(grep -o 'Copyright (c) [0-9]\{4\}' LICENSE | grep -o '[0-9]\{4\}')
    if [[ "$license_year" != "$current_year" ]]; then
        echo "error: LICENSE copyright year is $license_year, expected $current_year"
        exit 1
    fi
    echo "license copyright year is $current_year (ok)"

# verify all packages share the same version
check-versions:
    #!/usr/bin/env bash
    set -euo pipefail
    expected=$(just _current)
    status=0
    for f in $(just _version_files); do
        found=$(grep -m1 '^version' "$f" | sed 's/version = "\(.*\)"/\1/')
        if [[ "$found" != "$expected" ]]; then
            echo "version mismatch: $f is $found, expected $expected"
            status=1
        fi
    done
    if [[ $status -ne 0 ]]; then
        exit 1
    fi
    echo "all packages at $expected (ok)"

# ── versioning ────────────────────────────────────────────────────────────────

# print the current version
_current:
    @grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'

# every file carrying the workspace version, derived from the workspace
# members so a newly added crate is covered without editing this file
_version_files:
    #!/usr/bin/env bash
    set -euo pipefail
    awk '/^members = \[/,/^\]/' Cargo.toml | grep -oE '"[^"]+"' | tr -d '"' \
        | while read -r member; do echo "$member/Cargo.toml"; done
    echo pytix/pyproject.toml

# bump major, minor, or patch across all crates and pyproject.toml
bump type:
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    case "{{ type }}" in
        major) new="$((major + 1)).0.0" ;;
        minor) new="$major.$((minor + 1)).0" ;;
        patch) new="$major.$minor.$((patch + 1))" ;;
        *) echo "unknown bump type: {{ type }} (major, minor, patch)"; exit 1 ;;
    esac
    just _bump "$current" "$new"

# set an explicit version across all crates and pyproject.toml
set-version version:
    @just _bump $(just _current) {{ version }}

_bump old new:
    #!/usr/bin/env bash
    set -euo pipefail
    # Rewrites each file's first version line whatever it currently says,
    # rather than matching the old value — a crate that has drifted out of
    # lockstep is pulled back in instead of being silently skipped.
    for f in $(just _version_files); do
        perl -pi -e 'if (!$done && s/^version = ".*"/version = "{{ new }}"/) { $done = 1 }' "$f"
    done
    # Both lockfiles record the packages' own versions, so a bump that skips
    # them leaves them stale until the next unrelated `cargo build` or
    # `uv sync` quietly rewrites them into somebody else's diff — which is
    # how pytix/uv.lock sat at 3.0.0 through the whole 3.1.0 cycle.
    #
    # Deliberately not the `upgrade-*` recipes: a re-lock at the current
    # dependency versions, never an upgrade. Bumping tix's own version must
    # not quietly move everything it depends on.
    cargo update --workspace --quiet
    (cd pytix && uv sync --quiet)
    # The install page prints the version to download, so the generated
    # release data is stale the moment a bump lands — same reason the
    # lockfiles are refreshed here, and `just check-docs` fails without it.
    just docs-cli
    echo "bumped {{ old }} → {{ new }}"

# ── release ───────────────────────────────────────────────────────────────────

# tag the current version in git
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    git tag "v$version"
    echo "tagged v$version"

# tag the current version and push to origin (triggers release workflow)
push-tag: tag
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    git push origin "v$version"
    echo "pushed v$version"
