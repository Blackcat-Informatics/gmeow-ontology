# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Durable parity guard for SSSOM validation (#848, Task 1).

The golden ``tests/fixtures/lint-golden/sssom_validation.json`` was captured
from the *current* ``sssom`` Python package (via :mod:`tests._sssom_oracle`)
BEFORE #848 replaces that dependency with a native Rust validator. This test
pins the captured behaviour two ways:

* While ``sssom`` is still installed, the live oracle must still reproduce the
  golden exactly (catches accidental drift / a corpus regeneration).
* The golden's shape (default check set, clean corpus, tripping negatives) is
  asserted so the *native* validator's future parity test has an unambiguous
  contract to match — it can target this same file once the oracle is gone.

When the native validator lands, add the Rust-path assertions here against the
same golden, then retire the oracle import and the ``sssom`` dependency.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from tests._sssom_oracle import default_validation_types, validate_sssom_text

_REPO_ROOT = Path(__file__).resolve().parent.parent
_GOLDEN = json.loads(
    (
        Path(__file__).parent / "fixtures" / "lint-golden" / "sssom_validation.json"
    ).read_text(encoding="utf-8")
)
_NEGATIVE_DIR = Path(__file__).parent / "fixtures" / "sssom-negative"

# Negative fixtures whose documented contract is the parse pre-filter DROP
# (zero validation results), as opposed to actually tripping a default check.
_EXPECTED_EMPTY = {
    "missing-subject-id.sssom.tsv",
    "missing-predicate-id.sssom.tsv",
    "missing-object-id.sssom.tsv",
    "invalid-justification-noncurie.sssom.tsv",
}


def test_default_validation_types_match_golden() -> None:
    """The check set the native validator must run with ``validation_types=None``."""
    assert default_validation_types() == _GOLDEN["default_validation_types"]
    # Pin the exact expected set so a sssom upgrade that adds a default check is
    # caught loudly rather than silently widening the contract.
    assert _GOLDEN["default_validation_types"] == [
        "JsonSchema",
        "PrefixMapCompleteness",
        "StrictCurieFormat",
    ]


def test_corpus_is_clean_and_matches_golden() -> None:
    """Every committed generated SSSOM file validates clean, matching the golden."""
    for name, expected in _GOLDEN["corpus"].items():
        path = _REPO_ROOT / "generated" / "mappings" / name
        assert path.exists(), f"corpus file vanished: {name}"
        actual = validate_sssom_text(path.read_text(encoding="utf-8"))
        assert actual == expected == [], f"corpus drift on {name}: {actual}"


def test_corpus_section_covers_all_committed_files() -> None:
    committed = sorted(
        p.name for p in (_REPO_ROOT / "generated" / "mappings").glob("*.sssom.tsv")
    )
    assert sorted(_GOLDEN["corpus"]) == committed
    assert len(committed) == 66


@pytest.mark.parametrize("name", sorted(_GOLDEN["negatives"]))
def test_negative_fixture_matches_golden(name: str) -> None:
    path = _NEGATIVE_DIR / name
    assert path.exists(), f"negative fixture missing: {name}"
    actual = validate_sssom_text(path.read_text(encoding="utf-8"))
    assert actual == _GOLDEN["negatives"][name], f"negative drift on {name}"


@pytest.mark.parametrize("name", sorted(_GOLDEN["negatives"]))
def test_negative_fixture_outcome_is_consistent(name: str) -> None:
    """Each negative either trips >=1 ERROR/FATAL or is a documented parse-drop."""
    results = _GOLDEN["negatives"][name]
    if name in _EXPECTED_EMPTY:
        assert results == [], f"{name} was expected to be a parse-drop (empty)"
    else:
        assert results, f"{name} must produce at least one ERROR/FATAL"
        assert all(r["severity"] in ("ERROR", "FATAL") for r in results)
