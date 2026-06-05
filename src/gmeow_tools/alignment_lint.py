"""SSSOM alignment-direction lint: catch mappings to inverse / mismatched terms.

PR #24's :mod:`gmeow_tools.projection_lint` verifies the projection artifacts
against *each other*. This module verifies the SSSOM property alignments against
the **target vocabularies' own axioms** — the one class of error domain/range
checks on GMEOW alone cannot see.

The motivating bug: ``gmeow:subOrganizationOf`` (child→parent) was mapped via
``skos:closeMatch`` to ``schema:subOrganization`` (parent→child). Both ends are
``Organization``, so nothing GMEOW-internal flags it. But schema.org declares
``schema:subOrganization owl:inverseOf schema:parentOrganization`` — and GMEOW
*also* maps ``subOrganizationOf`` to ``schema:parentOrganization``. Mapping one
property to **both a term and that term's inverse** is self-contradictory; the
weaker of the two is the direction error.

Three checks, each degrading to a non-fatal ``INFO`` when the target axioms are
absent (so the gate is useful offline without false positives):

* :func:`_check_inverse_direction` — the self-contradiction detector above, plus a
  domain/range orientation fallback when only the wrong term is mapped.
* :func:`_check_domain_range` — the GMEOW term's domain/range must be compatible
  (via the SSSOM class bridge) with the target term's.
* :func:`_check_property_character` — ``owl:equivalentProperty`` between a
  functional/transitive/symmetric property and a target lacking that character,
  or an object-vs-datatype kind conflict.

Target axioms come from :mod:`gmeow_tools.target_axioms` (vendored snapshots for
IMPORT_OK targets, a hand-authored fixture for reference-only schema.org, and a
live fetch under ``allow_network``).
"""

from __future__ import annotations

import contextlib
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    MAPPINGS_DIR,
    PREFIXES,
    PROJECT_ROOT,
    TARGET_SNAPSHOT_DIR,
)
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mappings import Mapping, expand_curie, load_mappings
from gmeow_tools.target_axioms import (
    SCHEMA_DOMAIN_INCLUDES,
    SCHEMA_INVERSE_OF,
    SCHEMA_RANGE_INCLUDES,
    TARGET_SOURCES,
    fetch_target_axioms,
    load_target_snapshot,
)
from gmeow_tools.validate import ValidationResult

#: Hand-authored, validation-only target axioms for reference-only vocabularies
#: (schema.org). Never published; a handful of structural facts, not a copy.
TARGET_FIXTURE_DIR = PROJECT_ROOT / "tests" / "fixtures" / "target_axioms"

#: Predicate CURIEs whose alignment asserts (near-)equivalence — a direction or
#: character conflict here is a hard error.
_STRONG_PREDICATES: frozenset[str] = frozenset(
    {"owl:equivalentProperty", "skos:exactMatch"}
)
#: Intentionally directional/hierarchical predicates — exempt from direction checks.
#: (``skos:closeMatch`` is fuzzy "related, not identical": conflicts there are
#: warnings, not errors — see :func:`_severity_for`.)
_HIERARCHICAL_PREDICATES: frozenset[str] = frozenset(
    {"skos:broadMatch", "skos:narrowMatch", "rdfs:subPropertyOf"}
)

#: Strength rank used to pick the canonical term in a self-contradicting pair.
_PREDICATE_RANK: dict[str, int] = {
    "owl:equivalentProperty": 3,
    "skos:exactMatch": 3,
    "skos:closeMatch": 1,
}

_CHARACTER_TYPES: tuple[URIRef, ...] = (
    OWL.FunctionalProperty,
    OWL.InverseFunctionalProperty,
    OWL.TransitiveProperty,
    OWL.SymmetricProperty,
)

