# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Native-RL ≡ owlrl-RL agreement axis of the classic-cross-check lane (#666, Task 5).

The native OWL 2 RL engine (``gmeow_tools.native_rl.native_rl_closure``) is the
**primary** Docker-free entailment authority; the 8 converted conformance suites
run on it. ``owlrl`` is no longer on the primary path — it lives HERE, in the
classic-cross-check lane, as the agreement ORACLE that proves the native RL
closure matches the reference pure-Python RL reasoner over the actual ontology.

This axis is LANE-ONLY and ENFORCING (strict, no knob): it fails the lane on any
genuine RL divergence. It is never part of ``make check`` or the required gate.

What "agreement" means (honest, not a fudge)
--------------------------------------------
Two correct OWL 2 RL reasoners agree on the *non-tautological named-vocabulary
closure*, but a maximal RDF-Based-Semantics engine like ``owlrl`` additionally
materializes a large set of trivially-true axioms a practical engine omits, and
the two mint blank nodes independently. The comparison therefore canonicalizes
both closures by:

* dropping RL **tautologies / housekeeping** axioms (reflexive ``scm-*`` such as
  ``C rdfs:subClassOf C``; ``x a owl:Thing`` / ``rdfs:Resource``; ``x owl:sameAs
  x``; ``P rdfs:domain/range owl:Thing``; literal-value datatype typing; datatype
  disjointness) — see :data:`_TAUTOLOGY` ;
* restricting to the **named-vocabulary** surface (no blank-node subject/object) —
  the surface the suites assert over; the restriction/list blank-node structure
  is reasoner-internal and never asserted ;
* normalizing literals to oxigraph's integer value-space (``xsd:nonNegativeInteger
  → xsd:integer`` etc.), the same normalization the native engine's oxigraph
  round-trip applies, so the two sides compare value-for-value.

Every divergence that survives this canonicalization is a REAL disagreement and
fails the lane. Empirically (over the full ontology) the surviving set is empty —
native RL and owlrl agree exactly.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import TYPE_CHECKING

from rdflib import OWL, RDF, RDFS, BNode, Graph, Literal, URIRef
from rdflib.term import Node

from gmeow_tools.config import DIST_DIR, GTS_SNAPSHOT_FILE

if TYPE_CHECKING:
    from gmeow_tools.diagnostics import DiagnosticsReport

#: Diagnostics-artifact stem for the RL-agreement axis (JSON / SARIF / HTML).
RL_AGREEMENT_STEM = "gmeow-classic-cross-check-rl"

# Gate-taxonomy rule-ids (#666 owns these; #662 consumes them).
RULE_RL_DIVERGENCE = "classic-cross-check/rl-divergence"
RULE_RL_AGREEMENT = "classic-cross-check/rl-agreement"
RULE_RL_TIMING = "classic-cross-check/rl-timing"

_XSD = "http://www.w3.org/2001/XMLSchema#"
_RDFS_RESOURCE = URIRef("http://www.w3.org/2000/01/rdf-schema#Resource")

#: The xsd integer hierarchy — oxigraph normalizes every member to ``xsd:integer``
#: on parse, so the native closure (which round-trips through oxigraph) reports
#: ``xsd:integer`` where owlrl keeps the authored narrower datatype. Normalizing
#: both sides to ``xsd:integer`` compares the literals value-for-value.
_INT_HIERARCHY = frozenset(
    f"{_XSD}{name}"
    for name in (
        "integer",
        "nonNegativeInteger",
        "positiveInteger",
        "int",
        "long",
        "short",
        "byte",
        "nonPositiveInteger",
        "negativeInteger",
        "unsignedInt",
        "unsignedLong",
        "unsignedShort",
        "unsignedByte",
    )
)

