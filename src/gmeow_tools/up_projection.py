# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Up-projection — the clean-reversal lift (consumer RDF → GMEOW, #451).

The first half of the full transpile (#448): lift a non-GMEOW source graph *up*
into pure GMEOW. This module is the **clean-reversal** stage — the mechanically
invertible portion the #449 audit identified — and it is derived, not
hand-authored: the lift rules are the alignment layer read backwards.

A lift rule ``target → gmeow`` comes from either layer:

* **SSSOM clean-reversible cells** (``exactMatch``/``equivalent*``): symmetric,
  so the target reverses to its gmeow counterpart unambiguously.
* **Structural simple-1to1 cells** (a plain ``toPredicate``/``toClass`` with one
  ``edoalSource``): the down-cell ``gmeow:X → target:Y`` reverses to ``Y → X``.

Doctrine (#448): the output is **pure GMEOW** — only lifted terms appear; a
source term with no clean rule is reported in the gap, never guessed and never
passed through. Where a target is the down-image of *several* gmeow terms
(a many-to-one projection), the reverse is **ambiguous** and is deliberately
*not* lifted here — it needs a preferred-up-target decision (a later stage),
so guessing would fabricate. Subjects, objects, and literals are carried
verbatim; only the predicate / rdf:type IRI is rewritten.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field

from rdflib import RDF, Graph, Namespace, URIRef

from gmeow_tools.config import MAPPINGS_DIR
from gmeow_tools.up_projection_audit import (
    _canon_qname,
    _in_projection_ns,
    _projection_files,
    _read_sssom,
    _template_atoms,
    _to_iri,
    classify_sssom,
)

GM = Namespace("https://blackcatinformatics.ca/gmeow/")


def _sssom_clean_pairs() -> dict[str, set[str]]:
    """Target IRI → set of gmeow IRIs from clean-reversible SSSOM cells."""
    pairs: dict[str, set[str]] = defaultdict(set)
    for path in sorted(MAPPINGS_DIR.glob("*.sssom.tsv")):
        for row in _read_sssom(path):
            bucket, gmeow, target = classify_sssom(
                row["subject_id"], row["predicate_id"], row["object_id"]
            )
            if bucket != "clean-reversible":
                continue
            tiri = _to_iri(target)
            if _in_projection_ns(tiri):
                pairs[tiri].add(_to_iri(gmeow))
    return pairs


def _structural_pairs() -> dict[str, set[str]]:
    """Target IRI → set of gmeow IRIs from structural simple-1to1 cells."""
    pairs: dict[str, set[str]] = defaultdict(set)
    for path in _projection_files():
        graph = Graph().parse(path, format="turtle")
        for cell in graph.subjects(RDF.type, GM.ProjectionMapping):
            pattern = graph.value(cell, GM.hasMappingPattern)
            if pattern is None:
                continue
            # simple-1to1: no minting, no path/filter guard, a single template leg
            if any(graph.objects(pattern, GM.mint)):
                continue
            if any(graph.objects(pattern, GM.path)) or any(
                graph.objects(pattern, GM.filter)
            ):
                continue
            src = graph.value(pattern, GM.edoalSource)
            if not isinstance(src, URIRef):
                continue
            for binding in graph.objects(cell, GM.hasBinding):
                if len(list(_template_atoms(graph, binding))) > 1:
                    continue
                tgt = graph.value(binding, GM.toPredicate) or graph.value(
                    binding, GM.toClass
                )
                if isinstance(tgt, URIRef) and _in_projection_ns(str(tgt)):
                    pairs[str(tgt)].add(str(src))
    return pairs


@dataclass
class LiftMap:
    """The derived clean-reversal lift, plus the ambiguous targets it skips."""

    rules: dict[str, str]  # target IRI → the single gmeow IRI it lifts to
    ambiguous: dict[str, set[str]]  # target IRI → the rival gmeow IRIs (skipped)


def build_lift_map() -> LiftMap:
    """Derive the unambiguous ``target → gmeow`` lift from both alignment layers."""
    merged: dict[str, set[str]] = defaultdict(set)
    for layer in (_sssom_clean_pairs(), _structural_pairs()):
        for target, gmeows in layer.items():
            merged[target] |= gmeows
    rules: dict[str, str] = {}
    ambiguous: dict[str, set[str]] = {}
    for target, gmeows in merged.items():
        if len(gmeows) == 1:
            rules[target] = next(iter(gmeows))
        else:
            ambiguous[target] = gmeows
    return LiftMap(rules=rules, ambiguous=ambiguous)


@dataclass
class UpProjection:
    """The result of an up-projection: the GMEOW graph + an honest gap account."""

    graph: Graph  # pure GMEOW
    lifted: int  # source triples lifted
    gap_terms: dict[str, int] = field(default_factory=dict)  # uncovered qname → count
    ambiguous_terms: dict[str, int] = field(default_factory=dict)  # skipped → count


def up_project(source: Graph, lift: LiftMap | None = None) -> UpProjection:
    """Lift a consumer-vocabulary graph up to pure GMEOW via the clean rules.

    Each triple's predicate (and rdf:type object) is rewritten to its gmeow
    counterpart when a clean rule exists; subjects/objects/literals are carried
    verbatim. Terms with no clean rule are accounted in the gap (or, when the
    reverse is ambiguous, in ``ambiguous_terms``) — never guessed.
    """
    if len(source) == 0:
        raise ValueError("up_project: source graph is empty")
    if lift is None:
        lift = build_lift_map()
    out = Graph()
    out.bind("gmeow", GM)
    lifted = 0
    gaps: dict[str, int] = defaultdict(int)
    ambig: dict[str, int] = defaultdict(int)

    def resolve(term: URIRef) -> URIRef | None:
        key = str(term)
        if key in lift.rules:
            return URIRef(lift.rules[key])
        if key in lift.ambiguous:
            ambig[_canon_qname(key)] += 1
        elif _in_projection_ns(key):
            gaps[_canon_qname(key)] += 1
        return None

    for s, p, o in source:
        if p == RDF.type and isinstance(o, URIRef):
            lifted_o = resolve(o)
            if lifted_o is not None:
                out.add((s, RDF.type, lifted_o))
                lifted += 1
            continue
        if isinstance(p, URIRef):
            lifted_p = resolve(p)
            if lifted_p is not None:
                out.add((s, lifted_p, o))
                lifted += 1
    return UpProjection(
        graph=out, lifted=lifted, gap_terms=dict(gaps), ambiguous_terms=dict(ambig)
    )
