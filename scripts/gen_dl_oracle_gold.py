# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Generate FROZEN oracle gold for the #697 native-DL conformance lane.

This is a **fixture generator**, not a gate runtime (issue #697, Gap G). For each
curated OWL/gUFO dataset under ``coverage/external/697-dl-oracle-gold/datasets/``
it runs the pinned ROBOT/HermiT (and, where the dataset is EL-decidable, ELK)
Docker oracle and freezes the verdict — consistency, the sorted set of
unsatisfiable class IRIs, and full provenance — as a committed JSON file beside
the dataset.

The native Docker-free reasoner is then asserted (offline, in
``crates/conformance``) to reproduce every frozen oracle inconsistency/unsat:
native ⊇ oracle. The gold here is the ORACLE's verdict, produced by a real
HermiT/ELK run — never hand-typed to match native, never edited to make a test
pass (issue #697 honesty doctrine).

ROBOT reasoning verdict surface (verified against obolibrary/robot:v1.9.7):

* CONSISTENT, no unsat classes:  ``robot reason`` exits 0, no ERROR lines.
* CONSISTENT, N unsat classes:   exit 1, stderr lists
  ``There are N unsatisfiable classes`` + one ``unsatisfiable: <IRI>`` per class.
* INCONSISTENT:                   exit 1, stderr ``The ontology is inconsistent.``
  (the reasoner cannot enumerate unsat classes once the ABox collapses).

Run via ``make maint-697-oracle-gold`` (non-required maintainer lane; needs
Docker + the pinned ROBOT image, ``make maint-pull-images``).
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

from gmeow_tools.config import ROBOT_IMAGE
from gmeow_tools.runner import run_container

#: The curated-dataset + frozen-gold home (issue #697, the external-input fixture
#: convention — ``coverage/external/``).
GOLD_DIR = (
    Path(__file__).resolve().parent.parent
    / "coverage"
    / "external"
    / "697-dl-oracle-gold"
)
DATASETS_DIR = GOLD_DIR / "datasets"

#: ROBOT lists every unsatisfiable class on its own stderr line.
_UNSAT_RE = re.compile(r"unsatisfiable:\s*(\S+)")
#: ROBOT's inconsistency banner.
_INCONSISTENT_RE = re.compile(r"The ontology is inconsistent")


@dataclass(frozen=True)
class OracleVerdict:
    """A single reasoner's verdict over one dataset."""

    reasoner: str
    consistent: bool
    unsatisfiable_classes: list[str]


def _run_robot_reason(rel_input: str, reasoner: str) -> OracleVerdict:
    """Run ``robot reason`` over ``rel_input`` and parse the consistency verdict.

    ``rel_input`` is repo-root-relative (the repo is mounted at ``/work``). The
    output ontology is written to a throwaway path inside ``dist/`` that we never
    read — only the reasoner's stdout/stderr verdict matters.
    """
    out = f"dist/.dl-oracle-gold-{reasoner.lower()}.ttl"
    result = run_container(
        ROBOT_IMAGE,
        [
            "robot",
            "reason",
            "--reasoner",
            reasoner,
            "--input",
            rel_input,
            "--output",
            out,
        ],
        check=False,
        timeout=900.0,
    )
    combined = result.stdout + result.stderr
    inconsistent = bool(_INCONSISTENT_RE.search(combined))
    unsat = sorted({m.group(1) for m in _UNSAT_RE.finditer(combined)})
    if not inconsistent and result.returncode not in (0, 1):
        raise RuntimeError(
            f"ROBOT/{reasoner} failed unexpectedly (exit {result.returncode}) "
            f"on {rel_input}:\n{combined}"
        )
    return OracleVerdict(
        reasoner=reasoner,
        consistent=not inconsistent,
        unsatisfiable_classes=unsat,
    )


def _project_root() -> Path:
    return Path(__file__).resolve().parent.parent


def _freeze(dataset: Path) -> dict[str, object]:
    """Run the oracle(s) over one dataset and return its frozen verdict dict."""
    rel = str(dataset.relative_to(_project_root()))
    # HermiT is the sound-and-complete OWL 2 DL oracle: the authority for every
    # beyond-EL construct (∀, cardinality, nominals, complementOf, chains).
    hermit = _run_robot_reason(rel, "hermit")
    # ELK is the fast EL oracle. It is run for cross-reference but is NOT the
    # authority: it silently ignores beyond-EL axioms (e.g. allValuesFrom), so it
    # can report `consistent` where HermiT (correctly) reports inconsistent. We
    # record it but the frozen gold the native gate is checked against is HermiT.
    try:
        elk: OracleVerdict | None = _run_robot_reason(rel, "ELK")
    except RuntimeError:
        # ELK rejects ontologies outside the EL profile outright; that is fine —
        # the dataset is then beyond-EL and HermiT is the sole oracle for it.
        elk = None

    gold: dict[str, object] = {
        "dataset": dataset.name,
        # The AUTHORITATIVE verdict the native path is checked against.
        "oracle": "hermit",
        "consistent": hermit.consistent,
        "unsatisfiable_classes": hermit.unsatisfiable_classes,
        "provenance": {
            "producing_oracle": "ROBOT/HermiT",
            "image": ROBOT_IMAGE,
            "generated_utc": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "dataset_license": "CC-BY-4.0",
            "generator": "scripts/gen_dl_oracle_gold.py (make maint-697-oracle-gold)",
            "note": (
                "Frozen ORACLE verdict. Native Docker-free reasoner is asserted "
                "to reproduce every inconsistency/unsat class (native ⊇ oracle) "
                "offline in crates/conformance. Never hand-edited to match native."
            ),
        },
        "cross_reference": {
            "elk": None
            if elk is None
            else {
                "consistent": elk.consistent,
                "unsatisfiable_classes": elk.unsatisfiable_classes,
            },
            "elk_note": (
                "ELK is the EL oracle, recorded for reference only. It ignores "
                "beyond-EL axioms, so a disagreement with HermiT is expected and "
                "HermiT is authoritative."
            ),
        },
    }
    return gold


def main() -> int:
    """Regenerate every frozen verdict from a real oracle run."""
    datasets = sorted(DATASETS_DIR.glob("*.ttl"))
    if not datasets:
        print(f"no datasets under {DATASETS_DIR}", file=sys.stderr)
        return 1
    for dataset in datasets:
        print(f"[oracle] reasoning {dataset.name} ...", flush=True)
        gold = _freeze(dataset)
        out = GOLD_DIR / "expected" / f"{dataset.stem}.json"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(
            json.dumps(gold, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        verdict = "CONSISTENT" if gold["consistent"] else "INCONSISTENT"
        unsat = gold["unsatisfiable_classes"]
        extra = f", unsat={unsat}" if unsat else ""
        print(f"    -> {verdict}{extra}  (frozen to {out.name})", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
