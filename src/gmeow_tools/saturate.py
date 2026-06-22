# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Equivalence saturation — the E(G) of the transpiler (#34).

For every instance triple whose GMEOW term carries a *strong* equivalence to
an external term, emit the parallel cross-vocabulary triple: ``:me a
gmeow:Person`` becomes *simultaneously* ``foaf:Person`` / ``schema:Person`` /
``vcard:Individual``; ``:me gmeow:fullName "…"`` becomes ``foaf:name`` /
``schema:name`` / ``vcard:fn``. Term-level, lossless, broad — the structural
(lossy) half is the projection engine, P(G).

The correctness keystone: only STRONG predicates materialize —
:data:`gmeow_tools.alignment_lint.STRONG_CLASS_PREDICATES` /
:data:`~gmeow_tools.alignment_lint.STRONG_PROPERTY_PREDICATES` are the single
source of truth, so linter and saturator agree by construction.
``skos:closeMatch`` (a hint), ``broadMatch``/``narrowMatch`` (hierarchy), and
any cell the direction lint rates ERROR never materialize. Suppression is
honored fail-closed (#282): a node carrying ``gmeow:displayable false`` (or
an appellation whose bearer does) contributes no derived triple — a deadname
must not leak into five vocabularies at once.

Every derived triple carries inline RDF 1.2 provenance: a content-addressed
reifier annotated ``gmeow:mappedFrom`` → the authored ``gmeow:TermEquivalence``
cell IRI (and ``gmeow:confidence`` when the cell records one) — the audit
trail the hand-coded approach never had. Zero new ontology terms (P15).
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import TYPE_CHECKING

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import SKOS, XSD

from gmeow_tools.alignment_lint import (
    STRONG_CLASS_PREDICATES,
    STRONG_PROPERTY_PREDICATES,
)
from gmeow_tools.config import MAPPING_DSL_DIR, NAMESPACE
from gmeow_tools.export import curie

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence
    from pathlib import Path

    from gmeow_rdf.compat.rdflib.term import Node

    from gmeow_tools.mapping_compile import SuppressionVocab

_GM = NAMESPACE
_DISPLAYABLE = URIRef(_GM + "displayable")
_COARSEN_TO = URIRef(_GM + "coarsenTo")
_MAPPED_FROM = URIRef(_GM + "mappedFrom")
_CONFIDENCE = URIRef(_GM + "confidence")
_SCHEMA_SAME_AS = URIRef("https://schema.org/sameAs")

#: The fixed sameAs-mirror rule's provenance IRI — an individual reference
#: (like the projection alignment IRIs), not a minted term.
SAME_AS_MIRROR_RULE = URIRef(_GM + "rules/sameAsMirror")

#: Mapping-predicate IRI → the CURIE form the lint findings use.
_PREDICATE_CURIES: dict[URIRef, str] = {
    OWL.equivalentClass: "owl:equivalentClass",
    OWL.equivalentProperty: "owl:equivalentProperty",
    SKOS.exactMatch: "skos:exactMatch",
    SKOS.closeMatch: "skos:closeMatch",
    SKOS.relatedMatch: "skos:relatedMatch",
    SKOS.broadMatch: "skos:broadMatch",
    SKOS.narrowMatch: "skos:narrowMatch",
}


@dataclass(frozen=True, slots=True)
class DerivedTriple:
    """One derived triple plus its inline-provenance rows.

    The base triple stays ASSERTED (plain-RDF readable); the reifier and
    annotations ride the RDF 1.2 statement layer.
    """

    triple: tuple[Node, Node, Node]
    reifier: URIRef
    annotations: tuple[tuple[URIRef, Node], ...]


@dataclass(frozen=True, slots=True)
class Cell:
    """One authored ``gmeow:TermEquivalence`` cell (the audit anchor)."""

    iri: URIRef
    subject: URIRef
    predicate_curie: str
    obj: URIRef
    confidence: str  # lexical decimal, "" when absent

    @property
    def denial_key(self) -> tuple[str, str, str]:
        """The (subject, predicate, object) CURIEs the lint findings use."""
        return (curie(str(self.subject)), self.predicate_curie, curie(str(self.obj)))


