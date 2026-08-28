#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

if [[ $# -ne 1 || ! -f "$1/Cargo.toml" ]]; then
  echo "usage: $0 CRATE_DIR" >&2
  exit 2
fi

workspace=$(git rev-parse --show-toplevel)
cd "$workspace"
fingerprint_tmp=$(mktemp -d)
trap 'rm -rf -- "$fingerprint_tmp"' EXIT
rustc --edition=2024 build-support/list_producer_inputs.rs \
  -o "$fingerprint_tmp/list-producer-inputs"

"$fingerprint_tmp/list-producer-inputs" "$1" \
  | while IFS= read -r -d '' path; do
      sha256sum -- "$path"
    done \
  | sha256sum \
  | cut -d' ' -f1
