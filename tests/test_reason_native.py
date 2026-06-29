# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native-reasoning **report-wrapper** tests (`gmeow_tools.reason`, #665 / #695).

What remains here is the thin Python orchestration layer `gmeow_tools.reason`
exercises that genuinely belongs to Python — discovering verify-query files,
handing their text to the Rust core, folding the returned diagnostics into the
Python report, and writing JSON / SARIF / HTML artifacts to disk. The tests stub
the native core so pytest does not duplicate the full-bundle chases already run by
the normal `make check` `reason` and `verify` targets.

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

_CLOSURE_TTL = """@prefix ex: <https://example.org/> .

ex:s ex:p ex:o .
"""


def _native_reason_result() -> dict[str, object]:
    """Minimal `gmeow_logic.reason_native_with_artifacts` result for wrapper tests."""
    return {
        "consistent": True,
        "inferred": [
            {
                "subject": "https://example.org/s",
                "predicate": "https://example.org/p",
                "object": "https://example.org/o",
                "world": "https://example.org/w",
                "is_edb": False,
                "rule_name": "test:rule",
            }
        ],
        "unsatisfiable_classes": [],
        "inconsistencies": [],
        "coverage": {"present": [], "decided": [], "unsupported": []},
        "gaps": [],
        "status": {
            "input": "accepted",
            "evaluation": "completed",
            "completeness": "complete-for-fragment",
            "information": "supported",
        },
        "preservation": {"polarities": ["exact"], "unsupported_constructs": []},
        "provenance": {
            "contract_hash": "test",
            "engine_name": "gmeow-test",
            "engine_version": "0",
            "consumed_budget": 0,
        },
        "artifacts": {
            "closure": _CLOSURE_TTL,
            "explanations": "",
            "ledger": "",
            "result": "",
        },
    }


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
    """``reason.reason_native`` report + artifact writing."""

    def test_report_ok_and_writes_closure(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        """A consistent native result yields ``report.ok`` and writes the closure."""
        import gmeow_logic

        import gmeow_tools.reason as reason

        bundle = tmp_path / "fixture.gts"
        bundle.write_bytes(b"bundle")
        calls: list[tuple[bytes, bool]] = []

        def fake_reason_native_with_artifacts(
            gts_bytes: bytes, merge: bool = False
        ) -> dict[str, object]:
            calls.append((gts_bytes, merge))
            return _native_reason_result()

        monkeypatch.setattr(
            gmeow_logic,
            "reason_native_with_artifacts",
            fake_reason_native_with_artifacts,
        )

        report = reason.reason_native(
            gts=bundle, output_dir=tmp_path, run_box_roles=False
        )

        assert calls == [(b"bundle", False)]
        assert report.ok, "consistent bundle must report ok"
        closure = tmp_path / "gmeow-inferred-closure.rdf12.ttl"
        assert closure.exists(), "closure artifact was not written"
        # The closure must parse as valid RDF 1.2 with non-empty content.
        assert _parse_rdf12_turtle(closure.read_text(encoding="utf-8")) > 0


class TestVerifyNative:
    """``reason.verify_native`` — native reasoned-graph verify (#695)."""

    def test_clean_report_discovers_queries_and_writes_artifacts(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        """The wrapper passes query text to Rust and writes JSON/SARIF/HTML."""
        import gmeow_logic

        import gmeow_tools.reason as reason
        from gmeow_tools import diagnostics

        bundle = tmp_path / "fixture.gts"
        bundle.write_bytes(b"bundle")
        queries = tmp_path / "queries"
        queries.mkdir()
        query = queries / "clean.rq"
        query.write_text("SELECT ?s WHERE { ?s ?p ?o }\n", encoding="utf-8")
        calls: list[tuple[bytes, list[tuple[str, str]]]] = []

        def fake_verify_native(
            gts_bytes: bytes, pairs: list[tuple[str, str]]
        ) -> diagnostics.DiagnosticsReport:
            calls.append((gts_bytes, pairs))
            return diagnostics.report(tool="verify")

        monkeypatch.setattr(gmeow_logic, "verify_native", fake_verify_native)
        monkeypatch.setattr(reason, "PROJECT_ROOT", tmp_path)

        report = reason.verify_native(gts=bundle, queries=queries, output_dir=tmp_path)

        assert calls == [
            (b"bundle", [("queries/clean.rq", query.read_text(encoding="utf-8"))])
        ]
        assert report.ok, "clean native report must pass"
        assert report.error_count == 0
        for suffix in ("json", "sarif", "html"):
            artifact = tmp_path / f"gmeow-verify-native.{suffix}"
            assert artifact.exists(), f"{artifact.name} was not written"
