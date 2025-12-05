#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 vX.Y.Z" >&2
  exit 1
fi

TAG="$1"

# Ensure clean working tree (we only tag existing merge commits)
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "error: git working tree is dirty. Commit or stash changes first." >&2
  exit 1
fi

echo "==> Creating tag ${TAG} on current HEAD"
git tag "${TAG}"

echo "==> Pushing tag to origin (no branch push to avoid branch protection)"
git push origin "${TAG}"

echo "Done."
