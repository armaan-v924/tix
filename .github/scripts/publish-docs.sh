#!/usr/bin/env bash
# Publish site trees to the `docs` branch, which GitHub Pages serves at
# tix.armaanv.dev. Every workflow that writes the branch goes through here:
# docs.yml for /dev/, /latest/ and /vX.Y/, and the preview workflows for
# /pr/<number>/.
#
# usage: publish-docs.sh -m MESSAGE [--put TREE DIR]... [--remove TREE]...
#
#   --put TREE DIR   replace TREE on the branch with the contents of DIR
#   --remove TREE    delete TREE from the branch (already absent is fine)
#
# TREE is one of dev, latest, vX.Y or pr/<number>.
#
# Publishing is additive: only the trees named on the command line change, and
# every other tree is left exactly as it was. That is enforced, not assumed:
# before committing, every staged path is checked against the named trees and
# the root files this script owns, and anything else fails the run. A preview
# names only pr/<number>, so it cannot touch /dev/, /latest/ or /vX.Y/.
#
# Several workflows write the branch and they are not serialised with each
# other, so the push is a plain fast-forward, never a force-push. If another
# writer got there first the push is rejected and the update is redone from
# scratch on the new tip. Trees are copied from the staged directories rather
# than merged, so a redo always produces the same trees.
set -euo pipefail

usage() {
  echo "usage: publish-docs.sh -m MESSAGE [--put TREE DIR]... [--remove TREE]..." >&2
  exit 2
}

valid_tree() {
  [[ $1 =~ ^(dev|latest|v[0-9]+\.[0-9]+|pr/[0-9]+)$ ]]
}

