#!/usr/bin/env bash
# Install the ICN Development Control Center from this checkout onto the host.
#
#   ops/dev-control/install.sh [--prefix DIR]      (default prefix: ~/icn-dev)
#
# Layout written (nothing else is touched):
#   <prefix>/share/icn-control/releases/<id>/    immutable copy of the code + programs.json + policy snapshot
#   <prefix>/share/icn-control/current           symlink to the release in use (swapped atomically)
#   <prefix>/bin/icn-control                     the launcher
#
# Installing from a release copy means the launcher keeps working when this worktree is
# cleaned up, and it is always clear which commit the running control center came from.
set -euo pipefail

prefix="${ICN_DEV_ROOT:-$HOME/icn-dev}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) prefix="$2"; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

src="$(cd -- "$(dirname -- "$(readlink -f -- "$0")")" && pwd)"
repo="$(git -C "$src" rev-parse --show-toplevel)"
sha="$(git -C "$repo" rev-parse HEAD)"
dirty=""
if [[ -n "$(git -C "$repo" status --porcelain -- ops/dev-control)" ]]; then
  dirty="-dirty-$(date -u +%Y%m%dT%H%M%SZ)"
fi
id="${sha:0:12}${dirty}"
share="$prefix/share/icn-control"
rel="$share/releases/$id"

python3 -m py_compile "$src"/icn_control/*.py

mkdir -p "$share/releases" "$prefix/bin"
if [[ ! -d "$rel" ]]; then
  tmp="$(mktemp -d "$share/releases/.tmp.XXXXXX")"
  command cp -R "$src/icn_control" "$tmp/icn_control"
  find "$tmp/icn_control" -name '__pycache__' -prune -exec rm -rf {} +
  command cp "$src/programs.json" "$tmp/programs.json"
  command cp "$repo/ops/state/truth/policy.json" "$tmp/policy.json"
  printf '{"source_repo": "%s", "commit": "%s", "dirty": %s, "installed_at": "%s"}\n' \
    "$repo" "$sha" "$([[ -n $dirty ]] && echo true || echo false)" "$(date -u +%FT%TZ)" > "$tmp/INSTALLED.json"
  chmod -R a-w "$tmp"
  mv -T "$tmp" "$rel"
fi
ln -sfn "releases/$id" "$share/.current.tmp"
mv -T "$share/.current.tmp" "$share/current"

command cp -f "$src/bin/icn-control" "$prefix/bin/.icn-control.tmp"
chmod 755 "$prefix/bin/.icn-control.tmp"
mv -f "$prefix/bin/.icn-control.tmp" "$prefix/bin/icn-control"

echo "installed release $id"
echo "launcher: $prefix/bin/icn-control"
echo "note: an already-running control center keeps its old code until: $prefix/bin/icn-control --restart"
