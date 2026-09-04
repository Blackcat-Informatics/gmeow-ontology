#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Shared fail-closed helpers for repository shell entry points.
set -euo pipefail

gmeow_require_command() {
  local executable=$1
  local description=${2:-$1}

  if ! command -v "$executable" > /dev/null 2>&1; then
    printf '%s is required\n' "$description" >&2
    return 2
  fi
}

# Print the lowercase SHA-256 digest of one file without its filename.
gmeow_sha256_file() {
  local path=$1

  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum -- "$path" | cut -d' ' -f1
  elif command -v shasum > /dev/null 2>&1; then
    shasum -a 256 -- "$path" | cut -d' ' -f1
  else
    printf 'sha256sum or shasum is required\n' >&2
    return 2
  fi
}
