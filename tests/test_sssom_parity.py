# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Durable native-SSSOM parity test (#848) — no dependency on the ``sssom`` package.

The transitional ``sssom``-py oracle scaffolding (``tests/_sssom_oracle.py``,
``tests/_gen_sssom_golden.py``, ``tests/test_sssom_validation_parity.py``) has been
retired together with the ``sssom`` dependency. What remains is the **durable**
parity argument:

* ``tests/fixtures/lint-golden/sssom_validation.json`` — a frozen snapshot of
  sssom-py (0.4.x) validation behaviour, captured before the native Rust validator
  replaced it.
* ``tests/fixtures/sssom-negative/`` — hand-crafted defective documents.

This test pins the native validator (``gmeow_rdf.validate_sssom``,
``crates/rdf/src/sssom.rs``) against that golden. The negatives carry the parity
argument; the corpus is only a regression sentinel (see below).
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

import gmeow_rdf
import pytest

if TYPE_CHECKING:
    from gmeow_rdf import SssomDiagnostic

PROJECT_ROOT = Path(__file__).resolve().parent.parent
_GOLDEN = json.loads(
    (PROJECT_ROOT / "tests/fixtures/lint-golden/sssom_validation.json").read_text(
        encoding="utf-8"
    )
)
_CORPUS_DIR = PROJECT_ROOT / "generated/mappings"
_NEG_DIR = PROJECT_ROOT / "tests/fixtures/sssom-negative"

_CORPUS = sorted(_CORPUS_DIR.glob("*.sssom.tsv"))
_NEGATIVES = sorted(_NEG_DIR.glob("*.sssom.tsv"))


def _severities(text: str) -> list[str]:
    """Native diagnostic severities for an SSSOM TSV document."""
    return [d["severity"] for d in gmeow_rdf.validate_sssom(text)]


def _blocking(text: str) -> list[SssomDiagnostic]:
    """Native ERROR/FATAL diagnostics (the verdicts a gate would reject on)."""
    return [
        d for d in gmeow_rdf.validate_sssom(text) if d["severity"] in {"ERROR", "FATAL"}
    ]


# ---------------------------------------------------------------------------
# CORPUS — regression sentinel (NOT proof of parity).
#
# Every committed generated/mappings/*.sssom.tsv must validate clean (no ERROR /
# FATAL), matching the golden's empty corpus lists. An empty result is only a
# regression sentinel: it shows the live corpus stays clean as the native
# validator evolves; it does NOT by itself prove the native validator agrees with
# sssom-py (a no-op validator would also pass). The NEGATIVES below carry the
# actual parity argument.
# ---------------------------------------------------------------------------


def test_corpus_nonempty() -> None:
    # Guard against the sweep silently covering nothing.
    assert len(_CORPUS) > 10
    # The frozen golden enumerated the same corpus.
    assert {p.name for p in _CORPUS} == set(_GOLDEN["corpus"])


@pytest.mark.parametrize("path", _CORPUS, ids=lambda p: p.name)
def test_corpus_validates_clean(path: Path) -> None:
    golden = _GOLDEN["corpus"][path.name]
    assert golden == [], f"golden expected {path.name} clean"
    assert _blocking(path.read_text(encoding="utf-8")) == [], (
        f"{path.name}: native validator flagged a clean corpus file (regression)"
    )


# ---------------------------------------------------------------------------
# NEGATIVES — the real parity evidence.
#
# For each defective fixture we encode the EXACT native verdict (read from the
# live validator, not guessed) and document where it MATCHES the frozen sssom-py
# golden and where the native validator is an intentional STRICT SUPERSET
# (greenfield improvement — see the per-case comments).
#
# Native-vs-golden delta map (native verdict | golden verdict | relation):
#   confidence-too-high      ERROR JsonSchema    | ERROR JsonSchema | MATCH
#   confidence-negative      ERROR JsonSchema    | ERROR JsonSchema | MATCH
#   unknown-prefix-subject   ERROR PrefixMap     | ERROR PrefixMap  | MATCH
#   unknown-prefix-object    ERROR PrefixMap     | ERROR PrefixMap  | MATCH
#   unknown-prefix-predicate ERROR PrefixMap     | ERROR PrefixMap  | MATCH
#   missing-subject-id       ERROR RequiredSlot  | [] (dropped)     | STRICT SUPERSET
#   missing-predicate-id     ERROR RequiredSlot  | [] (dropped)     | STRICT SUPERSET
#   missing-object-id        ERROR RequiredSlot  | [] (dropped)     | STRICT SUPERSET
#   invalid-justification-noncurie  []           | []               | MATCH (neither)
#   unparseable-bad-yaml-curie-map  FATAL parse  | FATAL parse_tsv  | MATCH (both fatal)
# ---------------------------------------------------------------------------


def test_negatives_nonempty() -> None:
    assert {p.name for p in _NEGATIVES} == set(_GOLDEN["negatives"])


