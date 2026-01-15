#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${1:-HEAD~1}"

if ! git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
  echo "Base ref '$BASE_REF' not found. Pass a valid ref." >&2
  exit 2
fi

export BASE_REF

python3 - <<'PY'
import os
import re
import subprocess
import sys

base_ref = os.environ.get('BASE_REF', 'HEAD~1')

def run(cmd):
    return subprocess.check_output(cmd, text=True).strip()

onboarding_root = "docs/onboarding"
paths = set()

path_re = re.compile(r"`([^`]+)`")

for root, _, files in os.walk(onboarding_root):
    for name in files:
        if not name.endswith(".md"):
            continue
        path = os.path.join(root, name)
        with open(path, "r", encoding="utf-8") as f:
            for match in path_re.findall(f.read()):
                if match.startswith(("icn/", "docs/", "sdk/", "web/", "deploy/", "config/", "scripts/")):
                    paths.add(match)

missing = [p for p in sorted(paths) if not os.path.exists(p)]
if missing:
    print("Onboarding references missing files:")
    for p in missing:
        print(f"  - {p}")
    sys.exit(1)

changed = run(["git", "diff", "--name-only", f"{base_ref}..HEAD"]).splitlines()
changed_set = set(changed)

impacted = sorted([p for p in paths if p in changed_set])
if impacted:
    print("Onboarding references changed files:")
    for p in impacted:
        print(f"  - {p}")
    sys.exit(1)

print("Onboarding mapping check passed.")
PY
