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
``gmeow_native.pipeline.alignment_policy`` is the single
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

from dataclasses import dataclass
from types import ModuleType
from typing import TYPE_CHECKING, cast

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import SKOS

from gmeow_tools.config import MAPPING_DSL_DIR, NAMESPACE
from gmeow_tools.export import curie

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from gmeow_rdf.compat.rdflib.term import Node

_GM = NAMESPACE

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


def _pipeline() -> ModuleType:
    from gmeow_native import pipeline

    return cast(ModuleType, pipeline)


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


def saturate(
    abox: Graph,
    *,
    onto: Graph,
    cells: Sequence[Cell],
    denied: set[tuple[str, str, str]],
) -> list[DerivedTriple]:
    """Compute E(G) through the native Rust transform core."""
    abox_nt = abox.serialize(format="nt", encoding="utf-8").decode("utf-8")
    onto_nt = onto.serialize(format="nt", encoding="utf-8").decode("utf-8")
    native_cells = [
        (
            str(cell.iri),
            str(cell.subject),
            cell.predicate_curie,
            str(cell.obj),
            cell.confidence,
        )
        for cell in cells
    ]
    rows = _pipeline().transform_saturate_nt(
        abox_nt, onto_nt, native_cells, sorted(denied)
    )
    return [_derived_from_native(row) for row in rows]


def _derived_from_native(row: dict[str, object]) -> DerivedTriple:
    """Adapt one native E(G) row to the historical dataclass shape."""
    triple = _triple_from_native(
        cast(str, row["subject"]),
        cast(str, row["predicate"]),
        cast(str, row["object"]),
    )
    annotations = tuple(
        _annotation_from_native(pred, obj)
        for pred, obj in cast(list[tuple[str, str]], row["annotations"])
    )
    return DerivedTriple(
        triple=triple,
        reifier=URIRef(cast(str, row["reifier"])),
        annotations=annotations,
    )


def _triple_from_native(
    subject: str, predicate: str, obj: str
) -> tuple[Node, Node, Node]:
    from gmeow_tools.up_projection import _graph_from_native_nt

    graph = _graph_from_native_nt(f"{subject} <{predicate}> {obj} .\n")
    return next(iter(graph))


def _annotation_from_native(predicate: str, obj: str) -> tuple[URIRef, Node]:
    from gmeow_tools.up_projection import _graph_from_native_nt

    graph = _graph_from_native_nt(f"<urn:gmeow-native:ann> <{predicate}> {obj} .\n")
    _s, p, o = next(iter(graph))
    return cast(URIRef, p), o
