# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Standalone runner for the enforced native↔ELK/HermiT divergence cross-check.

Relocated from the retired gmeow-dev ``classic-cross-check`` subcommand.
Reasons the bundle natively (authority), runs the classic ELK + HermiT oracles,
calls the authoritative Rust comparator, writes the agreement matrix + per-tool
timing as SARIF/JSON, and exits NON-ZERO on any real divergence (``NativeOnly`` /
``OracleOnly``) or native coverage defect (``DlGap``). Enforces Principle 18.
"""

from __future__ import annotations

import sys

from oracles import classic_cross_check as crosscheck

from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError


def main() -> int:
    try:
        passed, ledger, _report = crosscheck.run()
    except ToolUnavailableError as exc:
        print(f"tool unavailable: {exc}", file=sys.stderr)
        return 2
    except ToolExecutionError as exc:
        print(f"classic cross-check oracle failed:\n{exc.output}", file=sys.stderr)
        return 1

    print(
        "classic cross-check — agreement matrix: "
        f"agree={ledger['agree']} native_only={ledger['native_only']} "
        f"oracle_only={ledger['oracle_only']} dl_gap={ledger['dl_gap']}"
    )
    if passed:
        print("✓ native ≡ oracle (ELK/HermiT) with zero native DL gaps")
        return 0
    for row in ledger["rows"]:
        if row["kind"] in ("NativeOnly", "OracleOnly", "DlGap"):
            print(f"{row['kind']} {row['detail']}", file=sys.stderr)
    print(
        f"✗ native↔oracle divergence: {ledger['native_only']} native-only + "
        f"{ledger['oracle_only']} oracle-only + {ledger['dl_gap']} dl-gap row(s)",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