#: OWL property-typing terms. A target that uses none of these does not speak the
#: OWL characteristic vocabulary, so a character comparison would be noise.
_OWL_PROPERTY_TYPES: frozenset[URIRef] = frozenset(
    {
        OWL.ObjectProperty,
        OWL.DatatypeProperty,
        OWL.FunctionalProperty,
        OWL.InverseFunctionalProperty,
        OWL.TransitiveProperty,
        OWL.SymmetricProperty,
        OWL.AsymmetricProperty,
    }
)


class Severity(StrEnum):
    """Severity of an alignment finding."""

    ERROR = "error"  # confident violation → hard fail
    WARNING = "warning"  # plausible mismatch, target axioms ambiguous
    INFO = "info"  # target axioms absent → skipped, never fails the gate


@dataclass(frozen=True, slots=True)
class AlignmentFinding:
    """One alignment-direction finding (a mapping row + what is wrong with it)."""

    severity: Severity
    check: str  # "inverse-direction" | "domain-range" | "property-character"
    subject_id: str
    predicate_id: str
    object_id: str
    message: str
    suggestion: str | None = None

    def render(self) -> str:
        """Render a human-readable one-line description of the finding."""
        row = f"{self.subject_id} {self.predicate_id} {self.object_id}"
        text = f"[{self.check}] {row}: {self.message}"
        if self.suggestion is not None:
            text += f" — did you mean {self.suggestion}?"
        return text


# --------------------------------------------------------------------------- #
# Target-axiom loading
# --------------------------------------------------------------------------- #


def _load_fixture(prefix: str, fixture_dir: Path) -> Graph | None:
    path = fixture_dir / f"{prefix}.ttl"
    if not path.exists():
        return None
    return Graph().parse(path, format="turtle")


def _target_graphs(
    prefixes: set[str],
    *,
    snapshot_dir: Path,
    fixture_dir: Path,
    allow_network: bool,
) -> tuple[dict[str, Graph], set[str]]:
    """Load the axiom graph for each target prefix.

    Returns a ``(graphs, unavailable)`` pair: ``graphs`` maps each prefix with any
    axioms to its merged graph (snapshot + fixture + live), and ``unavailable``
    is the set of prefixes for which no axioms could be sourced (INFO findings).
    """
    graphs: dict[str, Graph] = {}
    unavailable: set[str] = set()
    for prefix in sorted(prefixes):
        graph = Graph()
        snapshot = load_target_snapshot(prefix, snapshot_dir=snapshot_dir)
        if snapshot is not None:
            graph += snapshot
        fixture = _load_fixture(prefix, fixture_dir)
        if fixture is not None:
            graph += fixture
        # Under --network this is a *full* live sweep: fetch and merge the live
        # axioms on top of any snapshot/fixture (so reference-only targets whose
        # only offline source is a tiny fixture get their complete vocabulary).
        if allow_network and prefix in TARGET_SOURCES:
            # Best-effort: any failure (HTTP, an HTML error page that fails to
            # parse, a bad content type) falls back to whatever we have offline.
            with contextlib.suppress(Exception):
                graph += fetch_target_axioms(prefix)
        if len(graph) == 0:
            unavailable.add(prefix)
        else:
            graphs[prefix] = graph
    return graphs, unavailable


# --------------------------------------------------------------------------- #
# Axiom accessors (normalize rdfs:/owl: vs schema.org's soft predicates)
# --------------------------------------------------------------------------- #


def _target_domain(graph: Graph, term: URIRef) -> set[URIRef]:
    return {
        o
        for p in (RDFS.domain, SCHEMA_DOMAIN_INCLUDES)
        for o in graph.objects(term, p)
        if isinstance(o, URIRef)
    }


def _target_range(graph: Graph, term: URIRef) -> set[URIRef]:
    return {
        o
        for p in (RDFS.range, SCHEMA_RANGE_INCLUDES)
        for o in graph.objects(term, p)
        if isinstance(o, URIRef)
    }


