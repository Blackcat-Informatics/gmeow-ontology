#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
staged=${1:-"$repo_root/dist/bin/gmeow"}
staged=$(realpath "$staged")
scratch=$(mktemp -d)
repo_mode=$(stat -c '%a' "$repo_root")
restore() {
  chmod "$repo_mode" "$repo_root"
  rm -rf "$scratch"
}
trap restore EXIT HUP INT TERM

mkdir -p "$scratch/install/bin" "$scratch/blind"
cp "$staged" "$scratch/install/bin/gmeow"

cat >"$scratch/request.logic.ttl" <<'EOF'
@prefix ex: <https://example.org/> .
ex:subject ex:relation ex:object .
EOF

cat >"$scratch/eprover" <<'EOF'
#!/bin/sh
if [ "$1" = "--version" ]; then
  echo 'gmeow-one-file-eprover-stub 1.0'
  exit 0
fi
echo '% SZS status Satisfiable'
EOF
chmod 755 "$scratch/eprover"

# The executable and request now live outside the checkout. Remove search and
# absolute-path access to the complete source/generated tree for the duration of
# both consumer operations; the EXIT trap restores the directory on every
# ordinary success or failure path.
chmod 000 "$repo_root"

(
  cd "$scratch/blind"
  env -i \
    GMEOW_PROVER_PATH="$scratch/eprover" \
    "$scratch/install/bin/gmeow" prove "$scratch/request.logic.ttl" --format json \
    >"$scratch/prove.json"
  env -i \
    "$scratch/install/bin/gmeow" gmn digest "$scratch/request.logic.ttl" --format json \
    >"$scratch/gmn.json"
)

grep -q '"verdict": "consistent"' "$scratch/prove.json"
grep -q '"codebook_digest"' "$scratch/gmn.json"
grep -q '"content_digest"' "$scratch/gmn.json"

echo "one-file gmeow proof + embedded-asset smoke passed"
