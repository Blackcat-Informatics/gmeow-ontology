# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The GMEOW → maximally-interlinked transpiler (#34, Phase 1).

``MAXIMAL(G) = G + E(G) + P(G)``: author the profile ONCE as canonical GMEOW;
derive the multi-vocabulary file as a compiled, auditable artifact. ``E(G)``
is the strong-equivalence saturation (:mod:`gmeow_tools.saturate`); ``P(G)``
materializes every executable projection profile
(:mod:`gmeow_tools.projections`). Both read the ORIGINAL ``G`` — projection
output is never chained back in (no fixpoint, clean provenance).

Outputs (``gmeow transform <abox>``):

* ``<stem>.gts`` — the canonical artifact: base triples + every derived
  triple's RDF 1.2 provenance reifier (``gmeow:mappedFrom`` → the authoring
  cell / projection alignment IRI, ``gmeow:confidence`` when recorded),
  emitted natively by the GTS producer (no Jena hot path).
* ``index.nq`` — the full RDF 1.2 N-Quads form (the gts→nquads shim).
* ``index.ttl`` / ``index.jsonld`` / ``index.nt`` — the ASSERTED BASE TRIPLES
  only, in three plain-RDF syntaxes: a consumer ignorant of RDF 1.2 still parses
  every triple (maximal readability); the audit trail lives in ``.gts``/``.nq``.