def _target_inverses(graph: Graph, term: URIRef) -> set[URIRef]:
    """Return a term's inverses, reading owl:inverseOf/schema:inverseOf both ways."""
    out: set[URIRef] = set()
    for p in (OWL.inverseOf, SCHEMA_INVERSE_OF):
        out.update(o for o in graph.objects(term, p) if isinstance(o, URIRef))
        out.update(s for s in graph.subjects(p, term) if isinstance(s, URIRef))
    return out


# --------------------------------------------------------------------------- #
# Class-equivalence bridge (gmeow:Organization ↔ schema:Organization …)
# --------------------------------------------------------------------------- #


def _build_class_bridge(
    mappings: list[Mapping],
    onto: Graph,
    target_graphs: dict[str, Graph],
) -> dict[URIRef, set[URIRef]]:
    """Build a class-compatibility closure for domain/range overlap testing.

    Three sources feed the closure, so a domain/range stated in one vocabulary's
    terms can be matched against an equivalent stated in another's:

    * the cross-vocabulary SSSOM class mappings (``owl:equivalentClass``/
      ``skos:exactMatch`` link both directions; ``rdfs:subClassOf`` links the
      subclass up to its superclass — an instance of the subclass is also an
      instance of the superclass);
    * GMEOW-internal ``rdfs:subClassOf``/``owl:equivalentClass`` axioms (e.g.
      ``gmeow:FormalOrganization rdfs:subClassOf gmeow:Organization``);
    * the same axioms inside each target snapshot (e.g.
      ``org:FormalOrganization rdfs:subClassOf org:Organization``).

    Non-``URIRef`` objects (blank nodes for OWL restrictions/unions) are dropped
    so no internal blank-node id leaks into the closure.
    """
    adjacency: dict[URIRef, set[URIRef]] = {}

    def link(a: URIRef, b: URIRef) -> None:
        adjacency.setdefault(a, set()).add(b)

    for m in mappings:
        try:
            subj, obj = expand_curie(m.subject_id), expand_curie(m.object_id)
        except Exception:  # malformed CURIE — surfaced elsewhere
            continue
        if m.predicate_id in ("owl:equivalentClass", "skos:exactMatch"):
            link(subj, obj)
            link(obj, subj)
        elif m.predicate_id == "rdfs:subClassOf":
            link(subj, obj)

    # Internal taxonomy of GMEOW and of every loaded target snapshot.
    for graph in (onto, *target_graphs.values()):
        for sub, sup in graph.subject_objects(RDFS.subClassOf):
            if isinstance(sub, URIRef) and isinstance(sup, URIRef):
                link(sub, sup)
        for a, b in graph.subject_objects(OWL.equivalentClass):
            if isinstance(a, URIRef) and isinstance(b, URIRef):
                link(a, b)
                link(b, a)

    # Transitive closure (the graph is tiny — a simple fixpoint suffices).
    closure: dict[URIRef, set[URIRef]] = {}
    for start in adjacency:
        seen: set[URIRef] = set()
        stack = [start]
        while stack:
            node = stack.pop()
            for nxt in adjacency.get(node, ()):
                if nxt not in seen:
                    seen.add(nxt)
                    stack.append(nxt)
        closure[start] = seen
    return closure


def _resolve_class(iri: URIRef, bridge: dict[URIRef, set[URIRef]]) -> set[URIRef]:
    """Return ``iri`` plus every class it is bridge-compatible with."""
    return {iri} | bridge.get(iri, set())


def _overlaps(
    gmeow_classes: set[URIRef],
    target_classes: set[URIRef],
    bridge: dict[URIRef, set[URIRef]],
) -> bool:
    """Whether any GMEOW class (bridge-expanded) meets any target class."""
    if not gmeow_classes or not target_classes:
        return False
    expanded: set[URIRef] = set()
    for cls in gmeow_classes:
        expanded |= _resolve_class(cls, bridge)
    return bool(expanded & target_classes)


# --------------------------------------------------------------------------- #
# The checks
# --------------------------------------------------------------------------- #


