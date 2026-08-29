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
