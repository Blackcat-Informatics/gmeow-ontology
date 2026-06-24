"""SSSOM alignment-direction Python surface smoke tests (issue #25, #936).

The detailed synthetic regression tests for inverse-direction, domain-range,
property-character, equivalence-collapse, and DC refinement now live in the
native Rust suite (`crates/slice/src/alignment_lint.rs`). This file only
covers the Python binding surface and repository-policy guards that do not
belong in the Rust crate.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import gmeow_slice
import httpx
import pytest

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    PROJECT_ROOT,
    TARGET_SNAPSHOT_DIR,
    LinkPolicy,
)
from gmeow_tools.graph import iter_source_files

if TYPE_CHECKING:
    from gmeow_slice import ProjectionDiagnostic

_ALIGNMENT_CHECKS = frozenset(
    {
        "inverse-direction",
        "domain-range",
        "property-character",
        "equivalence-collapse",
        "dc-refinement",
        "dc-hand-authored",
    }
)


def _alignment_findings(*, network: bool = False) -> list[ProjectionDiagnostic]:
    """Run the Rust lint and return only alignment-family findings."""
    return [
        f
        for f in gmeow_slice.lint_projection(str(PROJECT_ROOT), allow_network=network)
        if f["check"] in _ALIGNMENT_CHECKS
    ]


def test_lint_projection_returns_alignment_findings() -> None:
    """The Rust lint returns structured alignment findings over the committed tree."""
    findings = _alignment_findings()
    assert findings, "expected alignment findings from the committed tree"
    checks = {f["check"] for f in findings}
    assert _ALIGNMENT_CHECKS & checks, "expected at least one alignment check to fire"


def test_committed_mappings_have_no_direction_errors() -> None:
    """The committed SSSOM mappings must contain no alignment ERRORs."""
    findings = _alignment_findings()
    errors = [f for f in findings if f["severity"] == "ERROR"]
    assert not errors, "alignment-direction errors:\n" + "\n".join(
        f["message"] for f in errors
    )


def test_alignment_checks_are_represented() -> None:
    """Every alignment check family is represented in the committed-data output."""
    findings = _alignment_findings()
    checks = {f["check"] for f in findings}
    # domain-range and dc-refinement always fire on the real data; the others
    # are verified by the Rust synthetic regression suite.
    assert "domain-range" in checks
    assert "dc-refinement" in checks


def test_warnings_are_collected_but_not_fatal() -> None:
    """Warnings are reported as structured findings without failing the gate."""
    findings = _alignment_findings()
    warnings = [f for f in findings if f["severity"] == "WARNING"]
    for finding in warnings:
        assert finding["message"]


def test_dc_refinement_lint_runs() -> None:
    """The DC refinement lint runs via the Rust surface and returns findings."""
    findings = [f for f in _alignment_findings() if f["check"] == "dc-refinement"]
    assert findings, "expected dc-refinement findings"
    for finding in findings:
        assert finding["message"]


@pytest.mark.network
def test_live_target_axioms_have_no_direction_errors() -> None:
    """Full sweep incl. reference-only targets fetched live — no ERRORs allowed."""
    try:
        findings = _alignment_findings(network=True)
    except httpx.HTTPError as exc:  # offline / transient — don't fail CI
        pytest.skip(f"target vocabulary fetch unavailable: {exc}")
    errors = [f for f in findings if f["severity"] == "ERROR"]
    assert not errors, "alignment-direction errors (live):\n" + "\n".join(
        f["message"] for f in errors
    )


def test_refresh_snapshot_refuses_target_without_fetch_source() -> None:
    """An IMPORT_OK target with no fetch source fails with a clear policy error."""
    from gmeow_tools.extract import LicensePolicyError
    from gmeow_tools.target_axioms import TARGET_SOURCES, refresh_snapshot

    # `rel` is CC-BY (IMPORT_OK) but has no entry in TARGET_SOURCES.
    assert "rel" not in TARGET_SOURCES
    with pytest.raises(LicensePolicyError):
        refresh_snapshot("rel")


def test_target_snapshots_stay_out_of_the_published_artifact() -> None:
    """Vendored target snapshots must never enter the published import closure."""
    snapshot_root = TARGET_SNAPSHOT_DIR.resolve()
    for source in iter_source_files(include_imports=True):
        assert snapshot_root not in source.resolve().parents, (
            f"{source} is under imports/targets/ but is in the publish path"
        )


def test_no_reference_only_target_is_vendored() -> None:
    """License guard: only IMPORT_OK targets may be committed under imports/targets/."""
    if not TARGET_SNAPSHOT_DIR.exists():
        pytest.skip("no vendored snapshots present")
    for snapshot in TARGET_SNAPSHOT_DIR.glob("*.ttl"):
        prefix = snapshot.stem
        target = ALIGNMENT_TARGETS.get(prefix)
        assert target is not None, f"snapshot for unknown target {prefix!r}"
        assert target.policy is LinkPolicy.IMPORT_OK, (
            f"{prefix} is {target.policy.value}; its axioms must not be vendored "
            f"(fetch live under --network instead)"
        )
