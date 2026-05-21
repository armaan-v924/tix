# lists available just recipes
default:
    @just --list

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
    case "{{type}}" in
        major) new="$((major + 1)).0.0" ;;
        minor) new="$major.$((minor + 1)).0" ;;
        patch) new="$major.$minor.$((patch + 1))" ;;
        *) echo "unknown bump type: {{type}} (major, minor, patch)"; exit 1 ;;
    esac
    just _bump "$current" "$new"

# set an explicit version across all crates and pyproject.toml
set-version version:
    @just _bump $(just _current) {{version}}

_bump old new:
    #!/usr/bin/env bash
    set -euo pipefail
    files=(tix-engine/Cargo.toml tix-cli/Cargo.toml pytix/Cargo.toml pytix/pyproject.toml)
    for f in "${files[@]}"; do
        sed -i '' "s/^version = \"{{old}}\"/version = \"{{new}}\"/" "$f"
    done
    echo "bumped {{old}} → {{new}}"

# ── release ───────────────────────────────────────────────────────────────────

# upgrade all lockfiles and commit
upgrade:
    #!/usr/bin/env bash
    set -euo pipefail
    uv sync --upgrade
    cargo update --workspace
    git add uv.lock Cargo.lock
    git commit -m "chore: upgrade lockfiles"

# run checks (entrypoint for CI and local pre-release validation)
# run all checks
check: check-license
    @echo "all checks passed"

# run all lint and formatting checks
lint: check-python-lint check-rust-format check-rust-clippy

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
    echo "License copyright year is $current_year (ok)"

# verify rust is formatted
check-rust-format:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --check

# verify rust code is linted
check-rust-clippy:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# verify python code in ./pytix is formatted and linted (ruff)
check-python-lint:
    #!/usr/bin/env bash
    set -euo pipefail
    uvx ruff check pytix

# verify python code in ./pytix is correct
check-python-test:
    #!/usr/bin/env bash
    set -euo pipefail
    uvx pytest pytix

# tag the current version in git
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    git tag "v$version"
    echo "tagged v$version"
