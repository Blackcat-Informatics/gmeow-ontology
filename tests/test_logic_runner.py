# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for :mod:`gmeow_tools.logic_runner` (issue #501, Task 7).

Covers:
* compare_rdf: match on isomorphic graphs; mismatch detected.
* compare_canonical_json: key-order independence; match/mismatch.
* compare_explanation_skeleton: cited-IRI set equality; prose irrelevance;
  mismatch caught on changed IRI.
* run() smoke test over each projection case: outputs non-empty projections,
  verdicts, witnesses == {}, answers == {}.
* discover_cases: finds the projection cases, hard-fails on missing fields.
* diff_case: passes over projection cases (actual == expected golden files).
"""

from __future__ import annotations

import functools
import json
from pathlib import Path

import pytest
from rdflib import RDF, Graph, Namespace

from gmeow_tools.logic_runner import (
    RunnerError,
    RunnerOutputs,
    _parse_cited_iri_skeleton,
    _parse_explanation_reifier,
    compare_canonical_json,
    compare_explanation_skeleton,
    compare_rdf,
    diff_case,
    discover_cases,
    run,
)

# --------------------------------------------------------------------------- #
# Repository paths
# --------------------------------------------------------------------------- #

_REPO_ROOT = Path(__file__).resolve().parents[1]
_CONFORMANCE_ROOT = _REPO_ROOT / "conformance" / "logic"
_PROJECTION_CASES_ROOT = _CONFORMANCE_ROOT / "cases" / "projections"


def _projection_cases() -> list[Path]:
    """Return all projection case directories that have the required files."""
    if not _PROJECTION_CASES_ROOT.is_dir():
        return []
    return sorted(
        p
        for p in _PROJECTION_CASES_ROOT.iterdir()
        if p.is_dir()
        and (p / "input.logic.ttl").exists()
        and (p / "profile.json").exists()
    )


_ALL_PROJECTION_CASES = _projection_cases()
_PROJECTION_CASE_IDS = [c.name for c in _ALL_PROJECTION_CASES]


# --------------------------------------------------------------------------- #
# compare_rdf tests
# --------------------------------------------------------------------------- #


class TestCompareRdf:
    """Unit tests for :func:`compare_rdf`."""

    def test_identical_graphs_match(self) -> None:
        """Two identical graphs must produce an empty diff list."""
        ex = Namespace("https://example.org/")
        g1 = Graph()
        g2 = Graph()
        g1.add((ex.A, RDF.type, ex.B))
        g2.add((ex.A, RDF.type, ex.B))
        result = compare_rdf(g1, g2)
        assert result == [], f"Expected empty diff for identical graphs, got: {result}"

    def test_isomorphic_blank_node_graphs_match(self) -> None:
        """Two isomorphic graphs (same structure, different BNode IDs) must match."""
        from rdflib.term import BNode

        ex = Namespace("https://example.org/")
        g1 = Graph()
        g2 = Graph()
        b1 = BNode("x")
        b2 = BNode("y")
        g1.add((ex.A, ex.rel, b1))
        g1.add((b1, ex.label, ex.val))
        g2.add((ex.A, ex.rel, b2))
        g2.add((b2, ex.label, ex.val))
        result = compare_rdf(g1, g2)
        assert result == [], f"Expected empty diff for isomorphic graphs, got: {result}"

    def test_differing_graphs_fail(self) -> None:
        """Two graphs with different content must produce a non-empty diff list."""
        ex = Namespace("https://example.org/")
        g1 = Graph()
        g2 = Graph()
        g1.add((ex.A, RDF.type, ex.B))
        g2.add((ex.A, RDF.type, ex.C))  # different object
        result = compare_rdf(g1, g2)
        assert len(result) > 0, "Expected non-empty diff for different graphs"

    def test_empty_vs_nonempty_fails(self) -> None:
        """An empty graph vs non-empty must fail."""
        ex = Namespace("https://example.org/")
        g1 = Graph()
        g2 = Graph()
        g2.add((ex.A, RDF.type, ex.B))
        result = compare_rdf(g1, g2)
        assert len(result) > 0

    def test_empty_vs_empty_passes(self) -> None:
        """Two empty graphs are isomorphic."""
        g1 = Graph()
        g2 = Graph()
        result = compare_rdf(g1, g2)
        assert result == []


# --------------------------------------------------------------------------- #
# compare_canonical_json tests
# --------------------------------------------------------------------------- #


class TestCompareCanonicalJson:
    """Unit tests for :func:`compare_canonical_json`."""

    def test_identical_dicts_match(self) -> None:
        """Identical dicts produce no diff."""
        d: dict[str, object] = {"a": 1, "b": "hello"}
        result = compare_canonical_json(d, d)
        assert result == []

    def test_key_order_independent(self) -> None:
        """Dicts with the same content but different key order must match."""
        d1: dict[str, object] = {"z": 3, "a": 1, "m": "foo"}
        d2: dict[str, object] = {"a": 1, "m": "foo", "z": 3}
        result = compare_canonical_json(d1, d2)
        assert result == [], f"Key-order difference produced unexpected diff: {result}"

    def test_nested_key_order_independent(self) -> None:
        """Nested objects also sort by key."""
        d1: dict[str, object] = {"x": {"b": 2, "a": 1}}
        d2: dict[str, object] = {"x": {"a": 1, "b": 2}}
        result = compare_canonical_json(d1, d2)
        assert result == []

    def test_value_difference_fails(self) -> None:
        """Different values for the same key must produce a diff."""
        d1: dict[str, object] = {"a": 1}
        d2: dict[str, object] = {"a": 2}
        result = compare_canonical_json(d1, d2)
        assert len(result) > 0

    def test_missing_key_fails(self) -> None:
        """A dict with an extra key must differ from one without it."""
        d1: dict[str, object] = {"a": 1, "b": 2}
        d2: dict[str, object] = {"a": 1}
        result = compare_canonical_json(d1, d2)
        assert len(result) > 0

    def test_string_normalization_unchanged(self) -> None:
        """String values must not be normalized beyond JSON encoding."""
        d1: dict[str, object] = {"k": "SoundUnderApproximation"}
        d2: dict[str, object] = {"k": "SoundUnderApproximation"}
        assert compare_canonical_json(d1, d2) == []

    def test_list_order_matters(self) -> None:
        """Lists are order-sensitive in JSON comparison."""
        d1: dict[str, object] = {"arr": [1, 2, 3]}
        d2: dict[str, object] = {"arr": [3, 2, 1]}
        result = compare_canonical_json(d1, d2)
        assert len(result) > 0, "List order must matter in JSON comparison"


# --------------------------------------------------------------------------- #
# compare_explanation_skeleton tests
# --------------------------------------------------------------------------- #


class TestCompareExplanationSkeleton:
    """Unit tests for :func:`compare_explanation_skeleton`."""

    _IRI_A = "https://example.org/rule/A"
    _IRI_B = "https://example.org/term/B"
    _IRI_C = "https://example.org/reifier/abc"

    def test_identical_skeletons_match(self) -> None:
        """Identical frozensets produce no diff."""
        iris = frozenset({self._IRI_A, self._IRI_B})
        result = compare_explanation_skeleton(iris, iris)
        assert result == []

    def test_different_skeletons_fail(self) -> None:
        """A changed cited IRI must produce a non-empty diff."""
        actual = frozenset({self._IRI_A, self._IRI_B})
        expected = frozenset({self._IRI_A, self._IRI_C})  # C instead of B
        result = compare_explanation_skeleton(actual, expected)
        assert len(result) > 0

    def test_extra_iri_fails(self) -> None:
        """Extra IRI in actual (not in expected) must be flagged as 'extra'."""
        actual = frozenset({self._IRI_A, self._IRI_B, self._IRI_C})
        expected = frozenset({self._IRI_A, self._IRI_B})
        result = compare_explanation_skeleton(actual, expected)
        assert len(result) > 0
        combined = "\n".join(result)
        assert "extra" in combined.lower() or self._IRI_C in combined

    def test_missing_iri_fails(self) -> None:
        """Missing IRI in actual (present in expected) must be flagged as 'missing'."""
        actual = frozenset({self._IRI_A})
        expected = frozenset({self._IRI_A, self._IRI_B})
        result = compare_explanation_skeleton(actual, expected)
        assert len(result) > 0
        combined = "\n".join(result)
        assert "missing" in combined.lower() or self._IRI_B in combined

    def test_prose_is_irrelevant(self) -> None:
        """Two skeletons with the same cited-IRI set but different prose must match.

        The runner contract: explanations are compared on cited-IRI skeleton ONLY,
        never on surface prose.  This test verifies that two Explanation objects
        with identical cited_iris but different prose_lines compare as identical.
        """
        from gmeow_tools.logic_seam import Explanation, ExplanationStep

        # Build two minimal Explanation objects with same cited_iris but
        # different prose_lines.
        iris = frozenset({self._IRI_A, self._IRI_B})
        step = ExplanationStep(
            derivation_id=self._IRI_A,
            rule_iri=self._IRI_A,
            quad_reifier=self._IRI_C,
            subject_iri=self._IRI_B,
            predicate_iri=self._IRI_A,
            obj_n3=f"<{self._IRI_B}>",
            graph_iri="https://example.org/world/1",
            term_iris=(self._IRI_A, self._IRI_B),
            source_step_ids=(),
            is_asserted=True,
            depth=0,
        )
        exp1 = Explanation(
            target_derivation_id=self._IRI_A,
            target_quad_reifier=self._IRI_C,
            world_iri="https://example.org/world/1",
            step_skeleton=(step,),
            cited_iris=iris,
            prose_lines=("This is prose version 1.",),
        )
        exp2 = Explanation(
            target_derivation_id=self._IRI_A,
            target_quad_reifier=self._IRI_C,
            world_iri="https://example.org/world/1",
            step_skeleton=(step,),
            cited_iris=iris,
            prose_lines=("Completely different prose.",),
        )
        # The skeleton comparison must pass (same cited_iris)
        result = compare_explanation_skeleton(exp1.cited_iris, exp2.cited_iris)
        assert result == [], (
            "Explanations with identical cited_iris but different prose should match"
        )


# --------------------------------------------------------------------------- #
# discover_cases tests
# --------------------------------------------------------------------------- #


class TestDiscoverCases:
    """Tests for :func:`discover_cases`."""

    def test_discovers_projection_cases(self) -> None:
        """discover_cases finds all projection cases in the conformance corpus."""
        if not _CONFORMANCE_ROOT.is_dir():
            pytest.skip("conformance/logic/ not present")
        cases = discover_cases(_CONFORMANCE_ROOT)
        # Must find at least the 3 projection cases committed in the worktree
        projection_ids = {c.case_id for c in cases}
        assert any("projections/" in cid for cid in projection_ids), (
            f"No projection cases found among: {sorted(projection_ids)}"
        )

    def test_hard_fails_on_missing_cases_dir(self, tmp_path: Path) -> None:
        """discover_cases raises RunnerError when conformance/logic/cases/ is absent."""
        with pytest.raises(RunnerError, match="does not exist"):
            discover_cases(tmp_path)

    def test_profile_json_parsed(self) -> None:
        """Each discovered case carries a parsed profile dict."""
        if not _CONFORMANCE_ROOT.is_dir():
            pytest.skip("conformance/logic/ not present")
        cases = discover_cases(_CONFORMANCE_ROOT)
        for case in cases:
            assert isinstance(case.profile, dict), (
                f"Case {case.case_id}: profile must be a dict"
            )
            assert "semantic_profile" in case.profile, (
                f"Case {case.case_id}: profile.json must contain 'semantic_profile'"
            )

    def test_hard_fails_on_malformed_profile(self, tmp_path: Path) -> None:
        """discover_cases raises RunnerError when profile.json is malformed JSON."""
        # Create a fake conformance root with a malformed case
        cases_dir = tmp_path / "cases" / "projections" / "bad-case"
        cases_dir.mkdir(parents=True)
        (cases_dir / "input.logic.ttl").write_text(
            "@prefix ex: <https://example.org/> . ex:A a ex:B .", encoding="utf-8"
        )
        (cases_dir / "profile.json").write_text("{bad json", encoding="utf-8")
        with pytest.raises(RunnerError, match=r"cannot read profile\.json"):
            discover_cases(tmp_path)


# --------------------------------------------------------------------------- #
# run() smoke test over projection cases
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize("case_dir", _ALL_PROJECTION_CASES, ids=_PROJECTION_CASE_IDS)
class TestRunSmokeProjectionCases:
    """Smoke tests: run() over each committed projection case."""

    def test_run_returns_runner_outputs(self, case_dir: Path) -> None:
        """run() completes without error and returns a RunnerOutputs."""
        outputs = run(case_dir, mode="native")
        assert isinstance(outputs, RunnerOutputs)

    def test_projections_non_empty(self, case_dir: Path) -> None:
        """All 7 projection back-ends must produce results."""
        outputs = run(case_dir, mode="native")
        assert len(outputs.projections.results) == 7, (
            f"Expected 7 projection results, got {len(outputs.projections.results)}"
        )

    def test_projection_targets_complete(self, case_dir: Path) -> None:
        """All expected projection target names must be present."""
        expected_targets = {
            "owl-dl",
            "owl-el",
            "datalog",
            "n3",
            "gufo",
            "canonical-rdf12",
            "nemo",
        }
        outputs = run(case_dir, mode="native")
        actual_targets = {pr.target for pr in outputs.projections.results}
        assert actual_targets == expected_targets

    def test_verdicts_dict_present(self, case_dir: Path) -> None:
        """verdicts must be a dict (possibly empty for projection-only cases)."""
        outputs = run(case_dir, mode="native")
        assert isinstance(outputs.verdicts, dict)

    def test_witnesses_always_empty(self, case_dir: Path) -> None:
        """witnesses is always {} in v1 monotonic core."""
        outputs = run(case_dir, mode="native")
        assert outputs.witnesses == {}, (
            "witnesses must be {} in the v1 monotonic oracle (no negation)"
        )

    def test_answers_always_empty(self, case_dir: Path) -> None:
        """answers is always {} — deferred to #504/#505."""
        outputs = run(case_dir, mode="native")
        assert outputs.answers == {}, (
            "answers must be {} in v1 (goal/counterfactual deferred)"
        )

    def test_ledger_json_has_all_targets(self, case_dir: Path) -> None:
        """preservation ledger JSON must have entries for all 7 targets."""
        outputs = run(case_dir, mode="native")
        ledger = outputs.projections.ledger_json
        expected_targets = {
            "owl-dl",
            "owl-el",
            "datalog",
            "n3",
            "gufo",
            "canonical-rdf12",
            "nemo",
        }
        assert set(ledger.keys()) == expected_targets

    def test_ledger_matches_expected(self, case_dir: Path) -> None:
        """Preservation ledger must match the committed preservation-ledger.json."""
        expected_path = (
            case_dir / "expected" / "projections" / "preservation-ledger.json"
        )
        if not expected_path.exists():
            pytest.skip(f"No committed preservation-ledger.json for {case_dir.name}")
        outputs = run(case_dir, mode="native")
        expected_ledger: dict[str, object] = json.loads(
            expected_path.read_text(encoding="utf-8")
        )
        diffs = compare_canonical_json(
            outputs.projections.ledger_json,  # type: ignore[arg-type]
            expected_ledger,
        )
        assert diffs == [], f"{case_dir.name}: ledger mismatch:\n" + "\n".join(diffs)

    def test_report_graph_isomorphic_to_expected(self, case_dir: Path) -> None:
        """Projection report graph must be isomorphic to the committed golden."""
        expected_path = case_dir / "expected" / "projections" / "projection-report.ttl"
        if not expected_path.exists():
            pytest.skip(f"No committed projection-report.ttl for {case_dir.name}")
        outputs = run(case_dir, mode="native")
        expected_graph = Graph()
        expected_graph.parse(str(expected_path), format="turtle")
        diffs = compare_rdf(outputs.projections.report_graph, expected_graph)
        assert diffs == [], (
            f"{case_dir.name}: projection-report.ttl mismatch:\n" + "\n".join(diffs)
        )

    def test_rdf_projections_isomorphic(self, case_dir: Path) -> None:
        """OWL-DL, OWL-EL, gUFO, and canonical-rdf12 must match their golden files."""
        rdf_targets = {
            "owl-dl": "owl-dl.ttl",
            "owl-el": "owl-el.ttl",
            "gufo": "gufo.ttl",
            "canonical-rdf12": "canonical-rdf12.ttl",
        }
        expected_dir = case_dir / "expected" / "projections"
        outputs = run(case_dir, mode="native")
        proj_by_target = {pr.target: pr for pr in outputs.projections.results}

        for target_name, filename in rdf_targets.items():
            golden_path = expected_dir / filename
            if not golden_path.exists():
                continue
            pr = proj_by_target[target_name]
            assert pr.graph is not None, f"{target_name}: no graph produced"
            expected_graph = Graph()
            expected_graph.parse(str(golden_path), format="turtle")
            diffs = compare_rdf(pr.graph, expected_graph)
            assert diffs == [], f"{case_dir.name}/{target_name}: {'\n'.join(diffs)}"


