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
checks:
    @echo "no checks configured yet"

# tag the current version in git
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(grep '^version' tix-engine/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    git tag "v$version"
    echo "tagged v$version"
