# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native-reasoning **report-wrapper** tests (`gmeow_tools.reason`, #665 / #695).

What remains here is the thin Python orchestration layer `gmeow_tools.reason`
exercises that genuinely belongs to Python — discovering the repo + per-slice
verify-query files, handing their text to the Rust core, and writing the
diagnostics report's JSON / SARIF / HTML artifacts to disk. These are an
independent live Python surface with no Rust twin (doctrine-guard), so they are
retained.

The engine-direct tests that used to live here — the `gmeow_logic.reason_native`
result structure, the `gmeow_logic.verify_native` violation path, the three
`reason_native_artifacts` structural-token checks, and the byte-regenerable
artifact pin — were migrated to / subsumed by their native Rust twins under issue
#896 (the python lane no longer pays those full-bundle chases). Their coverage now
lives in:

* `crates/logic/src/reason/mod.rs` — `reason_all` (consistency + non-empty closure)
* `crates/logic/src/verify.rs` — `violating_query_yields_error_finding_with_detail`
  (the verify violation path, same code + semantics)
* `crates/logic/src/reason/artifacts.rs` — the closure / explanations / ledger
  structural-token tests (`closure_emits_triple_and_reifier_with_provenance`,
  `explanations_emit_derivation_with_premise`, `ledger_header_entries_gaps_and_counts`)
* the `ontology-generated` CI lane (`gmeow-dev check-generated`) + `crates/pipeline/
  tests/full_parity.rs` — the byte-regeneration of the committed `generated/logic/*.ttl`
  artifacts through the SAME `reason_all → build_*_ttl` path

See `dsl/tests/MIGRATION-LEDGER.md`.
"""

from __future__ import annotations

from pathlib import Path

import pytest


def _parse_rdf12_turtle(ttl: str) -> int:
    """Parse an RDF 1.2 Turtle document with gmeow_rdf; return the triple count.

    The native-lane artifacts carry RDF 1.2 ``<< … >>`` triple terms, which
    rdflib's Turtle parser cannot read — so gmeow_rdf (not rdflib) is the only
    parser the contract permits here.
    """
    import gmeow_rdf

    dataset = gmeow_rdf.Dataset()
    for quad in gmeow_rdf.parse(ttl.encode("utf-8"), format=gmeow_rdf.RdfFormat.TURTLE):
        dataset.add(gmeow_rdf.Quad(quad.subject, quad.predicate, quad.object))
    return len(dataset)


class TestReasonNativePipeline:
    """``reason.reason_native`` report + artifact writing over the real bundle."""

    def test_report_ok_and_writes_closure(self, tmp_path: Path) -> None:
        """A consistent bundle yields ``report.ok`` and writes the closure artifact."""
        import gmeow_tools.reason as reason
        from gmeow_tools.config import GTS_SNAPSHOT_FILE

        if not GTS_SNAPSHOT_FILE.exists():
            pytest.skip("GTS snapshot not present in this checkout")

        report = reason.reason_native(output_dir=tmp_path, run_box_roles=False)
        assert report.ok, "consistent bundle must report ok"
        closure = tmp_path / "gmeow-inferred-closure.rdf12.ttl"
        assert closure.exists(), "closure artifact was not written"
        # The closure must parse as valid RDF 1.2 with non-empty content.
        assert _parse_rdf12_turtle(closure.read_text(encoding="utf-8")) > 0


class TestVerifyNative:
    """``reason.verify_native`` — native reasoned-graph verify (#695)."""

    def test_clean_over_bundle_and_writes_artifacts(self, tmp_path: Path) -> None:
        """The committed bundle passes verify and writes JSON/SARIF/HTML."""
        import gmeow_tools.reason as reason
        from gmeow_tools.config import GTS_SNAPSHOT_FILE

        if not GTS_SNAPSHOT_FILE.exists():
            pytest.skip("GTS snapshot not present in this checkout")

        report = reason.verify_native(output_dir=tmp_path)
        assert report.ok, "committed ontology must pass its own verify queries"
        assert report.error_count == 0
        for suffix in ("json", "sarif", "html"):
            artifact = tmp_path / f"gmeow-verify-native.{suffix}"
            assert artifact.exists(), f"{artifact.name} was not written"