# --------------------------------------------------------------------------- #
# diff_case tests over projection cases
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize("case_dir", _ALL_PROJECTION_CASES, ids=_PROJECTION_CASE_IDS)
def test_diff_case_passes_for_projection_cases(case_dir: Path) -> None:
    """diff_case must report PASS for every committed projection case."""
    outputs = run(case_dir, mode="native")
    diff_result = diff_case(outputs)
    assert diff_result.passed, f"diff_case FAILED for {case_dir.name}:\n" + "\n".join(
        diff_result.diffs
    )
    assert diff_result.diffs == []


# --------------------------------------------------------------------------- #
# diff_case test over explanation case
# --------------------------------------------------------------------------- #

_EXPLANATION_CASE_DIR = (
    _CONFORMANCE_ROOT / "cases" / "explanation" / "transitive-derivation"
)


def test_diff_case_passes_for_explanation_case() -> None:
    """diff_case must report PASS for the explanation/transitive-derivation case.

    Also verifies:
    * The derived explanation's cited_iris is non-empty (guard against vacuous pass).
    * The produced cited_iris equals the golden's parsed cited_iris.
    """
    if not _EXPLANATION_CASE_DIR.is_dir():
        pytest.skip("explanation/transitive-derivation case not present")

    outputs = run(_EXPLANATION_CASE_DIR, mode="native")

    # Locate the derived explanation: the one that cites a real rule (not assert).
    _assert_rule_iri = "https://blackcatinformatics.ca/logic/assert"
    derived = [
        e
        for e in outputs.explanations
        if any(step.rule_iri != _assert_rule_iri for step in e.step_skeleton)
    ]
    assert len(derived) >= 1, (
        "Expected at least one derived (non-trivial) explanation, got none. "
        f"Explanations: {[e.target_quad_reifier for e in outputs.explanations]}"
    )
    # Guard against vacuous pass: the derived explanation must cite real IRIs.
    derived_expl = derived[0]
    assert len(derived_expl.cited_iris) > 0, (
        "Derived explanation has empty cited_iris — vacuous pass guard triggered"
    )

    # Parse the committed golden for the derived explanation and compare.
    golden_dir = _EXPLANATION_CASE_DIR / "expected" / "explanation"
    golden_files = sorted(golden_dir.glob("*.md"))
    assert len(golden_files) >= 1, f"No golden .md files found in {golden_dir}"

    # Find the golden that matches the derived explanation's reifier.
    matched_golden: Path | None = None
    for gf in golden_files:
        reifier = _parse_explanation_reifier(gf.read_text(encoding="utf-8"))
        if reifier == derived_expl.target_quad_reifier:
            matched_golden = gf
            break
    assert matched_golden is not None, (
        f"No golden file found for derived explanation reifier "
        f"<{derived_expl.target_quad_reifier}> in {golden_dir}"
    )
    committed_iris = _parse_cited_iri_skeleton(
        matched_golden.read_text(encoding="utf-8")
    )
    extra = sorted(derived_expl.cited_iris - committed_iris)
    missing = sorted(committed_iris - derived_expl.cited_iris)
    assert derived_expl.cited_iris == committed_iris, (
        f"Derived explanation cited_iris does not match golden:\n"
        f"  extra   (produced not in golden): {extra}\n"
        f"  missing (golden not produced):    {missing}"
    )

    # Full diff_case must also pass.
    diff_result = diff_case(outputs)
    assert diff_result.passed, (
        "diff_case FAILED for explanation/transitive-derivation:\n"
        + "\n".join(diff_result.diffs)
    )
    assert diff_result.diffs == []