def load_cells(dsl_dir: Path | None = None) -> list[Cell]:
    """Read every authored TermEquivalence cell from the mapping DSL.

    Cells live in TWO authoring locations (the compiler reads both): the
    shared ``dsl/mappings/equivalences/`` directory and the slice-owned
    ``slices/*/*/mappings/*.ttl`` files (#287). Phase 1 read only the
    former — slice-owned strong cells were invisible to saturation.
    """
    from gmeow_tools.slices import iter_slice_mapping_files

    directory = dsl_dir if dsl_dir is not None else MAPPING_DSL_DIR / "equivalences"
    g = Graph()
    if directory.is_dir():
        paths = sorted(directory.glob("*.ttl"))
        if dsl_dir is None:
            paths += iter_slice_mapping_files()  # already sorted (slices.py)
        if not paths:
            msg = f"no mapping .ttl files found under {directory}"
            raise FileNotFoundError(msg)
        for path in paths:
            g.parse(path, format="turtle")
    elif dsl_dir is not None:
        msg = f"equivalence DSL directory not found: {directory}"
        raise FileNotFoundError(msg)
    else:
        # Wheel-only install (no source tree): read the equivalence + slice cells
        # folded into the bundle (#bundle — the CLI razor: gmeow needs no repo).
        from gmeow_tools.bundle import bundled_cells_under

        cells_bytes = bundled_cells_under("dsl/mappings/equivalences/")
        if not cells_bytes:
            raise FileNotFoundError(f"no bundled cells under {directory}")
        for _rel, data in sorted(cells_bytes.items()):
            g.parse(data=data, format="turtle")
    cells: list[Cell] = []
    for cell in sorted(g.subjects(RDF.type, URIRef(_GM + "TermEquivalence")), key=str):
        subj = g.value(cell, URIRef(_GM + "alignSubject"))
        pred = g.value(cell, URIRef(_GM + "alignPredicate"))
        obj = g.value(cell, URIRef(_GM + "alignObject"))
        if not (
            isinstance(cell, URIRef)
            and isinstance(subj, URIRef)
            and isinstance(pred, URIRef)
            and isinstance(obj, URIRef)
        ):
            continue
        confidence = g.value(cell, URIRef(_GM + "confidence"))
        cells.append(
            Cell(
                iri=cell,
                subject=subj,
                predicate_curie=_PREDICATE_CURIES.get(pred, curie(str(pred))),
                obj=obj,
                confidence=str(confidence) if confidence is not None else "",
            )
        )
    return cells


def build_strong_edges(
    cells: Iterable[Cell],
    onto: Graph,
    denied: set[tuple[str, str, str]],
) -> tuple[dict[URIRef, list[Cell]], dict[URIRef, list[Cell]]]:
    """Partition authorized strong cells into class edges and property edges.

    Kind comes from the merged ontology (mapping files mix kinds); the
    direction stays GMEOW→target (non-gmeow subjects never saturate); ERROR
    rows from the direction lint are refused.
    """
    class_edges: dict[URIRef, list[Cell]] = {}
    property_edges: dict[URIRef, list[Cell]] = {}
    property_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for cell in cells:
        if not str(cell.subject).startswith(_GM):
            continue
        if cell.denial_key in denied:
            continue
        is_class = (cell.subject, RDF.type, OWL.Class) in onto
        is_property = any((cell.subject, RDF.type, t) in onto for t in property_types)
        if is_class and cell.predicate_curie in STRONG_CLASS_PREDICATES:
            class_edges.setdefault(cell.subject, []).append(cell)
        elif is_property and cell.predicate_curie in STRONG_PROPERTY_PREDICATES:
            property_edges.setdefault(cell.subject, []).append(cell)
    return class_edges, property_edges