def _severity_for(predicate_id: str) -> Severity:
    """Map a mapping predicate to the severity of a conflict on it."""
    if predicate_id in _STRONG_PREDICATES:
        return Severity.ERROR
    return Severity.WARNING


def _check_inverse_direction(
    *,
    gmeow_props: dict[URIRef, list[Mapping]],
    onto: Graph,
    target_graphs: dict[str, Graph],
    bridge: dict[URIRef, set[URIRef]],
) -> tuple[list[AlignmentFinding], set[tuple[str, str, str]]]:
    """Self-contradiction detector + domain/range orientation fallback.

    Returns the findings and the set of ``(subject, predicate, object)`` keys it
    has already judged (so the domain/range check does not double-report them).
    """
    findings: list[AlignmentFinding] = []
    judged: set[tuple[str, str, str]] = set()

    for prop, prop_mappings in gmeow_props.items():
        if (prop, RDF.type, OWL.SymmetricProperty) in onto:
            continue  # a symmetric property may legitimately map to inverses

        # Index this property's target mappings by the resolved object IRI.
        by_iri: dict[URIRef, Mapping] = {}
        for m in prop_mappings:
            if m.predicate_id in _HIERARCHICAL_PREDICATES:
                continue
            try:
                by_iri[expand_curie(m.object_id)] = m
            except Exception:
                continue

        g_dom = {o for o in onto.objects(prop, RDFS.domain) if isinstance(o, URIRef)}
        g_rng = {o for o in onto.objects(prop, RDFS.range) if isinstance(o, URIRef)}

        seen_pairs: set[frozenset[URIRef]] = set()
        for target_iri, m in by_iri.items():
            prefix = _prefix_of(m.object_id)
            graph = target_graphs.get(prefix) if prefix else None
            if graph is None:
                continue
            inverses = _target_inverses(graph, target_iri)

            # (1) Self-contradiction: the property maps to both T and an inverse.
            for inv in inverses:
                if inv == target_iri:
                    continue  # a self-inverse (symmetric) target is not a conflict
                if inv not in by_iri:
                    continue
                pair = frozenset({target_iri, inv})
                if pair in seen_pairs:
                    continue
                seen_pairs.add(pair)
                m_inv = by_iri[inv]
                canonical, offender = _rank_pair(m, m_inv)
                key = (offender.subject_id, offender.predicate_id, offender.object_id)
                judged.add(key)
                # The contradiction is definite only when one side is a strong
                # equivalence anchoring the canonical direction; two unanchored
                # closeMatches to inverse terms is suspicious but not conclusive.
                severity = (
                    Severity.ERROR
                    if canonical.predicate_id in _STRONG_PREDICATES
                    else Severity.WARNING
                )
                findings.append(
                    AlignmentFinding(
                        severity=severity,
                        check="inverse-direction",
                        subject_id=offender.subject_id,
                        predicate_id=offender.predicate_id,
                        object_id=offender.object_id,
                        message=(
                            f"mapped to {offender.object_id}, but the property is also "
                            f"mapped to its declared inverse {canonical.object_id} "
                            f"(via {canonical.predicate_id}) — one direction is wrong"
                        ),
                        suggestion=canonical.object_id,
                    )
                )

            # (2) Orientation fallback: only the wrong term is mapped, but its
            #     inverse fits the GMEOW direction and it does not.
            key = (m.subject_id, m.predicate_id, m.object_id)
            if key in judged:
                continue
            t_dom = _target_domain(graph, target_iri)
            t_rng = _target_range(graph, target_iri)
            if not (t_dom and t_rng and g_dom and g_rng):
                continue
            direct_fit = _overlaps(g_dom, t_dom, bridge) and _overlaps(
                g_rng, t_rng, bridge
            )
            if direct_fit:
                continue
            for inv in inverses:
                if inv == target_iri:
                    continue  # self-inverse: its orientation equals the direct one
                inv_dom = _target_domain(graph, inv)
                inv_rng = _target_range(graph, inv)
                if not (inv_dom and inv_rng):
                    continue
                if _overlaps(g_dom, inv_dom, bridge) and _overlaps(
                    g_rng, inv_rng, bridge
                ):
                    judged.add(key)
                    inv_curie = _shorten(inv)
                    findings.append(
                        AlignmentFinding(
                            severity=_severity_for(m.predicate_id),
                            check="inverse-direction",
                            subject_id=m.subject_id,
                            predicate_id=m.predicate_id,
                            object_id=m.object_id,
                            message=(
                                f"{m.object_id}'s domain/range is inverted relative to "
                                f"{m.subject_id}; its inverse {inv_curie} matches the "
                                f"direction"
                            ),
                            suggestion=inv_curie,
                        )
                    )
                    break
    return findings, judged


