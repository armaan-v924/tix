# lists available just recipes
default:
    @just --list

# ── dev ───────────────────────────────────────────────────────────────────────

# upgrade all lockfiles and commit
upgrade: upgrade-rust upgrade-python
    #!/usr/bin/env bash
    set -euo pipefail
    git add -u
    git commit -m "chore: upgrade lockfiles"

# upgrade the rust lockfile
upgrade-rust:
    cargo update

# upgrade the python lockfile
[working-directory("pytix")]
upgrade-python:
    uv sync --upgrade

# ── hooks ─────────────────────────────────────────────────────────────────────

# point git at the checked-in hooks (one-time per clone)
hook-install:
    git config core.hooksPath .githooks

# fast, non-compiling checks — every commit
hook-pre-commit: lint-rust-format lint-python-ruff check-license check-versions

# the full local pre-flight suite — once per push rather than per commit
hook-pre-push: check lint test

# ── build ─────────────────────────────────────────────────────────────────────

# build the CLI binary for a given target triple
build-cli target:
    cargo build --release -p tix-cli --target {{ target }}

# package a built CLI binary into dist/ exactly as the release ships it
#
# The name and layout are a contract: `tix cli update` and the install docs
# fetch `tix-<tag>-<os>-<arch>.<tar.gz|zip>`, expect the executable at the
# archive root, and the updater refuses an archive without its sibling
# `.sha256`. `tag` defaults to `v` + the manifest version; the release passes
# the pushed tag so assets are named after the tag they are attached to.
package-cli target tag="":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ target }}" in
        x86_64-unknown-linux-gnu) suffix=linux-x86_64   ext=tar.gz exe=tix ;;
        aarch64-apple-darwin)     suffix=macos-aarch64  ext=tar.gz exe=tix ;;
        x86_64-pc-windows-msvc)   suffix=windows-x86_64 ext=zip    exe=tix.exe ;;
        *) echo "error: no release archive is defined for {{ target }}"; exit 1 ;;
    esac
    tag="{{ tag }}"
    [[ -n "$tag" ]] || tag="v$(just _current)"
    archive="tix-$tag-$suffix.$ext"
    # Where cargo put it: CARGO_TARGET_DIR when set, so a build shared
    # between checkouts is found too, else ./target as in CI.
    binary="${CARGO_TARGET_DIR:-target}/{{ target }}/release/$exe"
    if [[ ! -f "$binary" ]]; then
        echo "error: $binary not found — run \`just build-cli {{ target }}\` first"
        exit 1
    fi
    # Staged under target/ and wiped first, so a file left over from an
    # earlier run can never end up in the archive. Relative paths throughout:
    # on Windows the zip is made by PowerShell, which cannot read MSYS paths.
    stage="target/package/{{ target }}"
    rm -rf "$stage"
    mkdir -p "$stage" dist
    cp "$binary" README.md LICENSE "$stage/"
    rm -f "dist/$archive" "dist/$archive.sha256"
    case "$ext" in
        tar.gz) tar -czf "dist/$archive" -C "$stage" . ;;
        zip) pwsh -NoProfile -NonInteractive -Command \
            "Compress-Archive -Path '$stage/*' -DestinationPath 'dist/$archive'" ;;
    esac
    # A sha256sum-style "<digest>  <name>" line naming the bare archive, so it
    # is valid input for both the updater and `sha256sum -c` run from dist/.
    # Formatted here rather than taken verbatim from the tool: macOS runners
    # carry shasum, not coreutils' sha256sum, and the two differ in output.
    if command -v sha256sum >/dev/null 2>&1; then
        digest=$(sha256sum "dist/$archive")
    else
        digest=$(shasum -a 256 "dist/$archive")
    fi
    printf '%s  %s\n' "${digest%% *}" "$archive" > "dist/$archive.sha256"
    echo "packaged dist/$archive"

# build the pytix wheel for a given target triple
[working-directory("pytix")]
build-wheel target:
    uv build --wheel --out-dir ../dist -C build-args="--strip --target {{ target }}"

# ── docs ──────────────────────────────────────────────────────────────────────

# build rustdoc
docs-cargo:
    cargo doc --no-deps

# regenerate the CLI reference and man pages from the clap definition
docs-cli:
    cargo run -q -p xtask

# install the docs site's node dependencies
docs-deps:
    npm --prefix docs ci

# serve the docs site locally with live reload
docs-serve: docs-cli
    npm --prefix docs run dev

# build the documentation site into docs/dist, without rustdoc
docs-site: docs-cli
    npm --prefix docs run build

# build the documentation site into docs/dist, rustdoc included
docs-build: docs-site docs-cargo
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf docs/dist/crates
    mkdir -p docs/dist/crates
    cp -R target/doc/. docs/dist/crates/
    cp docs/crates-index.html docs/dist/crates/index.html
    echo "site built at docs/dist"

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

