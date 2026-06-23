#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Flatten criterion estimates into one machine-stable JSON for #668 (T9, #790).

Criterion writes ``target/criterion/<group>/<bench>/new/estimates.json`` after
``make bench``; this collapses every benchmark to its mean/median point estimate
(nanoseconds) so the leaderboard can ingest a single flat document instead of
walking the criterion tree. Pure stdlib; reads the tree, writes JSON to stdout.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def collect(root: Path) -> dict[str, dict[str, float]]:
    """Map ``<group>/<bench>`` to its mean/median point estimates in ns."""
    out: dict[str, dict[str, float]] = {}
    for estimates in sorted(root.glob("**/new/estimates.json")):
        # target/criterion/<group>/<bench>/new/estimates.json
        rel = estimates.relative_to(root).parts
        if len(rel) < 4 or rel[-1] != "estimates.json":
            continue
        name = "/".join(rel[:-2])  # drop the trailing "new/estimates.json"
        data = json.loads(estimates.read_text(encoding="utf-8"))
        out[name] = {
            "mean_ns": float(data["mean"]["point_estimate"]),
            "median_ns": float(data["median"]["point_estimate"]),
        }
    return out


def main() -> int:
    """Write the flattened criterion estimates to stdout; return a process code."""
    root = Path("target/criterion")
    if not root.is_dir():
        sys.stderr.write(
            "no target/criterion — run `make bench` first to produce estimates\n"
        )
        return 1
    results = collect(root)
    if not results:
        sys.stderr.write("no criterion estimates found under target/criterion\n")
        return 1
    json.dump(results, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
