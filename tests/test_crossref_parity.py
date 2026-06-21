# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Parity tests: Rust ``build_deposit_xml_native`` vs legacy Python oracle.

These tests assert **byte-identity** between the Rust-backed
``gmeow_tools.crossref.build_deposit_xml`` (Task 11, #819) and the frozen
Python snapshot in ``tests/_crossref_legacy.py``. A byte-identity failure
indicates a serialisation divergence that must be fixed before the cutover
is considered complete.

The ``lint_deposit`` parity test asserts **list equality** (same problems, same
order) because the lint result is a deterministic list, not a free-form string.
"""

from __future__ import annotations

import dataclasses
import importlib.util
import sys as _sys
from pathlib import Path

import pytest

_LEGACY_PATH = Path(__file__).with_name("_crossref_legacy.py")
_spec = importlib.util.spec_from_file_location("_crossref_legacy", _LEGACY_PATH)
_legacy_mod = importlib.util.module_from_spec(_spec)  # type: ignore[arg-type]
_sys.modules["_crossref_legacy"] = _legacy_mod  # required for @dataclass slots
_spec.loader.exec_module(_legacy_mod)  # type: ignore[union-attr]
build_deposit_xml_legacy = _legacy_mod.build_deposit_xml_legacy
lint_deposit_legacy = _legacy_mod.lint_deposit_legacy

from gmeow_tools.config import ALIGNMENT_TARGETS  # noqa: E402
from gmeow_tools.crossref import build_deposit_xml, lint_deposit  # noqa: E402
from gmeow_tools.self_desc import load_self_description  # noqa: E402

# Fixed stable inputs so the parity tests are deterministic (the live
# timestamp/batch_id path is covered by
# test_crossref.py::test_default_batch_stamp_is_a_live_submission_timestamp).
FIXED_TIMESTAMP = "20240115120000"
FIXED_BATCH_ID = "test-batch-001"


def _meta_concept_only():
    """Real self-description with version_doi forced to None (concept-only)."""
    return dataclasses.replace(load_self_description(), version_doi=None)


def _meta_with_version():
    """Real self-description with a synthetic version DOI (two-record deposit)."""
    return dataclasses.replace(load_self_description(), version_doi="10.67342/v010")


# ─────────────────────────────────────────────────────────────────────────────
# build_deposit_xml — byte-identity
# ─────────────────────────────────────────────────────────────────────────────


def test_build_deposit_xml_parity_concept_only() -> None:
    """Concept-only deposit is byte-identical between Rust and legacy Python."""
    meta = _meta_concept_only()
    rust = build_deposit_xml(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    legacy = build_deposit_xml_legacy(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    first_diff = next(
        (i for i, (r, p) in enumerate(zip(rust, legacy, strict=False)) if r != p),
        -1,
    )
    assert rust == legacy, (
        f"Byte-identity FAILED (concept-only):\n"
        f"  Rust length: {len(rust)}, Python length: {len(legacy)}\n"
        f"  First diff at char {first_diff}"
    )


def test_build_deposit_xml_parity_with_version_doi() -> None:
    """Two-record deposit (version DOI) is byte-identical."""
    meta = _meta_with_version()
    rust = build_deposit_xml(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    legacy = build_deposit_xml_legacy(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    assert rust == legacy, (
        f"Byte-identity FAILED (with version DOI):\n"
        f"  Rust length: {len(rust)}, Python length: {len(legacy)}"
    )


def test_build_deposit_xml_parity_real_self_description() -> None:
    """Parity with the real (unmodified) self-description from disk."""
    rust = build_deposit_xml(timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID)
    legacy = build_deposit_xml_legacy(
        timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    assert rust == legacy, (
        f"Byte-identity FAILED (real self-description):\n"
        f"  Rust length: {len(rust)}, Python length: {len(legacy)}"
    )


def test_build_deposit_xml_parity_crossmark_disabled(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Parity when Crossmark is disabled (ai:program goes directly under dataset)."""
    import gmeow_tools.crossref as crossref_mod

    monkeypatch.setattr(crossref_mod, "CROSSMARK_ENABLED", False)
    # Also patch the legacy module (loaded at module level as _legacy_mod)
    monkeypatch.setattr(_sys.modules["_crossref_legacy"], "CROSSMARK_ENABLED", False)

    meta = _meta_concept_only()
    rust = build_deposit_xml(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    legacy = build_deposit_xml_legacy(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    assert rust == legacy, "Byte-identity FAILED (Crossmark disabled)"


def test_build_deposit_xml_parity_with_all_alignment_targets() -> None:
    """Parity across the full alignment-target registry (sorted alphabetically).

    This test ensures the Rust side sorts alignment targets the same way Python
    does (sorted(ALIGNMENT_TARGETS) -> alphabetical key order).
    """
    meta = _meta_concept_only()
    rust = build_deposit_xml(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    legacy = build_deposit_xml_legacy(
        meta=meta, timestamp=FIXED_TIMESTAMP, batch_id=FIXED_BATCH_ID
    )
    assert rust == legacy, "Byte-identity FAILED (full alignment targets)"
    # Check that the alignment count matches.
    assert rust.count("<citation ") == len(ALIGNMENT_TARGETS)


# ─────────────────────────────────────────────────────────────────────────────
# lint_deposit — list equality
# ─────────────────────────────────────────────────────────────────────────────


def test_lint_deposit_parity_clean() -> None:
    """Clean self-description: both return empty problem list."""
    assert lint_deposit() == lint_deposit_legacy() == []


def test_lint_deposit_parity_placeholder_doi() -> None:
    """Placeholder DOI detected identically by both implementations."""
    bad = dataclasses.replace(load_self_description(), concept_doi="10.XXXXX/gmeow")
    rust_problems = lint_deposit(bad)
    python_problems = lint_deposit_legacy(bad)
    assert rust_problems == python_problems
    assert any("placeholder" in p for p in rust_problems)


def test_lint_deposit_parity_missing_license() -> None:
    """Missing license URI detected identically."""
    bad = dataclasses.replace(load_self_description(), license_uri="")
    assert lint_deposit(bad) == lint_deposit_legacy(bad)


def test_lint_deposit_parity_missing_wikidata() -> None:
    """Missing registrant Wikidata detected identically."""
    bad = dataclasses.replace(load_self_description(), registrant_wikidata=None)
    assert lint_deposit(bad) == lint_deposit_legacy(bad)