#: rdf:type objects that are RL axiomatic / housekeeping types (never asserted-on
#: by the suites; both reasoners differ only in how exhaustively they emit them).
_TRIVIAL_TYPES = frozenset(
    {
        OWL.Class,
        RDFS.Class,
        OWL.ObjectProperty,
        OWL.DatatypeProperty,
        RDF.Property,
        OWL.NamedIndividual,
        RDFS.Datatype,
        OWL.AnnotationProperty,
        OWL.Thing,
        _RDFS_RESOURCE,
    }
)


def _is_tautology(triple: tuple[Node, Node, Node]) -> bool:
    """Return True for an RL tautology / housekeeping axiom (see module docstring)."""
    s, p, o = triple
    if isinstance(s, Literal):
        # owlrl's D-entailment emits literal-subject triples (non-standard RDF);
        # the native authority has no literal-subject form.
        return True
    if p == OWL.sameAs and s == o:
        return True
    if p == RDF.type and isinstance(o, Literal):
        return True
    if p == RDF.type and o in _TRIVIAL_TYPES:
        return True
    if p == RDFS.subClassOf and (s == o or o == OWL.Thing or s == OWL.Nothing):
        return True
    if p == OWL.equivalentClass and s == o:
        return True
    if p == RDFS.subPropertyOf and s == o:
        return True
    if p == OWL.equivalentProperty and s == o:
        return True
    if p in (RDFS.domain, RDFS.range) and o in (OWL.Thing, _RDFS_RESOURCE):
        return True
    return bool(
        p == OWL.disjointWith and isinstance(s, URIRef) and str(s).startswith(_XSD)
    )


def _norm_object(obj: Node) -> tuple[str, ...]:
    """Canonical key for an object term (integer value-space normalized)."""
    if isinstance(obj, Literal):
        datatype = str(obj.datatype) if obj.datatype else None
        if datatype in _INT_HIERARCHY:
            datatype = f"{_XSD}integer"
        return ("lit", str(obj), str(datatype), str(obj.language))
    return ("iri", str(obj))


def canonical_named_closure(graph: Graph) -> set[tuple[str, str, tuple[str, ...]]]:
    """Canonicalize an RL closure to its comparable named-vocabulary surface.

    Drops blank-node-bearing triples (reasoner-internal restriction/list
    structure) and RL tautologies, and normalizes literals to oxigraph's integer
    value-space — the canonical form both engines are compared in.
    """
    out: set[tuple[str, str, tuple[str, ...]]] = set()
    for s, p, o in graph:
        if isinstance(s, BNode) or isinstance(o, BNode):
            continue
        if _is_tautology((s, p, o)):
            continue
        out.add((str(s), str(p), _norm_object(o)))
    return out


def _told_graph(gts: Path) -> Graph:
    """Load the native TOLD (asserted) closure basis as an rdflib graph.

    Reuses the lane's apples-to-apples corpus: the native engine's asserted
    axioms, the SAME facts both RL closures are computed over (so any divergence
    is a reasoner disagreement, not an input mismatch).
    """
    from gmeow_tools import classic_cross_check as crosscheck

    native = crosscheck.reason_native(gts)
    told_facts = crosscheck.write_told_facts(native)
    graph = Graph()
    graph.parse(str(told_facts), format="turtle")
    return graph


def compare(gts: Path = GTS_SNAPSHOT_FILE) -> dict[str, object]:
    """Compute native + owlrl RL closures over the told facts and diff them.

    Returns a dict with the agreement tallies, the (canonicalized) divergence
    rows, and the per-engine wall-clock timing.
    """
    import owlrl

    from gmeow_tools.native_rl import native_rl_closure

    base = _told_graph(gts)

    native_graph = Graph()
    native_graph += base
    start = time.monotonic()
    native_rl_closure(native_graph)
    native_seconds = time.monotonic() - start

    owlrl_graph = Graph()
    owlrl_graph += base
    start = time.monotonic()
    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(owlrl_graph)
    owlrl_seconds = time.monotonic() - start

    native_set = canonical_named_closure(native_graph)
    owlrl_set = canonical_named_closure(owlrl_graph)

    native_only = sorted(native_set - owlrl_set)
    oracle_only = sorted(owlrl_set - native_set)
    agree = len(native_set & owlrl_set)

    return {
        "agree": agree,
        "native_only": native_only,
        "oracle_only": oracle_only,
        "native_seconds": native_seconds,
        "owlrl_seconds": owlrl_seconds,
    }