@pytest.mark.parametrize(
    ("name", "check", "code", "msg_substr"),
    [
        # JsonSchema confidence-range errors — MATCH the golden exactly.
        (
            "confidence-too-high.sssom.tsv",
            "JsonSchema",
            "jsonschema validation",
            "greater than the maximum",
        ),
        (
            "confidence-negative.sssom.tsv",
            "JsonSchema",
            "jsonschema validation",
            "less than the minimum",
        ),
        # PrefixMapCompleteness — MATCH the golden's "Missing prefix: <pfx>".
        (
            "unknown-prefix-subject.sssom.tsv",
            "PrefixMapCompleteness",
            "prefix validation",
            "Missing prefix: nope",
        ),
        (
            "unknown-prefix-object.sssom.tsv",
            "PrefixMapCompleteness",
            "prefix validation",
            "Missing prefix: badpfx",
        ),
        (
            "unknown-prefix-predicate.sssom.tsv",
            "PrefixMapCompleteness",
            "prefix validation",
            "Missing prefix: weird",
        ),
    ],
)
def test_negative_matches_golden(
    name: str, check: str, code: str, msg_substr: str
) -> None:
    """Native verdict equals the frozen sssom-py golden: exactly one ERROR."""
    # The golden recorded exactly one ERROR of this check for these fixtures.
    golden = _GOLDEN["negatives"][name]
    assert len(golden) == 1 and golden[0]["severity"] == "ERROR"

    diags = gmeow_rdf.validate_sssom((_NEG_DIR / name).read_text(encoding="utf-8"))
    assert len(diags) == 1, f"{name}: expected exactly one native diagnostic"
    d = diags[0]
    assert d["severity"] == "ERROR"
    assert d["check"] == check
    assert d["code"] == code
    assert msg_substr in d["message"], d["message"]


@pytest.mark.parametrize(
    "name",
    [
        "missing-subject-id.sssom.tsv",
        "missing-predicate-id.sssom.tsv",
        "missing-object-id.sssom.tsv",
    ],
)
def test_negative_required_slot_is_strict_superset(name: str) -> None:
    """STRICT SUPERSET (deliberate greenfield improvement, NOT a regression).

    sssom-py silently DROPPED rows with a missing required id-column, so the
    golden recorded these fixtures as clean (``[]``). The native validator
    instead raises a ``RequiredSlot`` ERROR — surfacing data loss the upstream
    package hid. We assert the native flag AND that the golden was empty, so the
    superset relationship is pinned, not lost.
    """
    assert _GOLDEN["negatives"][name] == [], (
        f"{name}: golden was expected empty (sssom-py dropped the row)"
    )
    diags = gmeow_rdf.validate_sssom((_NEG_DIR / name).read_text(encoding="utf-8"))
    slot = name.split("missing-")[1].split("-id")[0] + "_id"
    assert len(diags) == 1
    d = diags[0]
    assert d["severity"] == "ERROR"
    assert d["check"] == "RequiredSlot"
    assert d["code"] == "required slot"
    assert f"Missing required slot: {slot}" == d["message"]


def test_negative_invalid_justification_matches_golden_empty() -> None:
    """MATCH: neither sssom-py nor the native validator flags a non-CURIE
    justification here (the golden is empty, and so is the native verdict)."""
    assert _GOLDEN["negatives"]["invalid-justification-noncurie.sssom.tsv"] == []
    diags = gmeow_rdf.validate_sssom(
        (_NEG_DIR / "invalid-justification-noncurie.sssom.tsv").read_text(
            encoding="utf-8"
        )
    )
    assert diags == []


def test_negative_unparseable_is_fatal_like_golden() -> None:
    """MATCH (in severity): both sssom-py and the native parser bail FATAL on the
    unclosed inline ``curie_map`` flow sequence. The golden's message text is
    sssom-py/PyYAML-specific (``ParserError: ...``); the native message is the
    Rust parser's own (``inline curie_map value is not supported``). We pin the
    durable contract — a single FATAL parse diagnostic — not the foreign wording."""
    golden = _GOLDEN["negatives"]["unparseable-bad-yaml-curie-map.sssom.tsv"]
    assert len(golden) == 1 and golden[0]["severity"] == "FATAL"

    diags = gmeow_rdf.validate_sssom(
        (_NEG_DIR / "unparseable-bad-yaml-curie-map.sssom.tsv").read_text(
            encoding="utf-8"
        )
    )
    assert len(diags) == 1
    d = diags[0]
    assert d["severity"] == "FATAL"
    assert d["check"] == "parse"


# ---------------------------------------------------------------------------
# Round-trip / RDF projection — the native codec surfaces the sssom-py package
# never offered here (it validated but emitted no RDF for GMEOW documents).
# ---------------------------------------------------------------------------


def test_sssom_roundtrip_is_stable() -> None:
    """The TSV round-trip is idempotent on a real corpus file."""
    text = _CORPUS[0].read_text(encoding="utf-8")
    once = gmeow_rdf.sssom_roundtrip_tsv(text)
    assert gmeow_rdf.sssom_roundtrip_tsv(once) == once


def test_sssom_to_rdf_nonempty() -> None:
    """The RDF projection of a corpus file is non-empty N-Triples."""
    text = _CORPUS[0].read_text(encoding="utf-8")
    rdf = gmeow_rdf.sssom_to_rdf(text)
    assert rdf.strip(), "sssom_to_rdf returned empty output"


def test_default_validation_types_match_golden() -> None:
    """The native default check set matches the captured sssom-py contract.

    The golden froze sssom-py's default ``validation_types`` (JsonSchema,
    PrefixMapCompleteness, StrictCurieFormat). Asserting it against
    ``sssom_default_validation_types()`` turns any silent drift in the native
    default surface into a test failure (CodeRabbit review #6 on #855); the value
    was previously loaded into the golden but never checked.
    """
    assert (
        gmeow_rdf.sssom_default_validation_types()
        == _GOLDEN["default_validation_types"]
    )