def _check_domain_range(
    *,
    gmeow_props: dict[URIRef, list[Mapping]],
    onto: Graph,
    target_graphs: dict[str, Graph],
    bridge: dict[URIRef, set[URIRef]],
    judged: set[tuple[str, str, str]],
    unavailable: set[str],
) -> list[AlignmentFinding]:
    """Flag mappings whose GMEOW domain/range is incompatible with the target's."""
    findings: list[AlignmentFinding] = []
    for prop, prop_mappings in gmeow_props.items():
        g_dom = {o for o in onto.objects(prop, RDFS.domain) if isinstance(o, URIRef)}
        g_rng = {o for o in onto.objects(prop, RDFS.range) if isinstance(o, URIRef)}
        for m in prop_mappings:
            if m.predicate_id in _HIERARCHICAL_PREDICATES:
                continue
            key = (m.subject_id, m.predicate_id, m.object_id)
            if key in judged:
                continue
            prefix = _prefix_of(m.object_id)
            if prefix is None:
                continue
            graph = target_graphs.get(prefix)
            if graph is None:
                if prefix in unavailable:
                    findings.append(_info_unavailable(m, prefix, "domain-range"))
                continue
            target_iri = expand_curie(m.object_id)
            t_dom = _target_domain(graph, target_iri)
            t_rng = _target_range(graph, target_iri)
            if not (t_dom and t_rng):
                # The target axioms are present but this term declares no
                # domain/range — honestly record that the row was not checked
                # rather than silently passing it (issue #25 "warn, don't skip").
                findings.append(
                    _info_not_checkable(
                        m, "target term declares no domain/range to check against"
                    )
                )
                continue
            if not (g_dom and g_rng):
                findings.append(
                    _info_not_checkable(
                        m, "GMEOW term declares no domain/range to check against"
                    )
                )
                continue
            if _overlaps(g_dom, t_dom, bridge) and _overlaps(g_rng, t_rng, bridge):
                continue  # direct orientation agrees
            swapped = _overlaps(g_dom, t_rng, bridge) and _overlaps(
                g_rng, t_dom, bridge
            )
            if not swapped:
                # No overlap in EITHER orientation almost always means there is no
                # SSSOM class bridge between the domain/range classes — ambiguous,
                # not a proven mismatch. Report as INFO so the gate stays
                # false-positive-free (issue #25's "warn, don't hard-fail" rule).
                findings.append(
                    AlignmentFinding(
                        severity=Severity.INFO,
                        check="domain-range",
                        subject_id=m.subject_id,
                        predicate_id=m.predicate_id,
                        object_id=m.object_id,
                        message=(
                            "domain/range overlap could not be established "
                            "(no class bridge to the target's domain/range)"
                        ),
                    )
                )
                continue
            # Swapped overlap is positive evidence of an inverted mapping.
            findings.append(
                AlignmentFinding(
                    severity=_severity_for(m.predicate_id),
                    check="domain-range",
                    subject_id=m.subject_id,
                    predicate_id=m.predicate_id,
                    object_id=m.object_id,
                    message="domain/range are inverted relative to the target term",
                )
            )
    return findings