def _divergence_detail(kind: str, row: tuple[str, str, tuple[str, ...]]) -> str:
    """Render a canonicalized divergence row as a human-readable detail string."""
    s, p, o = row
    return f"{kind}: <{s}> <{p}> {o!r}"


def build_report(result: dict[str, object]) -> DiagnosticsReport:
    """Build the diagnostics report for the RL-agreement axis.

    Every divergence row becomes an ``error`` finding (it FAILS the lane); the
    agreement count + per-engine timing are ``note`` findings.
    """
    from gmeow_tools import diagnostics

    report = diagnostics.report(tool="classic-cross-check")

    native_seconds = float(result["native_seconds"])  # type: ignore[arg-type]
    owlrl_seconds = float(result["owlrl_seconds"])  # type: ignore[arg-type]
    report.add(
        diagnostics.finding(
            severity="note",
            code=RULE_RL_TIMING,
            message=f"native RL closure ran in {native_seconds:.2f}s",
            tool="native-rl",
            tags=["timing"],
        )
    )
    report.add(
        diagnostics.finding(
            severity="note",
            code=RULE_RL_TIMING,
            message=f"owlrl RL closure oracle ran in {owlrl_seconds:.2f}s",
            tool="owlrl",
            tags=["timing"],
        )
    )

    native_only = result["native_only"]
    oracle_only = result["oracle_only"]
    assert isinstance(native_only, list)
    assert isinstance(oracle_only, list)
    report.add(
        diagnostics.finding(
            severity="note",
            code=RULE_RL_AGREEMENT,
            message=(
                f"RL agreement: agree={result['agree']} "
                f"native_only={len(native_only)} oracle_only={len(oracle_only)}"
            ),
            tool="classic-cross-check",
            tags=["agreement-matrix", "rl"],
        )
    )

    for row in native_only:
        report.add(
            diagnostics.finding(
                severity="error",
                code=RULE_RL_DIVERGENCE,
                message=_divergence_detail("NativeOnly", row),
                tool="native-rl",
                tags=["NativeOnly", "rl"],
            )
        )
    for row in oracle_only:
        report.add(
            diagnostics.finding(
                severity="error",
                code=RULE_RL_DIVERGENCE,
                message=_divergence_detail("OracleOnly", row),
                tool="owlrl",
                tags=["OracleOnly", "rl"],
            )
        )
    return report


def enforce(result: dict[str, object]) -> bool:
    """Strict verdict: pass iff the canonicalized closures agree exactly.

    Any ``native_only`` or ``oracle_only`` row is a real RL divergence and FAILS
    the lane. There is no severity knob (ETHOS §5/§19).
    """
    native_only = result["native_only"]
    oracle_only = result["oracle_only"]
    assert isinstance(native_only, list)
    assert isinstance(oracle_only, list)
    return len(native_only) == 0 and len(oracle_only) == 0


def run(
    *,
    gts: Path = GTS_SNAPSHOT_FILE,
    output_dir: Path = DIST_DIR,
) -> tuple[bool, dict[str, object], DiagnosticsReport]:
    """Run the enforced RL-agreement axis and write the SARIF/JSON artifacts.

    Returns ``(passed, result, report)``. ``passed`` is the strict verdict; the
    caller exits non-zero when it is False. Artifacts are a LANE output (under
    ``output_dir``), never a committed drift-gated generator.
    """
    from gmeow_tools import diagnostics

    result = compare(gts)
    report = build_report(result)
    output_dir.mkdir(parents=True, exist_ok=True)
    diagnostics.write_report_artifacts(
        report, output_dir=output_dir, stem=RL_AGREEMENT_STEM
    )
    passed = enforce(result)
    return passed, result, report
