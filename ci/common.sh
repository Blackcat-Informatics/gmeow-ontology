#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Common shell helpers shared by the repository's top-level scripts/ CI and
# build scripts: one validated strict-mode base plus deterministic, portable
# digest helpers so every scripts/*.sh shares the same safety and utility floor.
# This file is sourced, never executed directly.

set -euo pipefail

# sha256_file <path> — print the lowercase hex SHA-256 of a file (digest only,
# no filename). Prefers sha256sum, falls back to shasum. The output is
# byte-identical to `sha256sum -- <path> | cut -d' ' -f1`, so it is a drop-in
# replacement for the inline digest pipelines used across the receipt scripts.
sha256_file() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum -- "$1" | cut -d' ' -f1
  elif command -v shasum > /dev/null 2>&1; then
    shasum -a 256 -- "$1" | cut -d' ' -f1
  else
    echo "sha256_file: need sha256sum or shasum" >&2
    return 1
  fi
}
