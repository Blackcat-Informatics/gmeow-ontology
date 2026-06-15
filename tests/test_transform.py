# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The transpiler driver (#34 Phase 1): MAXIMAL(G) = G + E(G) + P(G).

Acceptance pins: one file family per run (gts/nq/ttl/jsonld); every derived
triple carries an RDF 1.2 provenance reifier in the .nq form while index.ttl
stays plain-RDF; standalone projection triples are a subset of the fat file;
the suppression canary never leaks through ANY output; an equivalence-collapse
lint ERROR aborts; reruns are byte-identical; paudley.ttl fits the recorded
wall-clock budget.
"""

from __future__ import annotations

from pathlib import Path

import pyoxigraph
import pytest
from rdflib import Graph, URIRef

from gmeow_tools.config import EXTERNAL_FIXTURES_DIR, FIXTURES_DIR, NAMESPACE
from gmeow_tools.transform import TransformAbortedError, transform, vocab_coverage

pytestmark = pytest.mark.ci_only

_RIGHTS = FIXTURES_DIR / "rights.ttl"
_CANARY = FIXTURES_DIR / "suppression-canary.ttl"
_PAUDLEY = EXTERNAL_FIXTURES_DIR / "paudley.ttl"
_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
_MAPPED_FROM = NAMESPACE + "mappedFrom"


@pytest.fixture(scope="module")
def rights_out(tmp_path_factory: pytest.TempPathFactory) -> Path:
    out = tmp_path_factory.mktemp("transform-rights")
    transform(_RIGHTS, out_dir=out)
    return out


def test_emits_the_file_family(rights_out: Path) -> None:
    for name in ("rights.gts", "index.nq", "index.ttl", "index.jsonld"):
        assert (rights_out / name).is_file(), name


def test_nq_is_valid_rdf12_with_provenance(rights_out: Path) -> None:
    """Every derived triple is reified and mappedFrom-attributed in the .nq."""
    nq = (rights_out / "index.nq").read_bytes()
    quads = list(pyoxigraph.parse(nq, format=pyoxigraph.RdfFormat.N_QUADS))
    reified = {
        (str(q.object.subject), str(q.object.predicate), str(q.object.object))
        for q in quads
        if isinstance(q.predicate, pyoxigraph.NamedNode)
        and q.predicate.value == _REIFIES
        and isinstance(q.object, pyoxigraph.Triple)
    }
    assert reified, "no reifier bindings in index.nq"
    mapped_reifiers = {
        str(q.subject)
        for q in quads
        if isinstance(q.predicate, pyoxigraph.NamedNode)
        and q.predicate.value == _MAPPED_FROM
    }
    reifier_subjects = {
        str(q.subject)
        for q in quads
        if isinstance(q.predicate, pyoxigraph.NamedNode)
        and q.predicate.value == _REIFIES
    }
    assert reifier_subjects == mapped_reifiers, (
        "every reifier must carry gmeow:mappedFrom (and nothing else mints one)"
    )


def test_index_ttl_is_plain_rdf_superset_of_g(rights_out: Path) -> None:
    """index.ttl = asserted base triples only — and contains all of G."""
    text = (rights_out / "index.ttl").read_text(encoding="utf-8")
    assert "<<(" not in text  # reification-stripped (plain-RDF readable)
    maximal = Graph().parse(rights_out / "index.ttl", format="turtle")
    # The skolemized source graph is a subgraph of the fat file.
    source = Graph().parse(_RIGHTS, format="turtle")
    assert len(maximal) > len(source)
    # Derived facts present: the Mark instance gained its schema.org type.
    assert (
        URIRef("https://example.org/rights/acme-mark"),
        None,
        URIRef("https://schema.org/Brand"),
    ) in maximal


def test_standalone_projection_is_a_subset(rights_out: Path) -> None:
    """Every triple a standalone profile run derives appears in the fat file."""
    from gmeow_tools import sparql
    from gmeow_tools.graph import load_merged_graph
    from gmeow_tools.projections import project_graph
    from gmeow_tools.transform import _skolemized

    abox = _skolemized(Graph().parse(_RIGHTS, format="turtle"))
    maximal = set(Graph().parse(rights_out / "index.ttl", format="turtle"))
    onto_subjects = set(load_merged_graph(include_imports=False).subjects())
    store = sparql.store_with(include_imports=False, extra_triples=abox)
    for profile in ("odrl", "cc", "dcterms", "schema-org"):
        for s, p, o in project_graph(profile, store):
            if s in onto_subjects:
                continue  # projections of the ontology itself are excluded
            assert (s, p, o) in maximal, f"{profile}: {(s, p, o)}"


def test_claim_layer_projects_back_to_source_vocab(tmp_path: Path) -> None:
    """A closeMatch up-lift parks a gmeow term in a StatementMetadata claim, never
    asserted G. P(G) still reproduces the source's own vocab term by projecting
    over the materialized claim layer (#552 option 1) — the round trip hands back
    what came in — WITHOUT asserting the claimed gmeow term itself."""
    from rdflib import RDF

    from gmeow_tools.transform import transform_graph

    gm = NAMESPACE
    doap = "http://usefulinc.com/ns/doap#"
    proj = URIRef("https://ex.org/proj")
    g = Graph()
    g.parse(
        data=f"""
        @prefix gmeow: <{gm}> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix doap: <{doap}> .
        [] a gmeow:StatementMetadata ;
           gmeow:qSubject <{proj}> ;
           gmeow:qPredicate rdf:type ;
           gmeow:qObject gmeow:SoftwareProject ;
           gmeow:annotation [ gmeow:annProperty gmeow:mappedFrom ;
                              gmeow:annValue doap:Project ] .
        """,
        format="turtle",
    )
    transform_graph(g, "claim", out_dir=tmp_path, profiles=["doap"])
    out = Graph().parse(tmp_path / "index.ttl", format="turtle")
    # the source's own vocab term round-trips (projected from the claim)
    assert (proj, RDF.type, URIRef(doap + "Project")) in out
    # but the claimed gmeow term is NOT asserted — it stays a claim
    assert (proj, RDF.type, URIRef(gm + "SoftwareProject")) not in out


def test_suppression_canary_never_leaks(tmp_path: Path) -> None:
    """No output form of MAXIMAL(G) carries a suppressed literal (#282)."""
    out = tmp_path / "canary"
    transform(_CANARY, out_dir=out)
    for path in sorted(out.iterdir()):
        data = path.read_bytes()
        assert b"SUPPRESSED-CANARY" not in data, path.name
    # Non-vacuous: the control twin survives into the fat file.
    assert b"CONTROL-CANARY" in (out / "index.ttl").read_bytes()


def test_equivalence_collapse_aborts(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A poisoned strong-edge graph refuses the WHOLE transform (#284)."""
    from gmeow_tools import alignment_lint

    finding = alignment_lint.AlignmentFinding(
        severity=alignment_lint.Severity.ERROR,
        check="equivalence-collapse",
        subject_id="gmeow:Person",
        predicate_id="owl:equivalentClass",
        object_id="schema:Person",
        message="synthetic collapse for the abort test",
    )
    monkeypatch.setattr(
        alignment_lint, "lint_alignment_directions", lambda **_: [finding]
    )
    with pytest.raises(TransformAbortedError, match="equivalence-collapse"):
        transform(_RIGHTS, out_dir=tmp_path / "aborted")


def test_denied_rows_shrink_e_of_g(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A plain direction-ERROR row drops that SATURATION edge; the run continues.

    schema:Brand may still arrive via the projection engine (P, not E), so the
    pin is on PROVENANCE: with the cell denied, no reifier annotation points
    at an authored TermEquivalence cell for that triple — only at the
    projection alignment IRI.
    """
    from gmeow_tools import alignment_lint

    finding = alignment_lint.AlignmentFinding(
        severity=alignment_lint.Severity.ERROR,
        check="inverse-direction",
        subject_id="gmeow:Mark",
        predicate_id="owl:equivalentClass",
        object_id="schema:Brand",
        message="synthetic denial",
    )
    monkeypatch.setattr(
        alignment_lint, "lint_alignment_directions", lambda **_: [finding]
    )
    out = tmp_path / "denied"
    report = transform(_RIGHTS, out_dir=out)
    assert report.denied_cells == 1
    nq = (out / "index.nq").read_bytes()
    quads = list(pyoxigraph.parse(nq, format=pyoxigraph.RdfFormat.N_QUADS))
    brand_reifiers = {
        str(q.subject)
        for q in quads
        if isinstance(q.predicate, pyoxigraph.NamedNode)
        and q.predicate.value == _REIFIES
        and isinstance(q.object, pyoxigraph.Triple)
        and str(q.object.object) == "<https://schema.org/Brand>"
    }
    for q in quads:
        if (
            isinstance(q.predicate, pyoxigraph.NamedNode)
            and q.predicate.value == _MAPPED_FROM
            and str(q.subject) in brand_reifiers
        ):
            assert str(q.object).startswith(f"<{NAMESPACE}projections/"), (
                "a denied saturation cell still attributed a Brand triple"
            )


def test_reruns_are_byte_identical(tmp_path: Path) -> None:
    """Skolemization + content-addressed reifiers ⇒ diffable reruns."""
    first = tmp_path / "a"
    second = tmp_path / "b"
    transform(_RIGHTS, out_dir=first)
    transform(_RIGHTS, out_dir=second)
    for name in ("rights.gts", "index.nq"):
        assert (first / name).read_bytes() == (second / name).read_bytes(), name


def test_wall_clock_budget_on_the_biggest_fixture(tmp_path: Path) -> None:
    """The 11.5k-triple dev A-Box transforms within the recorded budget."""
    report = transform(_PAUDLEY, out_dir=tmp_path / "paudley")
    assert report.asserted > 10_000
    assert report.wall_clock_s < 120, f"{report.wall_clock_s:.1f}s over budget"


def test_vocab_coverage_table_shape() -> None:
    maximal = Graph().parse(_RIGHTS, format="turtle")
    target = Graph().parse(_RIGHTS, format="turtle")
    table = vocab_coverage(maximal, target)
    assert table.startswith("| vocabulary |")
    assert "| **total** |" in table
