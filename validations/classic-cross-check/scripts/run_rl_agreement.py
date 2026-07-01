# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Standalone runner for the enforced native-RL ≡ owlrl-RL agreement axis.

Relocated from the retired gmeow-dev ``classic-cross-check-rl`` subcommand
(#1087). The native OWL 2 RL engine is the Docker-free entailment authority;
``owlrl`` lives ONLY here as the agreement ORACLE. Reasons the told facts under
BOTH RL closures, compares the canonicalized named-vocabulary closures, writes
the agreement matrix + per-engine timing as SARIF/JSON, and exits NON-ZERO on
any real RL divergence.
"""

from __future__ import annotations

import sys

from oracles import rl_agreement


def main() -> int:
    passed, result, _report = rl_agreement.run()

    native_only = result["native_only"]
    oracle_only = result["oracle_only"]
    assert isinstance(native_only, list)
    assert isinstance(oracle_only, list)
    print(
        "RL cross-check — agreement: "
        f"agree={result['agree']} native_only={len(native_only)} "
        f"oracle_only={len(oracle_only)}"
    )
    if passed:
        print("✓ native RL ≡ owlrl RL (named-vocabulary closure)")
        return 0
    for row in native_only:
        print(f"NativeOnly {row}", file=sys.stderr)
    for row in oracle_only:
        print(f"OracleOnly {row}", file=sys.stderr)
    print(
        f"✗ native↔owlrl RL divergence: {len(native_only)} native-only + "
        f"{len(oracle_only)} oracle-only row(s)",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