# ── smoke ─────────────────────────────────────────────────────────────────────

# run all smoke tests
smoke: smoke-wheel smoke-binary

# import and exercise a built wheel from ./dist in a throwaway venv
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

# unpack the packaged CLI from ./dist and check the shipped binary runs
#
# Runs the archive, not target/…/release/: signing and packaging both happen
# after the build, and either can break a binary that built fine. Only
# startup-and-parse checks, so nothing touches the network or a real config.
smoke-binary:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    archives=(dist/tix-*.tar.gz dist/tix-*.zip)
    if [[ ${#archives[@]} -ne 1 ]]; then
        echo "error: expected exactly one CLI archive in dist/, found ${#archives[@]}"
        echo "       (build one with \`just build-cli <target> && just package-cli <target>\`)"
        exit 1
    fi
    archive=${archives[0]}
    name=${archive#dist/}
    # The updater refuses an archive whose digest is missing or wrong, so a
    # bad .sha256 is as broken a release as a bad binary.
    if [[ ! -f "$archive.sha256" ]]; then
        echo "error: $archive.sha256 not found"
        exit 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        (cd dist && sha256sum -c "$name.sha256")
    else
        (cd dist && shasum -a 256 -c "$name.sha256")
    fi
    unpacked=target/smoke-binary
    rm -rf "$unpacked"
    mkdir -p "$unpacked"
    case "$archive" in
        *.tar.gz)
            tar -xzf "$archive" -C "$unpacked"
            bin="$unpacked/tix" ;;
        *.zip)
            if command -v unzip >/dev/null 2>&1; then
                unzip -q "$archive" -d "$unpacked"
            else
                pwsh -NoProfile -NonInteractive -Command \
                    "Expand-Archive -Path '$archive' -DestinationPath '$unpacked'"
            fi
            bin="$unpacked/tix.exe" ;;
    esac
    # -x as well as -f: a lost executable bit is a packaging bug to catch here.
    if [[ ! -x "$bin" ]]; then
        echo "error: $name does not contain an executable ${bin#"$unpacked"/}"
        exit 1
    fi
    # Pointed at a config that does not exist, so a developer's own config
    # cannot change the result. Relative, so Git Bash on Windows has no MSYS
    # path to translate for the binary.
    export TIX_CONFIG_PATH="$unpacked/no-config.toml"
    # Proves the binary starts — so dynamic linking resolved — and reports
    # the version it was cut as, which check-versions can't see.
    expected="tix $(just _current)"
    actual=$("$bin" --version)
    actual=${actual%$'\r'}
    if [[ "$actual" != "$expected" ]]; then
        echo "error: $name reports '$actual', expected '$expected'"
        exit 1
    fi
    # Builds the whole clap command tree, which panics on a wiring mistake.
    "$bin" --help >/dev/null
    # Exercises clap_complete.
    if [[ -z "$("$bin" completions bash)" ]]; then
        echo "error: \`tix completions bash\` printed nothing"
        exit 1
    fi
    echo "$name: $actual, --help and completions ok"

# ── lint ──────────────────────────────────────────────────────────────────────

# run all lint and formatting checks
lint: lint-rust lint-python

# run rust formatting and clippy
lint-rust: lint-rust-format lint-rust-clippy

# run python lint and formatting checks (ruff)
lint-python: lint-python-ruff

# verify rust is formatted
lint-rust-format:
    cargo fmt --check

# verify rust code passes clippy
lint-rust-clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# verify python code in ./pytix is formatted and linted
#
# Pinned so a new ruff release can't change what passes; bump it on purpose.
# `uvx` rather than a dev dependency: `uv run` would build the extension first.
lint-python-ruff:
    uvx ruff@0.17.3 check pytix

# ── checks ────────────────────────────────────────────────────────────────────

# run all non-lint checks (CI entrypoint)
check: check-license check-versions check-docs
    @echo "all checks passed"

# verify the generated CLI reference and man pages match the CLI
check-docs:
    #!/usr/bin/env bash
    set -euo pipefail
    # Listed precisely rather than by directory: reference/ also holds
    # hand-written pages, which this must not treat as drift.
    generated="docs/man docs/src/data \
        docs/src/content/docs/reference/cli \
        docs/src/content/docs/reference/configuration.mdx"
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
    current=$(just _current)
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
    for f in $(just _version_files); do
        perl -pi -e 'if (!$done && s/^version = ".*"/version = "{{ new }}"/) { $done = 1 }' "$f"
    done
    cargo update --workspace --quiet
    (cd pytix && uv sync --quiet)
    just docs-cli
    echo "bumped {{ old }} → {{ new }}"

# ── release ───────────────────────────────────────────────────────────────────

# tag the current version in git
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(just _current)
    git tag "v$version"
    echo "tagged v$version"

# tag the current version and push to origin (triggers release workflow)
push-tag: tag
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(just _current)
    git push origin "v$version"
    echo "pushed v$version"

# print a release's notes from its Conventional Commits; no tag previews HEAD (needs git-cliff)
release-notes tag="":
    #!/usr/bin/env bash
    # Build Release runs this with the pushed tag and uses the output as the
    # GitHub release body. Every tag must be present locally
    # (`git fetch --tags`), including tags on other release branches.
    #
    # The range starts at the highest tag below this version, on any branch,
    # rather than at the nearest ancestor tag. v3.0.0 is not an ancestor of
    # main, so `git describe` would go back to v2.1.0 and repeat all of 3.0
    # in v3.1.0's notes.
    #
    # A fix ends up on two branches with different SHAs. Either it is made on
    # release/X.Y and cascaded to main, or made on main and backported. So the
    # next minor release's range holds a copy of something a patch release
    # already shipped. A commit in the range is skipped when:
    #   - it was cherry-picked from a commit an earlier tag shipped,
    #   - a commit an earlier tag shipped was cherry-picked from it, or
    #   - its diff is identical to one an earlier tag shipped (patch-id).
    # The first two use the `(cherry picked from commit …)` line that
    # `cherry-pick -x` adds, and cascade-merge.yml always passes -x. They still
    # match when the subject was reworded on the way across. The third catches
    # a backport made without -x.
    set -euo pipefail
    tag="{{ tag }}"
    if [[ -n "$tag" ]]; then
        if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
            echo "error: no tag $tag — run \`git fetch --tags\`?" >&2
            exit 1
        fi
        end="$tag"
        version="${tag#v}"
    else
        end=HEAD
        version=$(just _current)
    fi

    # Every release before this one, from any branch. When previewing HEAD,
    # the declared version counts if it is already tagged: HEAD's notes are
    # then whatever came after that tag.
    older=()
    for t in $(git tag -l 'v*' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' || true); do
        v="${t#v}"
        if [[ "$v" == "$version" ]]; then
            [[ "$end" == HEAD ]] && older+=("$t")
        elif [[ "$(printf '%s\n%s\n' "$v" "$version" | sort -V | head -1)" == "$v" ]]; then
            older+=("$t")
        fi
    done
    prev=""
    if [[ ${#older[@]} -gt 0 ]]; then
        prev=$(printf '%s\n' "${older[@]}" | sort -V | tail -1)
    fi
    range="${prev:+$prev..}$end"
    echo "release notes for ${tag:-HEAD (v$version, unreleased)}: $range" >&2

    # What earlier releases shipped that this range cannot see: commits on
    # other release branches.
    skips=()
    if [[ -n "$prev" ]]; then
        tmp=$(mktemp -d)
        trap 'rm -rf "$tmp"' EXIT
        picked='cherry picked from commit [0-9a-f]{40}'
        git rev-list "${older[@]}" --not "$end" > "$tmp/shipped"
        # "<source> <shipped commit>" for each shipped cherry-pick.
        while read -r s; do
            git log -1 --format=%B "$s" | { grep -oE "$picked" || true; } \
                | awk -v s="$s" '{ print $NF, s }'
        done < "$tmp/shipped" > "$tmp/picked-into"
        # "<patch-id> <commit>" for both sides.
        git log -p --no-merges --no-color "${older[@]}" --not "$end" \
            | git patch-id --stable > "$tmp/shipped-patches"
        git log -p --no-merges --no-color "$range" \
            | git patch-id --stable > "$tmp/range-patches"

        for c in $(git rev-list "$range"); do
            twin=$(awk -v c="$c" '$1 == c { print $2; exit }' "$tmp/picked-into")
            if [[ -z "$twin" ]]; then
                for src in $(git log -1 --format=%B "$c" | { grep -oE "$picked" || true; } | awk '{ print $NF }'); do
                    if grep -qxF "$src" "$tmp/shipped"; then twin="$src"; break; fi
                done
            fi
            if [[ -z "$twin" ]]; then
                pid=$(awk -v c="$c" '$2 == c { print $1; exit }' "$tmp/range-patches")
                if [[ -n "$pid" ]]; then
                    twin=$(awk -v p="$pid" '$1 == p { print $2; exit }' "$tmp/shipped-patches")
                fi
            fi
            if [[ -n "$twin" ]]; then
                shipped_in=$(git tag --contains "$twin" -l 'v*' --sort=v:refname | head -1)
                echo "  skip ${c:0:7} $(git log -1 --format=%s "$c")" >&2
                echo "       already shipped in $shipped_in as ${twin:0:7}" >&2
                skips+=(--skip-commit "$c")
            fi
        done
    fi

    if [[ -n "$tag" && -n "$prev" ]]; then
        export TIX_NOTES_COMPARE="$prev...$tag"
    fi
    # sed drops the blank line git-cliff prints before the body.
    git-cliff --config cliff.toml ${skips[@]+"${skips[@]}"} -- "$range" | sed '/./,$!d'