Gates: the alignment-direction lint authorizes saturation (ERROR rows are
refused; an ``equivalence-collapse`` ERROR aborts the whole transform — a
poisoned strong-edge graph cannot be repaired row-wise, #284). Suppression
(#282) is enforced over BOTH derivation paths. Blank nodes are skolemized
(content-addressed via the canonical labels) so reruns are diffable.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from rdflib import Graph, URIRef

from gmeow_tools.config import DIST_DIR, NAMESPACE, PREFIXES
from gmeow_tools.graph import bind_prefixes, load_merged_graph
from gmeow_tools.saturate import (
    DerivedTriple,
    load_cells,
    reifier_for,
    saturate,
    suppressed_nodes,
)

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from rdflib.term import Node

_MAPPED_FROM = URIRef(NAMESPACE + "mappedFrom")
_SKOLEM_AUTHORITY = NAMESPACE.rstrip("/")
_SKOLEM_BASEPATH = "/.well-known/genid/"


class TransformAbortedError(RuntimeError):
    """The transform refused to run (e.g. an equivalence-collapse ERROR)."""


@dataclass(slots=True)
class TransformReport:
    """Counts and output paths of one transform run."""

    asserted: int = 0
    saturated: int = 0
    projected: int = 0
    suppressed_dropped: int = 0
    denied_cells: int = 0
    wall_clock_s: float = 0.0
    written: list[Path] = field(default_factory=list)


def _skolemized(abox: Graph) -> Graph:
    """Skolemize blank nodes on CONTENT-ADDRESSED labels (diffable reruns).

    rdflib blank-node labels are per-process UUIDs, so the graph is canonicalized
    first; the canonical labels are a pure function of the graph's content, making
    the skolem IRIs stable across runs. Projection-minted nodes get the same
    treatment.

    Canonicalization runs on **pyoxigraph** (RDFC-1.0, Rust): rdflib's pure-Python
    ``to_canonical_graph`` is pathologically slow on the blank-node-heavy graphs a
    real source produces (a paudley transpile spent >10 minutes there). pyoxigraph
    canonicalizes the same graph in well under a second; the cheap O(n) skolemize
    round-trip stays in rdflib.
    """
    import pyoxigraph
    from rdflib import Literal

    dataset = pyoxigraph.Dataset(
        pyoxigraph.Quad(t.subject, t.predicate, t.object)
        for t in pyoxigraph.parse(
            abox.serialize(format="nt", encoding="utf-8"),
            format=pyoxigraph.RdfFormat.N_TRIPLES,
        )
    )
    dataset.canonicalize(pyoxigraph.CanonicalizationAlgorithm.UNSTABLE)

    # Convert pyoxigraph terms → rdflib DIRECTLY, skolemizing each blank node by
    # its CANONICAL value. Going through an N-Triples round trip would let rdflib
    # re-randomize the blank-node ids (losing the canonical labels and breaking
    # diffable reruns); this keeps the canonical labels in the skolem IRIs.
    skolem = _SKOLEM_AUTHORITY + _SKOLEM_BASEPATH

    xsd_string = "http://www.w3.org/2001/XMLSchema#string"

    def _to_rdflib(term: object) -> URIRef | Literal:
        if isinstance(term, pyoxigraph.NamedNode):
            return URIRef(term.value)
        if isinstance(term, pyoxigraph.BlankNode):
            return URIRef(skolem + term.value)
        # pyoxigraph.Literal — always carries an explicit datatype (xsd:string for
        # a plain string, rdf:langString for a lang literal). Map back to rdflib's
        # convention: a lang tag, a NON-string datatype, or a plain literal (the
        # implicit xsd:string is dropped, matching the .gts and rdflib paths).
        if term.language:  # type: ignore[attr-defined]
            return Literal(term.value, lang=term.language)  # type: ignore[attr-defined]
        dt = term.datatype.value  # type: ignore[attr-defined]
        if dt != xsd_string:
            return Literal(term.value, datatype=URIRef(dt))  # type: ignore[attr-defined]
        return Literal(term.value)  # type: ignore[attr-defined]

    out = Graph()
    for q in dataset:
        out.add((_to_rdflib(q.subject), _to_rdflib(q.predicate), _to_rdflib(q.object)))
    bind_prefixes(out)
    return out


def _denied_cells() -> set[tuple[str, str, str]]:
    """The direction lint's ERROR rows — the saturation refusal set (#25).

    On a wheel-only install (no source tree) the lint cannot run — it reads the
    SSSOM tables and the vendored target axioms — so the set precomputed at
    bundle-build time is used instead (#bundle: the CLI razor; the
    equivalence-collapse abort below is enforced at build, never reached here).

    Raises:
        TransformAbortedError: On any ``equivalence-collapse`` ERROR (#284): the
            strong-edge graph itself connects disjoint classes, and no
            per-row denial can repair a poisoned chain.
    """
    from gmeow_tools.bundle import bundled_denied_cells, repo_sources_present

    if not repo_sources_present():
        precomputed = bundled_denied_cells()
        if precomputed is not None:
            return set(precomputed)

    from gmeow_tools.alignment_lint import Severity, lint_alignment_directions

    findings = lint_alignment_directions()
    collapses = [
        f
        for f in findings
        if f.severity is Severity.ERROR and f.check == "equivalence-collapse"
    ]
    if collapses:
        details = "; ".join(f.message for f in collapses[:3])
        msg = f"equivalence-collapse ERROR — transform refused: {details}"
        raise TransformAbortedError(msg)
    return {
        (f.subject_id, f.predicate_id, f.object_id)
        for f in findings
        if f.severity is Severity.ERROR
    }


def _projection_derived(
    abox: Graph,
    onto: Graph,
    profiles: Sequence[str],
    suppressed: set[Node],
) -> dict[tuple[Node, Node, Node], set[tuple[URIRef, Node]]]:
    """P(G): run every profile's CONSTRUCT over the ORIGINAL G.

    The store carries the merged ontology for matching context (subclass
    paths, the language-tag boundary), so the CONSTRUCT output includes
    projections OF THE ONTOLOGY'S OWN nodes (term annotations, value-
    vocabulary individuals) — those are filtered out: only triples about
    A-Box subjects (or nodes the projections MINT from them) survive.

    Each kept triple is annotated ``gmeow:mappedFrom`` → the profile's EDOAL
    alignment IRI (``…/projections/<name>``). The compiled queries carry
    their own suppression guards (#282); the filter here is the shared
    emission path's belt-and-braces re-check.
    """
    from gmeow_tools import sparql
    from gmeow_tools.projections import project_graph

    store = sparql.store_with(include_imports=False, extra_triples=abox)
    onto_subjects = set(onto.subjects())
    derived: dict[tuple[Node, Node, Node], set[tuple[URIRef, Node]]] = {}
    for name in profiles:
        alignment_iri = URIRef(f"{NAMESPACE}projections/{name}")
        projected = project_graph(name, store)
        for s, p, o in projected:
            if s in onto_subjects:
                continue  # a projection of the ontology itself, not of G
            if (s, p, o) in abox or s in suppressed or o in suppressed:
                continue
            derived.setdefault((s, p, o), set()).add((_MAPPED_FROM, alignment_iri))
    return derived


def _merge_derived(
    saturated: Sequence[DerivedTriple],
    projected: dict[tuple[Node, Node, Node], set[tuple[URIRef, Node]]],
) -> list[DerivedTriple]:
    """One reifier per derived triple; E- and P-annotations merge."""
    merged: dict[tuple[Node, Node, Node], set[tuple[URIRef, Node]]] = {
        row.triple: set(row.annotations) for row in saturated
    }
    for triple, ann_rows in projected.items():
        merged.setdefault(triple, set()).update(ann_rows)
    return [
        DerivedTriple(
            triple=triple,
            reifier=reifier_for(*triple),
            annotations=tuple(
                sorted(ann_rows, key=lambda row: (str(row[0]), str(row[1])))
            ),
        )
        for triple, ann_rows in sorted(
            merged.items(), key=lambda item: tuple(n.n3() for n in item[0])
        )
    ]


def _serialize_outputs(
    base_plus_derived: Graph,
    gts_bytes: bytes,
    out_dir: Path,
    stem: str,
) -> list[Path]:
    """Write .gts / index.nq / index.ttl / index.jsonld / index.nt, all verified.

    Tier discipline (#452): the canonical ``.gts`` / ``.nq`` carry the full RDF
    1.2 provenance AND the canonical internal ``x-gmeow-*`` language tags; the
    consumer tiers (``.ttl`` / ``.jsonld`` / ``.nt``) are clean asserted triples
    with public BCP-47 tags — a consumer parser (Google included) reads
    ``x-gmeow-english`` as nothing, so the readable serializations must not leak
    it. The ``.gts``/``.nq`` are written first (from the canonical bytes); then
    ``base_plus_derived`` is retagged in place for the consumer tiers.
    """
    import pyoxigraph

    from gmeow_tools.language_tags import retag_graph
    from gts import read, to_nquads

    out_dir.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []

    gts_path = out_dir / f"{stem}.gts"
    gts_path.write_bytes(gts_bytes)
    written.append(gts_path)

    nq_text = to_nquads(read(gts_bytes))
    nq_path = out_dir / "index.nq"
    nq_path.write_text(nq_text, encoding="utf-8")
    # RDF 1.2 verification: the trusted parser must accept every line.
    list(pyoxigraph.parse(nq_text.encode(), format=pyoxigraph.RdfFormat.N_QUADS))
    written.append(nq_path)

    # consumer-facing tiers only: retag x-gmeow-* → public BCP-47 across the whole
    # graph (base + derived together; idempotent for the derived triples
    # project_graph already retagged). The canonical .gts/.nq keep the internal
    # tags for round-trip fidelity.
    retag_graph(base_plus_derived)

    ttl_path = out_dir / "index.ttl"
    base_plus_derived.serialize(destination=ttl_path, format="turtle")
    check = Graph()
    check.parse(ttl_path, format="turtle")
    if len(check) != len(base_plus_derived):
        msg = f"index.ttl round-trip changed triple count: {ttl_path}"
        raise ValueError(msg)
    written.append(ttl_path)

    jsonld_path = out_dir / "index.jsonld"
    base_plus_derived.serialize(
        destination=jsonld_path, format="json-ld", auto_compact=True
    )
    check = Graph()
    check.parse(jsonld_path, format="json-ld")
    if len(check) != len(base_plus_derived):
        msg = f"index.jsonld round-trip changed triple count: {jsonld_path}"
        raise ValueError(msg)
    written.append(jsonld_path)

    nt_path = out_dir / "index.nt"
    base_plus_derived.serialize(destination=nt_path, format="nt")
    check = Graph()
    check.parse(nt_path, format="nt")
    if len(check) != len(base_plus_derived):
        msg = f"index.nt round-trip changed triple count: {nt_path}"
        raise ValueError(msg)
    written.append(nt_path)
    return written


def transform(
    abox_path: Path,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
) -> TransformReport:
    """Run MAXIMAL(G) over an A-Box *file*.

    Args:
        abox_path: The canonical GMEOW instance Turtle file.
        out_dir: Output directory (default ``dist/transform/<stem>/``).
        profiles: Projection profiles for P(G) (default: all registered).

    Returns:
        The :class:`TransformReport` (counts, wall clock, written paths).
    """
    raw = Graph()
    raw.parse(abox_path, format="turtle")
    return transform_graph(raw, abox_path.stem, out_dir=out_dir, profiles=profiles)


def transform_graph(
    raw: Graph,
    stem: str,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
) -> TransformReport:
    """Run MAXIMAL(G) over an in-memory A-Box graph.

    The graph-core of :func:`transform` — used directly by the end-to-end
    :func:`gmeow_tools.transpile.transpile` so an up-projected GMEOW graph flows
    into the maximal projection without a temp file.

    Args:
        raw: The canonical GMEOW A-Box graph.
        stem: The output basename (the ``.gts`` file and default sub-directory).
        out_dir: Output directory (default ``dist/transform/<stem>/``).
        profiles: Projection profiles for P(G) (default: all registered).

    Returns:
        The :class:`TransformReport` (counts, wall clock, written paths).
    """
    from gmeow_tools.gts_producer import gts_from_maximal
    from gmeow_tools.mapping_compile import _default_suppression_vocab
    from gmeow_tools.projections import PROFILES

    start = time.perf_counter()
    target = out_dir if out_dir is not None else DIST_DIR / "transform" / stem
    names = list(PROFILES) if profiles is None else list(profiles)
    unknown = sorted(set(names) - set(PROFILES))
    if unknown:
        msg = f"unknown projection profile(s): {', '.join(unknown)}"
        raise ValueError(msg)

    abox = _skolemized(raw)

    onto = load_merged_graph(include_imports=False)
    vocab = _default_suppression_vocab()
    denied = _denied_cells()
    suppressed = suppressed_nodes(abox, vocab)

    # The outputs are PUBLICATIONS — projections of the canonical source.
    # Suppressed nodes are withheld from the BASE graph too (#282, P10):
    # the canonical input file retains them (suppression never deletes at
    # the source); the published fat file must not carry them.
    if suppressed:
        published = Graph()
        bind_prefixes(published)
        for s, p, o in abox:
            if s not in suppressed and o not in suppressed:
                published.add((s, p, o))
        abox = published

    saturated = saturate(
        abox, onto=onto, cells=load_cells(), denied=denied, vocab=vocab
    )
    projected = _projection_derived(abox, onto, names, suppressed)
    derived = _merge_derived(saturated, projected)

    base_plus_derived = Graph()
    bind_prefixes(base_plus_derived)
    for triple in abox:
        base_plus_derived.add(triple)
    for row in derived:
        base_plus_derived.add(row.triple)

    gts_bytes = gts_from_maximal(abox, derived)
    written = _serialize_outputs(base_plus_derived, gts_bytes, target, stem)

    return TransformReport(
        asserted=len(abox),
        saturated=len(saturated),
        projected=len(projected),
        suppressed_dropped=len(suppressed),
        denied_cells=len(denied),
        wall_clock_s=time.perf_counter() - start,
        written=written,
    )


def vocab_coverage(maximal: Graph, target: Graph) -> str:
    """A vocabulary-coverage diff (the #34 backlog generator), as Markdown.

    Compares the PREDICATE + CLASS vocabulary of a MAXIMAL(G) output against
    a parity-target graph, grouped by namespace — never triple-exact (the
    instance data differs); the *missing terms* column is the backlog.
    """
    if len(maximal) == 0 or len(target) == 0:
        msg = "vocab_coverage needs two non-empty graphs"
        raise ValueError(msg)

    def vocab_terms(g: Graph) -> set[str]:
        from rdflib import RDF as _RDF

        terms = {str(p) for p in g.predicates()}
        terms.update(
            str(o) for o in g.objects(None, _RDF.type) if isinstance(o, URIRef)
        )
        return terms

    def by_namespace(terms: set[str]) -> dict[str, set[str]]:
        sorted_prefixes = sorted(PREFIXES.items(), key=lambda kv: -len(kv[1]))
        grouped: dict[str, set[str]] = {}
        for term in terms:
            for prefix, ns in sorted_prefixes:
                if term.startswith(ns):
                    grouped.setdefault(prefix, set()).add(term[len(ns) :])
                    break
            else:
                ns = term.rsplit("#", 1)[0] if "#" in term else term.rsplit("/", 1)[0]
                grouped.setdefault(ns, set()).add(term.rsplit("/", 1)[-1])
        return grouped

    ours = by_namespace(vocab_terms(maximal))
    theirs = by_namespace(vocab_terms(target))

    lines = [
        "| vocabulary | terms in target | covered | missing |",
        "|---|---|---|---|",
    ]
    total_target = total_covered = 0
    for vocab_name in sorted(theirs):
        target_terms = theirs[vocab_name]
        covered = target_terms & ours.get(vocab_name, set())
        missing = sorted(target_terms - covered)
        total_target += len(target_terms)
        total_covered += len(covered)
        shown = ", ".join(f"`{m}`" for m in missing[:8])
        if len(missing) > 8:
            shown += f" … +{len(missing) - 8} more"
        lines.append(
            f"| {vocab_name} | {len(target_terms)} | {len(covered)} | {shown or '—'} |"
        )
    lines.append(f"| **total** | **{total_target}** | **{total_covered}** | |")
    return "\n".join(lines) + "\n"
