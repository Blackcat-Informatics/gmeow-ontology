#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Execute the actual docs recipe in isolation. Every producer command is an
# inert local stub: this test cannot compile or materialize a corpus.
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
mkdir -p "$repo_root/.cache"
scratch=$(mktemp -d "$repo_root/.cache/check-sync-prerequisites.XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT

awk '
    /^check-sync:/ { recording = 1; print "probe:"; next }
    recording && /^\t/ { print; next }
    recording && /^$/ { next }
    recording { exit }
' "$repo_root/Makefile" > "$scratch/Makefile"

cat > "$scratch/producer-stub" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$CALL_LOG"
case "$1" in
    check-sync) exit "$FAIL_SYNC" ;;
    build) exit "$FAIL_BUILD" ;;
    sync) exit 0 ;;
    *) echo "unexpected inert producer operation: $1" >&2; exit 2 ;;
esac
STUB
chmod +x "$scratch/producer-stub"

run_probe() {
    CALL_LOG="$scratch/calls" FAIL_SYNC="$1" FAIL_BUILD="$2" \
        make --no-print-directory -C "$scratch" -f Makefile probe \
        SYNC_OUTPUTS=docs SYNC_MODE=update \
        MAKE="\"$scratch/producer-stub\"" GMEOW_DEV="\"$scratch/producer-stub\"" \
        > "$scratch/output" 2>&1
}

if run_probe 1 0; then
    echo "docs prerequisites accepted a failed generated prerequisite" >&2
    exit 1
fi
mapfile -t calls < "$scratch/calls"
[[ ${#calls[@]} == 1 && ${calls[0]} == 'check-sync '* ]] || {
    echo "docs synchronization continued after its first prerequisite failed" >&2
    exit 1
}

: > "$scratch/calls"
if run_probe 0 1; then
    echo "docs prerequisites accepted a failed build prerequisite" >&2
    exit 1
fi
mapfile -t calls < "$scratch/calls"
[[ ${#calls[@]} == 2 && ${calls[0]} == 'check-sync '* && ${calls[1]} == build ]] || {
    echo "docs synchronization continued after its build prerequisite failed" >&2
    exit 1
}

: > "$scratch/calls"
run_probe 0 0
mapfile -t calls < "$scratch/calls"
[[ ${#calls[@]} == 3 && ${calls[0]} == 'check-sync '* && ${calls[1]} == build && ${calls[2]} == 'sync '* ]] || {
    echo "docs prerequisites lost their generated/build/docs ordering" >&2
    exit 1
}
echo "check-sync prerequisites fail closed OK"
