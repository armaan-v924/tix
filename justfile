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
    cargo build --release -p tix-cli --target {{target}}

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
test-python:
    uvx pytest pytix

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
    engine=$(grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    cli=$(grep '^version' tix-cli/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    pytix_cargo=$(grep '^version' pytix/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    pytix_py=$(grep '^version' pytix/pyproject.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    for v in "$cli" "$pytix_cargo" "$pytix_py"; do
        if [[ "$v" != "$engine" ]]; then
            echo "version mismatch: tix-engine=$engine but found $v in one of the packages"
            exit 1
        fi
    done
    echo "all packages at $engine (ok)"

# ── versioning ────────────────────────────────────────────────────────────────

# print the current version
_current:
    @grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'

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
    files=(tix-engine/Cargo.toml tix-cli/Cargo.toml pytix/Cargo.toml pytix/pyproject.toml)
    for f in "${files[@]}"; do
        perl -pi -e "s/^version = \"{{ old }}\"/version = \"{{ new }}\"/" "$f"
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
