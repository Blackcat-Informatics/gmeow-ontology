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

from rdflib import RDF, BNode, Graph, Namespace, URIRef

from gmeow_tools.config import MAPPINGS_DIR
from gmeow_tools.up_projection_audit import (
    _canon_qname,
    _in_projection_ns,
    _projection_files,
    _rdf_list,
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


def _edoalpath_pairs() -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    """``(direct, inverse)`` target IRI → gmeow IRIs from single-atom edoalPath cells.

    An ``edoalPath`` cell traverses one atom whose *predicate* is the gmeow term.
    When the pattern's anchor is the atom's OBJECT, the down-projection inverted
    the edge (e.g. ``subOrganizationOf`` child→parent emitted ``schema:department``
    parent→child), so the up-lift must swap subject and object. Multi-atom / minting
    edoalPath cells are left to a later structural stage.
    """
    direct: dict[str, set[str]] = defaultdict(set)
    inverse: dict[str, set[str]] = defaultdict(set)
    for path in _projection_files():
        graph = Graph().parse(path, format="turtle")
        for cell in graph.subjects(RDF.type, GM.ProjectionMapping):
            pattern = graph.value(cell, GM.hasMappingPattern)
            if pattern is None or not any(graph.objects(pattern, GM.edoalPath)):
                continue
            if any(graph.objects(pattern, GM.mint)):
                continue
            atoms = _rdf_list(graph, graph.value(pattern, GM.atom))
            if len(atoms) != 1:
                continue
            apred = graph.value(atoms[0], GM.predicate)
            if not isinstance(apred, URIRef):
                continue
            anchor = graph.value(pattern, GM.anchor)
            objvar = graph.value(atoms[0], GM.objectVar)
            # a missing anchor/objectVar must NOT read as inverse — guard the
            # None==None trap before comparing the variable names
            is_inverse = anchor is not None and objvar is not None and anchor == objvar
            for binding in graph.objects(cell, GM.hasBinding):
                tgt = graph.value(binding, GM.toPredicate)
                if isinstance(tgt, URIRef) and _in_projection_ns(str(tgt)):
                    (inverse if is_inverse else direct)[str(tgt)].add(str(apred))
    return direct, inverse


@dataclass
class LiftMap:
    """The derived lift.

    Holds the direct rules, the direction-swapped inverse rules, and the
    ambiguous targets held out of both.
    """

    rules: dict[str, str]  # target IRI → the single gmeow IRI it lifts to
    ambiguous: dict[str, set[str]]  # target IRI → the rival gmeow IRIs (skipped)
    # inverse-path targets: lift with a subject↔object swap
    inverse_rules: dict[str, str] = field(default_factory=dict)


def build_lift_map() -> LiftMap:
    """Derive the unambiguous lift from the alignment layers (incl. inverse paths)."""
    direct_edoalpath, inverse_edoalpath = _edoalpath_pairs()
    merged: dict[str, set[str]] = defaultdict(set)
    for layer in (_sssom_clean_pairs(), _structural_pairs(), direct_edoalpath):
        for target, gmeows in layer.items():
            merged[target] |= gmeows
    rules: dict[str, str] = {}
    ambiguous: dict[str, set[str]] = {}
    for target, gmeows in merged.items():
        if len(gmeows) == 1:
            rules[target] = next(iter(gmeows))
        else:
            ambiguous[target] = gmeows
    # inverse rules: a direct (non-swap) rule, when one exists, always wins; a
    # many-to-one inverse collision is ambiguous, never silently dropped (so
    # up_project reports it honestly instead of miscounting it as a gap).
    inverse_rules: dict[str, str] = {}
    for target, gmeows in inverse_edoalpath.items():
        if target in rules or target in ambiguous:
            continue
        if len(gmeows) == 1:
            inverse_rules[target] = next(iter(gmeows))
        else:
            ambiguous[target] = gmeows
    return LiftMap(rules=rules, ambiguous=ambiguous, inverse_rules=inverse_rules)


@dataclass
class UpProjection:
    """The result of an up-projection: the GMEOW graph + an honest gap account."""

    graph: Graph  # pure GMEOW
    lifted: int  # source triples lifted
    gap_terms: dict[str, int] = field(default_factory=dict)  # uncovered qname → count
    ambiguous_terms: dict[str, int] = field(default_factory=dict)  # skipped → count


def up_project(source: Graph, lift: LiftMap | None = None) -> UpProjection:
    """Lift a consumer-vocabulary graph up to pure GMEOW via the derived rules.

    Each triple's predicate (and rdf:type object) is rewritten to its gmeow
    counterpart: a direct rule keeps the edge, an inverse-path rule swaps subject
    and object (undoing an inverted down-projection). Subjects/objects/literals
    are carried verbatim. Terms with no rule are accounted in the gap (or, when
    the reverse is ambiguous, in ``ambiguous_terms``) — never guessed.
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

    def account(key: str) -> None:
        if key in lift.ambiguous:
            ambig[_canon_qname(key)] += 1
        elif _in_projection_ns(key):
            gaps[_canon_qname(key)] += 1

    for s, p, o in source:
        if p == RDF.type and isinstance(o, URIRef):
            key = str(o)
            if key in lift.rules:  # rdf:type is never inverted
                out.add((s, RDF.type, URIRef(lift.rules[key])))
                lifted += 1
            else:
                account(key)
            continue
        if not isinstance(p, URIRef):
            continue
        key = str(p)
        if key in lift.rules:
            out.add((s, URIRef(lift.rules[key]), o))
            lifted += 1
        elif key in lift.inverse_rules:
            # a rule exists — this is not a gap. A literal object is skipped
            # (it cannot become a subject after the swap), not accounted.
            if isinstance(o, URIRef | BNode):
                out.add((o, URIRef(lift.inverse_rules[key]), s))
                lifted += 1
        else:
            account(key)
    return UpProjection(
        graph=out, lifted=lifted, gap_terms=dict(gaps), ambiguous_terms=dict(ambig)
    )