def suppressed_nodes(abox: Graph, vocab: SuppressionVocab) -> set[Node]:
    """Nodes that must not contribute derived triples (#282, fail-closed).

    A node is suppressed when it carries ``gmeow:displayable false`` itself,
    or when it is an appellation whose bearer does (the bearer guard).
    """
    suppressed: set[Node] = {
        s
        for s, o in abox.subject_objects(_DISPLAYABLE)
        if isinstance(o, Literal) and not o.toPython()
    }
    if not suppressed:
        return suppressed
    appellations: set[Node] = set()
    for cls in vocab.appellation_classes:
        appellations.update(abox.subjects(RDF.type, cls))
    for prop in vocab.appellation_domain_props:
        appellations.update(abox.subjects(prop, None))
    extra: set[Node] = set()
    for prop in vocab.bearer_props:
        for bearer, appellation in abox.subject_objects(prop):
            if bearer in suppressed and appellation in appellations:
                extra.add(appellation)
    return suppressed | extra


def reifier_for(s: Node, p: Node, o: Node) -> URIRef:
    """A content-addressed reifier IRI — stable across runs (diffable)."""
    digest = hashlib.sha256(f"{s.n3()}|{p.n3()}|{o.n3()}".encode()).hexdigest()[:16]
    return URIRef(f"{_GM}derivations/{digest}")


def _cell_annotations(cell: Cell) -> tuple[tuple[URIRef, Node], ...]:
    rows: list[tuple[URIRef, Node]] = [(_MAPPED_FROM, cell.iri)]
    if cell.confidence:
        rows.append((_CONFIDENCE, Literal(cell.confidence, datatype=XSD.decimal)))
    return tuple(rows)


def saturate(
    abox: Graph,
    *,
    onto: Graph,
    cells: Sequence[Cell],
    denied: set[tuple[str, str, str]],
    vocab: SuppressionVocab,
) -> list[DerivedTriple]:
    """Compute E(G): the strong-equivalence saturation of an A-Box.

    Args:
        abox: The canonical instance graph (read-only; never chained).
        onto: The merged ontology (term-kind classification + guards).
        cells: The authored equivalence cells (:func:`load_cells`).
        denied: Lint-refused ``(subject, predicate, object)`` CURIE rows.
        vocab: The suppression vocabulary (#282 guards).

    Returns:
        Derived triples sorted by N-Triples token, each carrying its
        provenance reifier; triples already asserted in ``abox`` are skipped,
        and a triple derivable through several cells gets ONE reifier with
        merged annotations.
    """
    class_edges, property_edges = build_strong_edges(cells, onto, denied)
    suppressed = suppressed_nodes(abox, vocab)

    derived: dict[tuple[Node, Node, Node], set[tuple[URIRef, Node]]] = {}

    def emit(
        s: Node, p: Node, o: Node, annotations: Iterable[tuple[URIRef, Node]]
    ) -> None:
        if (s, p, o) in abox:
            return
        derived.setdefault((s, p, o), set()).update(annotations)

    for s, _p, cls in abox.triples((None, RDF.type, None)):
        if s in suppressed or not isinstance(cls, URIRef):
            continue
        for cell in class_edges.get(cls, []):
            emit(s, RDF.type, cell.obj, _cell_annotations(cell))

    for prop, edge_cells in property_edges.items():
        for s, o in abox.subject_objects(prop):
            if s in suppressed or o in suppressed:
                continue
            if prop in vocab.coarsen_guarded and (s, _COARSEN_TO, None) in abox:
                continue  # the precise value must not pass the coarsen mark
            for cell in edge_cells:
                emit(s, cell.obj, o, _cell_annotations(cell))

    # The fixed sameAs-mirror rule: instance-level external identity links
    # pass through and mirror to schema:sameAs (the redundancy today's
    # hand-coded profiles maintain by hand).
    for s, o in abox.subject_objects(OWL.sameAs):
        if s in suppressed or o in suppressed:
            continue
        emit(s, _SCHEMA_SAME_AS, o, [(_MAPPED_FROM, SAME_AS_MIRROR_RULE)])

    return [
        DerivedTriple(
            triple=(s, p, o),
            reifier=reifier_for(s, p, o),
            annotations=tuple(
                sorted(annotations, key=lambda row: (str(row[0]), str(row[1])))
            ),
        )
        for (s, p, o), annotations in sorted(
            derived.items(), key=lambda item: tuple(n.n3() for n in item[0])
        )
    ]
