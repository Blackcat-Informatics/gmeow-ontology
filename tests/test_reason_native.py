# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native EL/DL reasoning + verify tests (#665 / #695 / #666).

These tests exercise the Rust ``gmeow_logic.reason_native`` /
``gmeow_logic.verify_native`` / ``gmeow_logic.reason_native_artifacts`` surfaces
and the ``gmeow_tools.reason`` report wrappers. They were extracted verbatim from
the retired ``tests/test_logic_runner.py`` when the Python conformance runner was
removed (#785, T4): they never depended on ``logic_runner`` — only on the native
reasoning/verify engine, which is NOT being retired.
"""

from __future__ import annotations

import functools
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


class TestReasonNativeEngine:
    """``gmeow_logic.reason_native`` over the committed GTS bundle (#665)."""

    def test_consistent_with_entailments_and_gaps(self) -> None:
        """The bundle reasons consistent, derives axioms, and names beyond-EL gaps."""
        import gmeow_logic

        from gmeow_tools.config import GTS_SNAPSHOT_FILE

        if not GTS_SNAPSHOT_FILE.exists():
            pytest.skip("GTS snapshot not present in this checkout")

        result = gmeow_logic.reason_native(GTS_SNAPSHOT_FILE.read_bytes())
        assert result["consistent"] is True
        assert len(result["inferred"]) > 0, "native lane produced no entailments"
        # The beyond-EL DL gaps must be named (not silently assumed consistent).
        assert len(result["gaps"]) > 0, "expected named beyond-EL gaps, got none"
        # Each gap carries a code + message (the diagnostics-finding contract).
        for gap in result["gaps"]:
            assert gap.get("code") and gap.get("message")
        # Each inferred entailment carries the full provenance tuple.
        sample = result["inferred"][0]
        for key in ("subject", "predicate", "object", "world", "is_edb", "rule_name"):
            assert key in sample, f"inferred entailment missing key {key!r}: {sample}"


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

    def test_violating_query_reports_error(self) -> None:
        """A query that returns rows over the bundle yields an error report."""
        import gmeow_logic

        from gmeow_tools.config import GTS_SNAPSHOT_FILE

        if not GTS_SNAPSHOT_FILE.exists():
            pytest.skip("GTS snapshot not present in this checkout")

        # Every class is a row → guaranteed non-empty → a "violation" by the
        # negative-test convention; exercises the PyO3 live-report violation path.
        tripping = (
            "queries/verify/_synthetic-every-class.rq",
            "SELECT ?c WHERE { ?c a <http://www.w3.org/2002/07/owl#Class> }",
        )
        report = gmeow_logic.verify_native(GTS_SNAPSHOT_FILE.read_bytes(), [tripping])
        assert not report.ok, "a returned row must fail the report"
        assert report.error_count >= 1
        codes = {f["code"] for f in report.findings}
        assert "verify._synthetic-every-class" in codes, codes


@functools.cache
def _native_artifacts() -> dict[str, str]:
    """Emit the three native RDF 1.2 artifacts once, cached across the tests.

    The full native chase is expensive; all three artifacts come from ONE Rust
    ``reason_native_artifacts`` call (engine reasons once, serializes via the
    gmeow-rdf RDF 1.2 Turtle emitter), so a single cached run is shared by every
    structure test instead of recomputing the whole pipeline per method. (A skip
    when the snapshot is absent raises before returning, so nothing is cached.)
    """
    import gmeow_logic

    from gmeow_tools.config import GTS_SNAPSHOT_FILE

    if not GTS_SNAPSHOT_FILE.exists():
        pytest.skip("GTS snapshot not present in this checkout")
    return gmeow_logic.reason_native_artifacts(GTS_SNAPSHOT_FILE.read_bytes(), False)


class TestNativeReasonArtifacts:
    """The native (Rust) RDF-1.2-Turtle artifacts parse under gmeow_rdf (#666).

    Task 3 moved the closure / explanations / ledger emission off the Python
    primary path into ``gmeow_logic.reason_native_artifacts`` (Rust + the
    gmeow-rdf RDF 1.2 Turtle emitter). These tests assert the three artifacts
    parse as valid RDF 1.2 and carry their key structural tokens.
    """

    def _artifacts(self) -> dict[str, str]:
        return _native_artifacts()

    def test_closure_parses_and_carries_reifier_provenance(self) -> None:
        ttl = self._artifacts()["closure"]
        assert _parse_rdf12_turtle(ttl) > 0
        # Derived axioms carry an RDF 1.2 reifier with derivation provenance.
        assert "rdf:reifies" in ttl or "22-rdf-syntax-ns#reifies" in ttl
        assert "viaRule" in ttl
        assert "Deduction" in ttl

    def test_explanations_parse_and_carry_derivation_skeleton(self) -> None:
        ttl = self._artifacts()["explanations"]
        assert _parse_rdf12_turtle(ttl) > 0
        # Each proof skeleton is a Derivation that concludes a triple term.
        assert "Derivation" in ttl
        assert "concludes" in ttl
        # Multi-step derivations cite their premises (now exposed by the engine).
        assert "hasPremise" in ttl

    def test_ledger_parses_and_carries_entries_gaps_and_counts(self) -> None:
        ttl = self._artifacts()["ledger"]
        assert _parse_rdf12_turtle(ttl) > 0
        # The report-only crosscheck ledger header + verdict (the native emitter
        # writes full IRIs, so the consistency verdict is a `consistent> true`
        # property — not the prefixed `gmeow:consistent true` form).
        assert "CrosscheckLedger" in ttl
        assert "consistent> true" in ttl
        # Native-only subsumption entries and beyond-EL DL gaps.
        assert "LedgerEntry" in ttl
        assert "DlGap" in ttl
        # The report-only / #666 oracle-deferral note is present.
        assert "classic-cross-check" in ttl
        assert "#666" in ttl
        # Counts are emitted.
        assert "entailmentCount" in ttl
        assert "gapCount" in ttl

    def test_artifacts_are_byte_regenerable_against_committed(self) -> None:
        """The Rust-emitted artifacts are RDF-isomorphic to the committed files.

        Mirrors ``NativeReasoningGenerator.compare`` (gmeow_rdf RDFC-1.0
        canonical quad-set equality): a fresh native emission must equal the
        committed ``generated/logic/*.ttl`` so the drift gate stays green.
        """
        import gmeow_rdf

        from gmeow_tools.config import GENERATED_DIR

        def _canon(text: str) -> list[str]:
            dataset = gmeow_rdf.Dataset()
            for quad in gmeow_rdf.parse(
                text.encode("utf-8"), format=gmeow_rdf.RdfFormat.TURTLE
            ):
                dataset.add(gmeow_rdf.Quad(quad.subject, quad.predicate, quad.object))
            dataset.canonicalize(gmeow_rdf.CanonicalizationAlgorithm.RDFC_1_0)
            return sorted(str(quad) for quad in dataset)

        artifacts = self._artifacts()
        committed = {
            "closure": GENERATED_DIR / "logic" / "inferred-closure.rdf12.ttl",
            "explanations": GENERATED_DIR
            / "logic"
            / "reasoning-explanations.rdf12.ttl",
            "ledger": GENERATED_DIR / "logic" / "dl-el-crosscheck-report.ttl",
        }
        for key, path in committed.items():
            # The artifacts are git-tracked, so absence is a real regression
            # (a deleted committed output), not a fresh-checkout condition —
            # the drift gate must fail closed, never skip past missing outputs.
            assert path.exists(), (
                f"committed artifact missing; drift gate fails closed: {path}"
            )
            assert _canon(artifacts[key]) == _canon(path.read_text(encoding="utf-8")), (
                f"{key} drifted from committed {path.name}"
            )
