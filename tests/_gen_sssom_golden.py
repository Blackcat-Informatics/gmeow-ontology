# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Regenerate the SSSOM-validation golden snapshot (#848, Task 1).

Captures the *current* ``sssom`` Python validation behaviour so the future
native Rust validator can be proven to match it. Idempotent: running it twice
produces byte-identical output.

It does three things, all via :mod:`tests._sssom_oracle` (the transient oracle
that wraps the real ``sssom`` package):

1. Records ``DEFAULT_VALIDATION_TYPES`` — the exact check set ``validate`` runs
   when ``validation_types=None`` (what production passes).
2. Runs the oracle over every committed ``generated/mappings/*.sssom.tsv`` and
   records the verdict per file. These drift-gated artifacts MUST validate
   clean; if any is non-empty the script **aborts** (a dirty corpus is a bug to
   fix upstream, not to bake into the golden).
3. (Re)writes the hand-crafted negative fixtures under
   ``tests/fixtures/sssom-negative/`` — each mutates exactly ONE thing from a
   real corpus header so the failure is isolated — runs the oracle over each,
   and records the verdict.

Output: ``tests/fixtures/lint-golden/sssom_validation.json``.

Run with::

    uv run python tests/_gen_sssom_golden.py

(from the repository root; uses the project venv so the pinned ``sssom`` is the
oracle).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

_TESTS_DIR = Path(__file__).resolve().parent
# Make `import tests._sssom_oracle` resolve when this file is run as a script
# (`uv run python tests/_gen_sssom_golden.py`), where the repo root is not yet
# on sys.path. Under pytest the package is already importable.
_REPO_ROOT_FOR_IMPORT = _TESTS_DIR.parent
if str(_REPO_ROOT_FOR_IMPORT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT_FOR_IMPORT))

from tests._sssom_oracle import (  # noqa: E402 — needs sys.path tweak above
    OracleResult,
    default_validation_types,
    validate_sssom_text,
)

_REPO_ROOT = _TESTS_DIR.parent
_MAPPINGS_DIR = _REPO_ROOT / "generated" / "mappings"
_NEGATIVE_DIR = _TESTS_DIR / "fixtures" / "sssom-negative"
_GOLDEN_PATH = _TESTS_DIR / "fixtures" / "lint-golden" / "sssom_validation.json"

# Base corpus file every negative fixture is mutated from. Chosen because it is
# small, stable, and exercises subject/predicate/object/confidence/justification.
_BASE_FILE = _REPO_ROOT / "generated" / "mappings" / "gmeow-accessibility.sssom.tsv"


# --------------------------------------------------------------------------- #
# Negative-fixture construction
# --------------------------------------------------------------------------- #


def _base_lines() -> tuple[list[str], int, list[str]]:
    """Return (lines, header_index, columns) of the base corpus file."""
    text = _BASE_FILE.read_text(encoding="utf-8")
    lines = text.splitlines()
    header_idx = next(i for i, ln in enumerate(lines) if not ln.startswith("#"))
    columns = lines[header_idx].split("\t")
    return lines, header_idx, columns


def _mutate_cell(column: str, value: str, *, row: int = 1) -> str:
    """Return the base file with one data-cell replaced (``row`` is 1-based)."""
    lines, header_idx, columns = _base_lines()
    col_idx = columns.index(column)
    cells = lines[header_idx + row].split("\t")
    cells[col_idx] = value
    lines[header_idx + row] = "\t".join(cells)
    return "\n".join(lines) + "\n"


def _blank_cell(column: str, *, row: int = 1) -> str:
    return _mutate_cell(column, "", row=row)


def _build_negatives() -> dict[str, str]:
    """Build the negative-fixture {filename: tsv-text} map.

    Each entry mutates exactly ONE thing. Grouped by what they prove:

    * **Reachable validator failures** — defects that survive ``parse_tsv`` and
      are caught by a default check (``JsonSchema`` numeric range,
      ``PrefixMapCompleteness`` unknown prefix). These are the primary parity
      evidence: the native validator MUST flag them.
    * **Parse pre-filter drops** — defects that ``parse_tsv`` silently discards
      (blank required field, non-CURIE entity reference). They produce ZERO
      validation results, which is itself a behavioural contract the native
      path must honour: a malformed row is excluded, not validated.
    * **Hard parse failure** — input ``parse_tsv`` cannot read at all, recorded
      as the synthetic ``parse error`` class.
    """
    return {
        # --- JsonSchema: confidence out of [0.0, 1.0] -----------------------
        "confidence-too-high.sssom.tsv": _mutate_cell("confidence", "1.5"),
        "confidence-negative.sssom.tsv": _mutate_cell("confidence", "-0.5"),
        # --- PrefixMapCompleteness: CURIE prefix not in curie_map -----------
        "unknown-prefix-subject.sssom.tsv": _mutate_cell("subject_id", "nope:Foo"),
        "unknown-prefix-object.sssom.tsv": _mutate_cell("object_id", "badpfx:Thing"),
        "unknown-prefix-predicate.sssom.tsv": _mutate_cell(
            "predicate_id", "weird:relatesTo"
        ),
        # --- parse pre-filter drops (expected EMPTY verdict) ---------------
        # Blank required slots: the row is dropped before validate() runs.
        "missing-subject-id.sssom.tsv": _blank_cell("subject_id"),
        "missing-predicate-id.sssom.tsv": _blank_cell("predicate_id"),
        "missing-object-id.sssom.tsv": _blank_cell("object_id"),
        # Non-CURIE mapping_justification: also dropped at parse.
        "invalid-justification-noncurie.sssom.tsv": _mutate_cell(
            "mapping_justification", "just some prose, not a curie"
        ),
        # --- hard parse failure (parse-error class) ------------------------
        # Malformed YAML in the curie_map header: parse_tsv raises.
        "unparseable-bad-yaml-curie-map.sssom.tsv": (
            "# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/broken\n"
            "# curie_map: [unclosed\n"
            "subject_id\tpredicate_id\tobject_id\n"
            "gmeow:A\tskos:exactMatch\tgmeow:B\n"
        ),
    }


# --------------------------------------------------------------------------- #
# Golden assembly
# --------------------------------------------------------------------------- #


def _corpus_section() -> dict[str, list[OracleResult]]:
    files = sorted(_MAPPINGS_DIR.glob("*.sssom.tsv"))
    if not files:
        raise SystemExit(f"no corpus files matched {_MAPPINGS_DIR}/*.sssom.tsv")
    section: dict[str, list[OracleResult]] = {}
    dirty: list[tuple[str, list[OracleResult]]] = []
    for path in files:
        name = path.name
        results = validate_sssom_text(path.read_text(encoding="utf-8"))
        section[name] = results
        if results:
            dirty.append((name, results))
    if dirty:
        lines = "\n".join(f"  {n}: {json.dumps(r)}" for n, r in dirty)
        raise SystemExit(
            "ABORT: committed corpus is DIRTY — these generated SSSOM files do "
            "not validate clean under sssom-py. Fix the upstream artifact, do "
            f"not bake the failure into the golden:\n{lines}"
        )
    return section


def _negatives_section() -> dict[str, list[OracleResult]]:
    _NEGATIVE_DIR.mkdir(parents=True, exist_ok=True)
    negatives = _build_negatives()
    section: dict[str, list[OracleResult]] = {}
    weak: list[str] = []
    for name, text in negatives.items():
        (_NEGATIVE_DIR / name).write_text(text, encoding="utf-8")
        results = validate_sssom_text(text)
        section[name] = results
        # Fixtures whose name does NOT advertise a "drop"/"parse" outcome are
        # expected to actually trip a check.
        is_expected_empty = name.startswith(("missing-", "invalid-"))
        if not is_expected_empty and not results:
            weak.append(name)
    if weak:
        raise SystemExit(
            "ABORT: these negative fixtures did not trip any ERROR/FATAL but "
            f"were expected to: {weak}. Strengthen them or rename to document "
            "the parse-drop behaviour."
        )
    return section


def main() -> None:
    golden = {
        "_comment": (
            "Golden snapshot of sssom-py (0.4.x) validation behaviour, captured "
            "by tests/_gen_sssom_golden.py via tests/_sssom_oracle.py BEFORE the "
            "native Rust validator replaces the sssom dependency (#848). The "
            "native validator must reproduce this exactly. Regenerate with: "
            "uv run python tests/_gen_sssom_golden.py"
        ),
        "default_validation_types": default_validation_types(),
        "corpus": _corpus_section(),
        "negatives": _negatives_section(),
    }
    _GOLDEN_PATH.parent.mkdir(parents=True, exist_ok=True)
    _GOLDEN_PATH.write_text(
        json.dumps(golden, indent=2, sort_keys=False, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )
    n_corpus = len(golden["corpus"])
    n_neg = len(golden["negatives"])
    print(f"default_validation_types: {golden['default_validation_types']}")
    print(f"corpus: {n_corpus} files, all clean")
    print(f"negatives: {n_neg} fixtures")
    print(f"wrote {_GOLDEN_PATH}")


if __name__ == "__main__":
    main()