def _check_property_character(
    *,
    gmeow_props: dict[URIRef, list[Mapping]],
    onto: Graph,
    target_graphs: dict[str, Graph],
) -> list[AlignmentFinding]:
    """Flag equivalentProperty mappings with mismatched property character."""
    findings: list[AlignmentFinding] = []
    for prop, prop_mappings in gmeow_props.items():
        g_is_object = (prop, RDF.type, OWL.ObjectProperty) in onto
        g_is_data = (prop, RDF.type, OWL.DatatypeProperty) in onto
        g_chars = {c for c in _CHARACTER_TYPES if (prop, RDF.type, c) in onto}
        for m in prop_mappings:
            if m.predicate_id not in _STRONG_PREDICATES:
                continue  # character must agree only for asserted equivalence
            prefix = _prefix_of(m.object_id)
            graph = target_graphs.get(prefix) if prefix else None
            if graph is None:
                continue
            term = expand_curie(m.object_id)
            t_types = set(graph.objects(term, RDF.type))
            if not t_types:
                continue  # target character unknown → skip

            # Object-vs-datatype kind conflict is a hard semantic error.
            if g_is_object and OWL.DatatypeProperty in t_types:
                findings.append(
                    _character_finding(
                        m,
                        Severity.ERROR,
                        "GMEOW object property vs target datatype property",
                    )
                )
            elif g_is_data and OWL.ObjectProperty in t_types:
                findings.append(
                    _character_finding(
                        m,
                        Severity.ERROR,
                        "GMEOW datatype property vs target object property",
                    )
                )

            # Functional/transitive/symmetric/IFP disagreement → warning, but only
            # when the target speaks the OWL characteristic vocabulary at all.
            # schema.org types everything as schema:Property and never declares
            # OWL characteristics, so comparing would warn on every mapping —
            # pure noise, not signal.
            if not (t_types & _OWL_PROPERTY_TYPES):
                continue
            for char in g_chars:
                if char not in t_types:
                    label = _shorten(char).split(":")[-1]
                    findings.append(
                        _character_finding(
                            m,
                            Severity.WARNING,
                            f"GMEOW declares {label} but the target does not",
                        )
                    )
    return findings


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _prefix_of(curie: str) -> str | None:
    """Return the CURIE prefix if it is a known alignment target, else None."""
    if ":" not in curie:
        return None
    prefix = curie.split(":", 1)[0]
    return prefix if prefix in ALIGNMENT_TARGETS else None


def _shorten(iri: URIRef) -> str:
    """Render an IRI as a CURIE using the canonical prefix registry."""
    text = str(iri)
    for prefix, namespace in PREFIXES.items():
        if text.startswith(namespace):
            return f"{prefix}:{text[len(namespace) :]}"
    return text


def _rank_pair(a: Mapping, b: Mapping) -> tuple[Mapping, Mapping]:
    """Return ``(canonical, offender)`` for a self-contradicting mapping pair."""

    def score(m: Mapping) -> tuple[int, float]:
        try:
            conf = float(m.confidence) if m.confidence else 0.0
        except ValueError:
            conf = 0.0
        return (_PREDICATE_RANK.get(m.predicate_id, 0), conf)

    return (a, b) if score(a) >= score(b) else (b, a)


def _info_unavailable(m: Mapping, prefix: str, check: str) -> AlignmentFinding:
    return AlignmentFinding(
        severity=Severity.INFO,
        check=check,
        subject_id=m.subject_id,
        predicate_id=m.predicate_id,
        object_id=m.object_id,
        message=(
            f"skipped — no axioms available for target {prefix!r} "
            f"(vendor a snapshot or run with --network)"
        ),
    )