message=""
put_trees=()
put_dirs=()
remove_trees=()
while (($#)); do
  case "$1" in
    -m | --message)
      (($# >= 2)) || usage
      message=$2
      shift 2
      ;;
    --put)
      (($# >= 3)) || usage
      [[ -d $3 ]] || { echo "publish-docs: not a directory: $3" >&2; exit 2; }
      put_trees+=("$2")
      put_dirs+=("$(realpath "$3")")
      shift 3
      ;;
    --remove)
      (($# >= 2)) || usage
      remove_trees+=("$2")
      shift 2
      ;;
    *) usage ;;
  esac
done

trees=("${put_trees[@]}" "${remove_trees[@]}")
[[ -n $message ]] || usage
((${#trees[@]})) || usage
for tree in "${trees[@]}"; do
  valid_tree "$tree" || { echo "publish-docs: not a site tree: $tree" >&2; exit 2; }
done

# Pages reads these from the branch root, so they live there and not inside
# any one tree. They are regenerated from the branch contents on every
# publish, so writing them never depends on which trees a run changed.
root_files=(.nojekyll CNAME robots.txt versions.json index.html)

remote=${DOCS_REMOTE:-origin}
branch=docs
workdir=$(mktemp -d)
site=$workdir/site
# shellcheck disable=SC2329 # invoked by the trap below
cleanup() {
  git worktree remove --force "$site" >/dev/null 2>&1 || true
  rm -rf "$workdir"
}
trap cleanup EXIT

# A worktree rather than a branch switch: the caller's checkout is full of
# build output that `git add -A` would happily commit, and a worktree gives a
# tree containing the docs branch and nothing else. It shares .git, so pushing
# needs no separate credentials.
checkout_branch() {
  git worktree remove --force "$site" >/dev/null 2>&1 || true
  rm -rf "$site"
  git worktree prune
  if git ls-remote --exit-code --heads "$remote" "$branch" >/dev/null 2>&1; then
    git fetch --quiet "$remote" "+refs/heads/$branch:refs/remotes/$remote/$branch"
    git worktree add --quiet --force -B "$branch" "$site" "$remote/$branch"
  else
    git worktree add --quiet --detach "$site"
    git -C "$site" checkout --quiet --orphan "$branch"
    git -C "$site" rm -rfq . >/dev/null 2>&1 || true
  fi
}

apply_trees() {
  local i tree
  for tree in "${remove_trees[@]}"; do
    rm -rf "${site:?}/$tree"
  done
  for i in "${!put_trees[@]}"; do
    tree=${put_trees[i]}
    rm -rf "${site:?}/$tree"
    mkdir -p "$site/$tree"
    cp -R "${put_dirs[i]}/." "$site/$tree/"
  done
}

write_root_files() {
  # Without .nojekyll, Jekyll drops `_astro/` and every site serves unstyled.
  touch "$site/.nojekyll"
  echo "tix.armaanv.dev" >"$site/CNAME"
  # Previews are throwaway copies of pages that exist under /dev/ or a
  # release; keep them out of search results.
  printf 'User-agent: *\nDisallow: /pr/\n' >"$site/robots.txt"

  python3 - "$site" <<'PY'
import json, os, re, sys

root = sys.argv[1]
versions = sorted(
    (d for d in os.listdir(root) if re.fullmatch(r"v\d+\.\d+", d)),
    key=lambda d: [int(p) for p in d[1:].split(".")],
    reverse=True,
)
has = lambda name: os.path.isdir(os.path.join(root, name))

# Read by the version switcher when there is more than one release to switch
# between; until then it is just a manifest of what the branch holds.
# Previews are deliberately not listed: they are not versions.
manifest = {
    "latest": versions[0] if versions else None,
    "versions": versions,
    "dev": has("dev"),
}
with open(os.path.join(root, "versions.json"), "w") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")

# Nothing deep-links to the root, so a redirect is enough — and it has to be
# a real page, because Pages has no server-side rewrites.
if has("latest"):
    home = "/latest/"
elif versions:
    home = f"/{versions[0]}/"
else:
    home = "/dev/"
with open(os.path.join(root, "index.html"), "w") as f:
    f.write(
        "<!DOCTYPE html>\n"
        '<html lang="en">\n<head>\n<meta charset="utf-8">\n'
        "<title>tix documentation</title>\n"
        f'<link rel="canonical" href="https://tix.armaanv.dev{home}">\n'
        f'<meta http-equiv="refresh" content="0; url={home}">\n'
        "</head>\n<body>\n"
        f'<p><a href="{home}">tix documentation</a></p>\n'
        "</body>\n</html>\n"
    )
PY
}

allowed_path() {
  local path=$1 name tree
  for name in "${root_files[@]}"; do
    [[ $path == "$name" ]] && return 0
  done
  for tree in "${trees[@]}"; do
    [[ $path == "$tree/"* ]] && return 0
  done
  return 1
}

# The guarantee that a publish touches only what it names. --no-renames
# matters: with rename detection a file moved from dev/ into pr/1/ would be
# listed only under its new name, and the old tree's loss would go unseen.
check_staged_paths() {
  local path unexpected=()
  while IFS= read -r -d '' path; do
    allowed_path "$path" || unexpected+=("$path")
  done < <(git -C "$site" diff --cached --name-only --no-renames -z)
  if ((${#unexpected[@]})); then
    echo "publish-docs: refusing to publish; these paths are outside ${trees[*]}:" >&2
    printf '  %s\n' "${unexpected[@]:0:20}" >&2
    exit 1
  fi
}

for attempt in 1 2 3 4 5; do
  checkout_branch
  apply_trees
  write_root_files
  git -C "$site" add -A
  check_staged_paths

  if git -C "$site" diff --cached --quiet; then
    echo "publish-docs: nothing to publish for ${trees[*]}"
    exit 0
  fi
  git -C "$site" \
    -c user.name="github-actions" \
    -c user.email="github-actions@users.noreply.github.com" \
    commit --quiet -m "$message"
  git -C "$site" diff --stat HEAD~1 HEAD 2>/dev/null | tail -1 || true

  if git -C "$site" push --quiet "$remote" "HEAD:refs/heads/$branch"; then
    echo "publish-docs: pushed ${trees[*]}"
    exit 0
  fi
  echo "publish-docs: push rejected (attempt $attempt); redoing on the new tip" >&2
  sleep $((attempt * 3))
done

echo "publish-docs: gave up after $attempt attempts" >&2
exit 1
