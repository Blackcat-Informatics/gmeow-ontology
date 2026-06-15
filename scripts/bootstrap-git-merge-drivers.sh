#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Skipping Git merge-driver bootstrap: not inside a worktree."
  exit 0
fi

git config --local merge.ours.driver true
git config --local merge.ours.name "Keep the current branch copy for generated binary artifacts"

echo "Configured Git merge driver: merge.ours.driver=true"