def _info_not_checkable(m: Mapping, reason: str) -> AlignmentFinding:
    return AlignmentFinding(
        severity=Severity.INFO,
        check="domain-range",
        subject_id=m.subject_id,
        predicate_id=m.predicate_id,
        object_id=m.object_id,
        message=f"direction not checked — {reason}",
    )


def _character_finding(
    m: Mapping, severity: Severity, message: str
) -> AlignmentFinding:
    return AlignmentFinding(
        severity=severity,
        check="property-character",
        subject_id=m.subject_id,
        predicate_id=m.predicate_id,
        object_id=m.object_id,
        message=message,
    )


# --------------------------------------------------------------------------- #
# Entry points
# --------------------------------------------------------------------------- #


def lint_alignment_directions(
    *,
    mappings_dir: Path = MAPPINGS_DIR,
    snapshot_dir: Path = TARGET_SNAPSHOT_DIR,
    fixture_dir: Path = TARGET_FIXTURE_DIR,
    allow_network: bool = False,
) -> list[AlignmentFinding]:
    """Lint SSSOM property mappings for inverse / mismatched target terms.

    Args:
        mappings_dir: Directory of ``*.sssom.tsv`` mapping tables.
        snapshot_dir: Vendored target axiom snapshots (``imports/targets/``).
        fixture_dir: Hand-authored axioms for reference-only targets.
        allow_network: When true, fetch missing target axioms live (the
            ``network``-marked path). When false (default), missing axioms yield
            non-fatal ``INFO`` findings.

    Returns:
        All findings, in a stable order (errors first, then warnings, then info).
    """
    onto = load_merged_graph(include_imports=False)
    mappings = load_mappings(mappings_dir)

    # Group the property mappings (subject is a GMEOW property, object is a known
    # alignment target) by the GMEOW property they align.
    gmeow_props: dict[URIRef, list[Mapping]] = {}
    referenced_prefixes: set[str] = set()
    for m in mappings:
        if not m.subject_id.startswith("gmeow:"):
            continue
        prefix = _prefix_of(m.object_id)
        if prefix is None:
            continue
        subj = expand_curie(m.subject_id)
        is_property = (subj, RDF.type, OWL.ObjectProperty) in onto or (
            subj,
            RDF.type,
            OWL.DatatypeProperty,
        ) in onto
        if not is_property:
            continue
        gmeow_props.setdefault(subj, []).append(m)
        referenced_prefixes.add(prefix)

    target_graphs, unavailable = _target_graphs(
        referenced_prefixes,
        snapshot_dir=snapshot_dir,
        fixture_dir=fixture_dir,
        allow_network=allow_network,
    )
    # Built after the target graphs so the bridge can ingest their internal
    # taxonomies (and GMEOW's) alongside the cross-vocabulary SSSOM mappings.
    bridge = _build_class_bridge(mappings, onto, target_graphs)

    inverse_findings, judged = _check_inverse_direction(
        gmeow_props=gmeow_props,
        onto=onto,
        target_graphs=target_graphs,
        bridge=bridge,
    )
    domain_findings = _check_domain_range(
        gmeow_props=gmeow_props,
        onto=onto,
        target_graphs=target_graphs,
        bridge=bridge,
        judged=judged,
        unavailable=unavailable,
    )
    character_findings = _check_property_character(
        gmeow_props=gmeow_props,
        onto=onto,
        target_graphs=target_graphs,
    )

    findings = inverse_findings + domain_findings + character_findings
    order = {Severity.ERROR: 0, Severity.WARNING: 1, Severity.INFO: 2}
    findings.sort(key=lambda f: (order[f.severity], f.check, f.subject_id, f.object_id))
    return findings


def findings_to_result(findings: list[AlignmentFinding]) -> ValidationResult:
    """Collapse findings into a :class:`ValidationResult` (INFO dropped)."""
    result = ValidationResult()
    for finding in findings:
        if finding.severity is Severity.ERROR:
            result.errors.append(finding.render())
        elif finding.severity is Severity.WARNING:
            result.warnings.append(finding.render())
    return result
