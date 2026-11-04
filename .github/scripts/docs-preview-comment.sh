#!/usr/bin/env bash
# Create or update the single docs-preview comment on a pull request.
#
# usage: docs-preview-comment.sh [--update-only] PR BODY-FILE
#
# The comment is found by a hidden marker on its first line, so every push
# edits one comment instead of adding another. --update-only leaves a PR with
# no preview comment alone, which is what cleanup wants: a PR that never had a
# preview should not get a comment saying it was removed.
#
# Needs GH_TOKEN with pull-requests: write, and GITHUB_REPOSITORY.
set -euo pipefail

update_only=false
if [[ ${1:-} == --update-only ]]; then
  update_only=true
  shift
fi
(($# == 2)) || { echo "usage: docs-preview-comment.sh [--update-only] PR BODY-FILE" >&2; exit 2; }
pr=$1
body_file=$2

marker="<!-- docs-preview -->"
body="$marker"$'\n'"$(cat "$body_file")"

# Only our own comments: anyone can post a comment that starts with the marker.
ids=$(gh api --paginate "repos/$GITHUB_REPOSITORY/issues/$pr/comments" \
  --jq ".[] | select(.user.login == \"github-actions[bot]\" and (.body | startswith(\"$marker\"))) | .id")
id=${ids%%$'\n'*}

if [[ -n $id ]]; then
  gh api --method PATCH "repos/$GITHUB_REPOSITORY/issues/comments/$id" \
    -f body="$body" --silent
  echo "updated preview comment $id on #$pr"
elif ! $update_only; then
  gh api --method POST "repos/$GITHUB_REPOSITORY/issues/$pr/comments" \
    -f body="$body" --silent
  echo "posted preview comment on #$pr"
else
  echo "no preview comment on #$pr to update"
fi