# --------------------------------------------------------------------------- #
# Native EL/DL reasoning lane (#665) — gmeow_logic.reason_native + reason.py
# --------------------------------------------------------------------------- #


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


# --------------------------------------------------------------------------- #
# run() error handling
# --------------------------------------------------------------------------- #


class TestRunErrorHandling:
    """Tests for run() error paths."""

    def test_unsupported_mode_raises(self, tmp_path: Path) -> None:
        """run() with an unsupported mode must raise RunnerError."""
        # Create a minimal valid case
        (tmp_path / "input.logic.ttl").write_text(
            "@prefix ex: <https://example.org/> . ex:A a ex:B .", encoding="utf-8"
        )
        (tmp_path / "profile.json").write_text(
            json.dumps(
                {
                    "semantic_profile": "PositiveHornProfile",
                    "world_types": [],
                    "mode": "native",
                    "expected_decidability_class": "decidable",
                }
            ),
            encoding="utf-8",
        )
        with pytest.raises(RunnerError, match="Unsupported mode"):
            run(tmp_path, mode="owl-dl")

    def test_missing_input_raises(self, tmp_path: Path) -> None:
        """run() with missing input.logic.ttl must raise RunnerError."""
        (tmp_path / "profile.json").write_text(
            json.dumps({"semantic_profile": "PositiveHornProfile"}), encoding="utf-8"
        )
        with pytest.raises(RunnerError, match=r"input\.logic\.ttl not found"):
            run(tmp_path)

    def test_missing_profile_raises(self, tmp_path: Path) -> None:
        """run() with missing profile.json must raise RunnerError."""
        (tmp_path / "input.logic.ttl").write_text(
            "@prefix ex: <https://example.org/> . ex:A a ex:B .", encoding="utf-8"
        )
        with pytest.raises(RunnerError, match=r"profile\.json not found"):
            run(tmp_path)

    def test_run_minimal_valid_case(self, tmp_path: Path) -> None:
        """run() over a minimal valid case must succeed."""
        (tmp_path / "input.logic.ttl").write_text(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n"
            "@prefix ex: <https://example.org/> .\n"
            "ex:Animal a logic:Kind .\n",
            encoding="utf-8",
        )
        (tmp_path / "profile.json").write_text(
            json.dumps(
                {
                    "semantic_profile": "PositiveHornProfile",
                    "world_types": [],
                    "mode": "native",
                    "expected_decidability_class": "decidable",
                }
            ),
            encoding="utf-8",
        )
        outputs = run(tmp_path, mode="native")
        assert len(outputs.projections.results) == 7
        assert outputs.witnesses == {}
        assert outputs.answers == {}
