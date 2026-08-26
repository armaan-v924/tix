# lists available just recipes
default:
    @just --list

# ── dev ───────────────────────────────────────────────────────────────────────

# run all checks and tests (local pre-flight before pushing)
ci: check test
    @echo "all good"

# upgrade all lockfiles and commit
upgrade:
    #!/usr/bin/env bash
    set -euo pipefail
    uv sync --upgrade
    cargo update --workspace
    git add uv.lock Cargo.lock
    git commit -m "chore: upgrade lockfiles"

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
check: check-license check-versions lint
    @echo "all checks passed"

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
