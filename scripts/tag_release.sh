#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 vX.Y.Z" >&2
  exit 1
fi

TAG="$1"
VERSION="${TAG#v}"

# Ensure clean working tree
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "error: git working tree is dirty. Commit or stash changes first." >&2
  exit 1
fi

echo "==> Bumping version in Cargo.toml and Cargo.lock to ${VERSION}"
perl -0777 -pi -e "s/(^version = )\"[^\"]+\"/\$1\"${VERSION}\"/m" Cargo.toml
cargo update -p tix

echo "==> Git status after version bump:"
git status -sb

echo "==> Committing and tagging"
git commit -am "Release ${TAG}"
git tag "${TAG}"

echo "==> Pushing commit and tag to origin"
git push origin HEAD
git push origin "${TAG}"

echo "Done."
