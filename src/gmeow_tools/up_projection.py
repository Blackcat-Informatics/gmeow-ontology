# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Up-projection — clean-reversal lift + closeMatch claims (consumer RDF → GMEOW, #451).

The first half of the full transpile (#448): lift a non-GMEOW source graph *up*
into pure GMEOW. The module derives its rules from the alignment layer read
backwards — never hand-authored — across an **epistemic ladder**, best→weakest:

* **Clean rule** (``exactMatch``/``equivalent*`` SSSOM, or a structural
  simple-1to1 / direct ``edoalPath`` cell): symmetric, so the target reverses to
  its gmeow counterpart as a **bare fact triple**.
* **Inverse rule** (an ``edoalPath`` cell anchored on its atom's object): the
  down-projection inverted the edge, so the lift re-swaps subject and object —
  still a bare fact.
* **closeMatch claim** (``skos:closeMatch`` SSSOM): the terms are *close but not
  equivalent*, so the target is **not** asserted as fact. It is lifted wrapped in
  a provenance-stamped ``gmeow:StatementMetadata`` claim that quotes the inferred
  triple and hangs ``gmeow:confidence`` (the curator's value) and
  ``gmeow:mappedFrom`` (the source term) off it — best-faithful and refutable,
  never an unmarked overclaim.

Rule resolution is **layer-ranked** (preferred-up-target disambiguation): an
identity match (``exactMatch``/``equivalent*``) outranks a structural projection
of a *narrower* gmeow term to the same external target — so ``schema:name``
reverses to the identity ``gmeow:name`` even though the narrower ``gmeow:fullName``
also projects down to it. Only a target with no identity match falls to the
structural layer.

Doctrine (#448): the output is **pure GMEOW** — only lifted terms appear; a
source term with no rule is reported in the gap, never guessed and never passed
through. Where a target is the down-image of *several* peer gmeow terms with no
ranking winner (rival identities, or rival projections and no identity, in
either the clean or closeMatch layer), the reverse is **ambiguous** and is
deliberately *not* lifted — guessing would fabricate. Subjects, objects, and
literals are carried verbatim; only the predicate / rdf:type IRI is rewritten.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field
from decimal import Decimal, InvalidOperation

from rdflib import RDF, XSD, BNode, Graph, Literal, Namespace, URIRef
from rdflib.term import Node

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


def _sssom_closematch_pairs() -> dict[str, dict[str, str]]:
    """Target IRI → {gmeow IRI: confidence} from ``skos:closeMatch`` cells.

    A closeMatch is *not* an equivalence, so it lifts with a claim (below), not
    as a fact. The confidence is the curator's value carried verbatim for the
    claim's ``gmeow:confidence`` annotation; when a pair recurs across files the
    higher confidence wins (deterministic). A row is skipped unless its
    confidence is a finite value in [0,1] in valid ``xsd:decimal`` lexical form
    (no exponent) — the raw string is emitted as an ``xsd:decimal`` literal, so
    an ill-formed value would produce an ill-typed claim, and a claim without a
    sound confidence loses the very metadata that makes the lift honest.
    """
    pairs: dict[str, dict[str, str]] = defaultdict(dict)
    for path in sorted(MAPPINGS_DIR.glob("*.sssom.tsv")):
        for row in _read_sssom(path):
            bucket, gmeow, target = classify_sssom(
                row["subject_id"], row["predicate_id"], row["object_id"]
            )
            if bucket != "liftable-with-claim":
                continue
            conf = row.get("confidence", "").strip()
            conf_val = _decimal_confidence(conf)
            if conf_val is None:
                continue
            tiri = _to_iri(target)
            if not _in_projection_ns(tiri):
                continue
            giri = _to_iri(gmeow)
            prev = pairs[tiri].get(giri)
            if prev is None or conf_val > Decimal(prev):  # higher confidence wins
                pairs[tiri][giri] = conf
    return pairs


def _decimal_confidence(conf: str) -> Decimal | None:
    """Parse a confidence string to a Decimal in [0,1], or None if unusable.

    Rejects non-decimals, exponent forms (``1e-1`` is valid ``float`` but not
    valid ``xsd:decimal`` lexical form), and NaN/Infinity — so the raw string
    can be emitted verbatim as a well-typed ``xsd:decimal`` literal.
    """
    if "e" in conf.lower():
        return None
    try:
        value = Decimal(conf)
    except InvalidOperation:
        return None
    if not value.is_finite() or not Decimal(0) <= value <= Decimal(1):
        return None
    return value


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
            # classify by which endpoint the anchor binds: subject → direct
            # (plain predicate rewrite), object → inverse (subject↔object swap).
            # A missing anchor, or one matching NEITHER endpoint, is malformed —
            # skip it rather than guess a direction (a wrong guess would shadow
            # the correct rule for the same target and recover the edge reversed).
            anchor = graph.value(pattern, GM.anchor)
            subjvar = graph.value(atoms[0], GM.subjectVar)
            objvar = graph.value(atoms[0], GM.objectVar)
            if anchor is None:
                continue
            if subjvar is not None and anchor == subjvar:
                bucket = direct
            elif objvar is not None and anchor == objvar:
                bucket = inverse
            else:
                continue
            for binding in graph.objects(cell, GM.hasBinding):
                tgt = graph.value(binding, GM.toPredicate)
                if isinstance(tgt, URIRef) and _in_projection_ns(str(tgt)):
                    bucket[str(tgt)].add(str(apred))
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
    # closeMatch targets: lift with a provenance-stamped claim, not a bare fact.
    # target IRI → (gmeow IRI, curated confidence string)
    claim_rules: dict[str, tuple[str, str]] = field(default_factory=dict)


def build_lift_map() -> LiftMap:
    """Derive the unambiguous lift from the alignment layers (incl. inverse paths).

    Rule resolution is **layer-ranked** (preferred-up-target disambiguation,
    #451 stage 3): an SSSOM ``exactMatch``/``equivalent*`` declares term
    *identity*, whose reverse is unambiguous by definition, so it outranks any
    structural *projection* of a narrower gmeow term to the same external target.
    Only when a target has **no** identity match does the structural layer decide
    it; a tie *within* either layer (two identities, or two projections and no
    identity) is genuinely ambiguous and held out — never guessed.
    """
    direct_edoalpath, inverse_edoalpath = _edoalpath_pairs()
    identity = _sssom_clean_pairs()  # exactMatch / equivalent* — term identity
    projection: dict[str, set[str]] = defaultdict(set)  # structural + direct path
    for layer in (_structural_pairs(), direct_edoalpath):
        for target, gmeows in layer.items():
            projection[target] |= gmeows
    rules: dict[str, str] = {}
    ambiguous: dict[str, set[str]] = {}
    for target in set(identity) | set(projection):
        ids = identity.get(target, set())
        if len(ids) == 1:
            # identity wins over any projection collision for the same target
            rules[target] = next(iter(ids))
        elif len(ids) > 1:
            ambiguous[target] = ids  # rival identities — genuinely ambiguous
        else:
            projs = projection.get(target, set())
            if len(projs) == 1:
                rules[target] = next(iter(projs))
            else:
                ambiguous[target] = projs
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
    # closeMatch claims: any clean coverage (direct or inverse fact) wins over a
    # weaker claim; a many-to-one closeMatch collision is ambiguous, held out.
    claim_rules: dict[str, tuple[str, str]] = {}
    for target, gmeow_confs in _sssom_closematch_pairs().items():
        if target in rules or target in inverse_rules or target in ambiguous:
            continue
        if len(gmeow_confs) == 1:
            gmeow, conf = next(iter(gmeow_confs.items()))
            claim_rules[target] = (gmeow, conf)
        else:
            ambiguous[target] = set(gmeow_confs)
    return LiftMap(
        rules=rules,
        ambiguous=ambiguous,
        inverse_rules=inverse_rules,
        claim_rules=claim_rules,
    )


@dataclass
class UpProjection:
    """The result of an up-projection: the GMEOW graph + an honest account.

    ``lifted`` counts bare-fact triples (clean + inverse rules); ``claimed``
    counts closeMatch lifts wrapped in a ``gmeow:StatementMetadata`` claim — the
    two are kept distinct because fact and claim are exactly the doctrine's
    epistemic line.
    """

    graph: Graph  # pure GMEOW
    lifted: int  # source triples lifted as bare facts
    gap_terms: dict[str, int] = field(default_factory=dict)  # uncovered qname → count
    ambiguous_terms: dict[str, int] = field(default_factory=dict)  # skipped → count
    claimed: int = 0  # source triples lifted as provenance-stamped claims
    claim_terms: dict[str, int] = field(default_factory=dict)  # claimed qname → count


def _emit_claim(
    out: Graph, subj: Node, qpred: URIRef, qobj: Node, source_term: URIRef, conf: str
) -> None:
    """Emit one ``gmeow:StatementMetadata`` cell quoting ``(subj qpred qobj)``.

    The cell carries two annotations: ``gmeow:confidence`` (the curator's value)
    and ``gmeow:mappedFrom`` (the source term the claim was lifted from). The
    quoted triple is *not* asserted directly — a closeMatch is close, not equal,
    so the lifted edge exists only inside the reifier, refutable and provenanced.
    """
    cell = BNode()
    out.add((cell, RDF.type, GM.StatementMetadata))
    out.add((cell, GM.qSubject, subj))
    out.add((cell, GM.qPredicate, qpred))
    qobj_pred = GM.qObjectLiteral if isinstance(qobj, Literal) else GM.qObject
    out.add((cell, qobj_pred, qobj))
    for prop, value in (
        (GM.confidence, Literal(conf, datatype=XSD.decimal)),
        (GM.mappedFrom, source_term),
    ):
        ann = BNode()
        out.add((cell, GM.annotation, ann))
        out.add((ann, GM.annProperty, prop))
        out.add((ann, GM.annValue, value))


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
    claimed = 0
    gaps: dict[str, int] = defaultdict(int)
    ambig: dict[str, int] = defaultdict(int)
    claims: dict[str, int] = defaultdict(int)

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
            elif key in lift.claim_rules:
                # a class closeMatch — claim membership, never assert it. A
                # blank-node subject is unquotable (qSubject is IRI-only), so it
                # is skipped — a rule exists, so it is NOT a gap.
                if isinstance(s, URIRef):
                    gmeow, conf = lift.claim_rules[key]
                    _emit_claim(out, s, RDF.type, URIRef(gmeow), URIRef(key), conf)
                    claimed += 1
                    claims[_canon_qname(key)] += 1
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
        elif key in lift.claim_rules:
            # closeMatch — lift with a claim, never as a bare fact. The quoted
            # triple needs an IRI subject and an IRI/literal object (the SHACL
            # StatementMetadata shape); a blank-node endpoint is unquotable, so
            # it is skipped (a rule exists — not a gap).
            if isinstance(s, URIRef) and isinstance(o, URIRef | Literal):
                gmeow, conf = lift.claim_rules[key]
                _emit_claim(out, s, URIRef(gmeow), o, URIRef(key), conf)
                claimed += 1
                claims[_canon_qname(key)] += 1
        else:
            account(key)
    return UpProjection(
        graph=out,
        lifted=lifted,
        gap_terms=dict(gaps),
        ambiguous_terms=dict(ambig),
        claimed=claimed,
        claim_terms=dict(claims),
    )
