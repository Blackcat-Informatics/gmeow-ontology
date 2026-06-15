# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Real-data acceptance harness for the full transpile (#450).

The honest scoreboard. A prior coverage metric was *gamed*: self-authored
fixtures, written to make the projection emit a target vocabulary, then measured
to confirm that vocabulary was emitted — circular. This harness is the antidote.
It runs the whole ``source RDF → pure-GMEOW draft → MAXIMAL`` pipeline over
**verbatim real-world graphs** (the `external/` snapshots — explicitly NOT
GMEOW-authored, so the numbers cannot be moved by writing more fixtures) and
scores five gates:

1. **Pure-GMEOW intermediate** — the up-projected draft carries *only* GMEOW
   (+ RDF/RDFS/OWL structural) terms. Any consumer-vocab residue is a reported
   coverage gap, never a pass. *Hard.*
2. **Round-trip ⊇ source, per vocabulary** — ``down(up(source))`` restricted to
   each vocabulary must contain the source's triples in that vocabulary (modulo
   language-tag + blank-node normalization). A miss is a GMEOW coverage gap or a
   non-inverse cell — both bugs. *Scoreboard: red until done.*
3. **Size invariant** — the maximal output strictly out-sizes the source on
   covered content (the representational fan-out fired). *Hard.*
4. **External-validator pass** — the consumer serialization carries **no**
   ``x-gmeow-*`` language tag (*hard* — a parser reads it as nothing). Two further
   checks run *report-only*, a decision the **real data** made (#450, "how can we
   know until we run it"): the vendored definitions are *minimal axiom graphs*, so
   neither term-attestation (a term we emit that no vendored axiom mentions) nor
   the open-world range SHACL **generated from the vocabularies' own axioms** can
   be a hard gate without false-failing legitimate real-world RDF — both are
   surfaced for inspection instead.
5. **Honest coverage** — per source, triples lifted to GMEOW vs the gap, and the
   maximal output's vocabulary coverage against the source as a parity target.

The harness is a **progress meter, red until done** — it does not block CI, it
scores. The validator gate uses real upstream artifacts (the vendored vocabulary
definitions in :data:`~gmeow_tools.config.TARGET_SNAPSHOT_DIR`), never a
GMEOW-internal re-implementation that could rubber-stamp itself.
"""

from __future__ import annotations

import tempfile
from dataclasses import dataclass, field
from functools import cache
from pathlib import Path
from typing import TYPE_CHECKING

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef

from gmeow_tools.config import (
    EXTERNAL_FIXTURES_DIR,
    NAMESPACE,
    PREFIXES,
)
from gmeow_tools.language_tags import retag_literal

if TYPE_CHECKING:
    from collections.abc import Iterable

    from rdflib.term import Node

#: Namespaces that may appear in the pure-GMEOW draft besides GMEOW itself: the
#: structural RDF/RDFS/OWL terms the claim reification and typing use.
_STRUCTURAL_NS: tuple[str, ...] = (
    NAMESPACE,
    str(RDF),
    str(RDFS),
    str(OWL),
)

#: The consumer vocabularies whose own definition is vendored under
#: :data:`TARGET_SNAPSHOT_DIR` (the genuinely-external validator source). The
#: prefix is both the snapshot stem and the key into :data:`PREFIXES`.
_VENDORED_DEFS: tuple[str, ...] = (
    "foaf",
    "vcard",
    "org",
    "prov",
    "time",
    "geo",
    "ontolex",
)


@dataclass(slots=True)
class GateResult:
    """One gate's verdict over one source file."""

    name: str
    passed: bool
    hard: bool  # a hard gate fails the file; a scoreboard gate only reports
    summary: str
    metrics: dict[str, float] = field(default_factory=dict)
    detail: list[str] = field(default_factory=list)


@dataclass(slots=True)
class FileAcceptance:
    """Every gate's result over one source file."""

    source: str
    source_triples: int
    output_triples: int
    gates: list[GateResult]

    @property
    def passed(self) -> bool:
        """True iff every HARD gate passed (scoreboard gates never block)."""
        return all(g.passed for g in self.gates if g.hard)


# --------------------------------------------------------------------------- #
# vocabulary bucketing
# --------------------------------------------------------------------------- #

_SORTED_PREFIXES: list[tuple[str, str]] = sorted(
    PREFIXES.items(), key=lambda kv: -len(kv[1])
)


def _vocab_of(term: Node) -> str | None:
    """The registered prefix whose namespace *term* falls under, or ``None``."""
    if not isinstance(term, URIRef):
        return None
    text = str(term)
    for prefix, ns in _SORTED_PREFIXES:
        if text.startswith(ns):
            return prefix
    return None


def _triple_vocab(s: Node, p: Node, o: Node) -> str | None:
    """The vocabulary a triple belongs to.

    Its ``rdf:type`` object's vocab for a type assertion, else its predicate's.
    """
    if p == RDF.type and isinstance(o, URIRef):
        return _vocab_of(o)
    return _vocab_of(p)


def _normalize(s: Node, p: Node, o: Node) -> tuple[Node, Node, Node]:
    """Normalize a triple for round-trip comparison.

    An internal ``@x-gmeow-*`` tag is folded to its public BCP-47 form — the
    up/down retag is a known, lossless re-tagging — so ``@x-gmeow-english`` and
    ``@en`` compare equal. Distinct *languages* stay distinct: ``@en`` and ``@fr``
    must NOT collapse, or a mistranslated / wrong-tagged round trip would be
    silently scored as recovered.
    """
    if isinstance(o, Literal) and o.language:
        o = retag_literal(o)
    return (s, p, o)


def _by_vocab(
    graph: Iterable[tuple[Node, Node, Node]], *, iri_subjects_only: bool
) -> dict[str, set[tuple[Node, Node, Node]]]:
    """Bucket normalized triples by vocabulary.

    Blank-node subjects skolemize unstably across the round-trip, so the
    round-trip gate skips them.
    """
    buckets: dict[str, set[tuple[Node, Node, Node]]] = {}
    for s, p, o in graph:
        if iri_subjects_only and not isinstance(s, URIRef):
            continue
        vocab = _triple_vocab(s, p, o)
        if vocab is None:
            continue
        buckets.setdefault(vocab, set()).add(_normalize(s, p, o))
    return buckets


# --------------------------------------------------------------------------- #
# the gates
# --------------------------------------------------------------------------- #


def _gate_pure_gmeow(draft: Graph) -> GateResult:
    """Gate 1: the up-projected draft contains only GMEOW + structural terms."""
    foreign: dict[str, int] = {}
    for _s, p, o in draft:
        if p != RDF.type and not str(p).startswith(_STRUCTURAL_NS):
            key = _vocab_of(p) or str(p)
            foreign[key] = foreign.get(key, 0) + 1
        if (
            p == RDF.type
            and isinstance(o, URIRef)
            and not str(o).startswith(_STRUCTURAL_NS)
        ):
            key = "a " + (_vocab_of(o) or str(o))
            foreign[key] = foreign.get(key, 0) + 1
    passed = not foreign
    summary = (
        "draft is pure GMEOW"
        if passed
        else f"{sum(foreign.values())} consumer-vocab residue triples"
    )
    return GateResult(
        name="pure-gmeow-intermediate",
        passed=passed,
        hard=True,
        summary=summary,
        metrics={"residue": float(sum(foreign.values()))},
        detail=[f"{v}: {n}" for v, n in sorted(foreign.items(), key=lambda kv: -kv[1])],
    )


#: Vocabularies a real-world graph carries that GMEOW deliberately does NOT model
#: — the ``external/`` snapshot doctrine: ``owl:sameAs`` links to outside entities
#: (geni, Wikidata, DBpedia) are "whatever the outside world emits," not a GMEOW
#: coverage gap. Reported separately so the headline recall is not charged for
#: content GMEOW was never meant to round-trip (issue point 6: state it precisely).
_EXTERNAL_LINKAGE_VOCABS: frozenset[str] = frozenset({"owl"})


def _gate_round_trip(source: Graph, output: Graph) -> GateResult:
    """Gate 2: per vocabulary, ``output ⊇ source`` (the scoreboard).

    Headline recall is over GMEOW-addressable vocabularies; documented external
    linkage (:data:`_EXTERNAL_LINKAGE_VOCABS`) is recovered-or-not on its own line
    but kept out of the headline — neither hidden nor charged against coverage.
    """
    src_v = _by_vocab(source, iri_subjects_only=True)
    out_v = _by_vocab(output, iri_subjects_only=True)
    rows: list[str] = []
    linkage_rows: list[str] = []
    total_src = total_recovered = 0
    for vocab in sorted(src_v):
        want = src_v[vocab]
        recovered = want & out_v.get(vocab, set())
        pct = 100.0 * len(recovered) / len(want) if want else 100.0
        row = f"{vocab}: {len(recovered)}/{len(want)} ({pct:.0f}%)"
        if vocab in _EXTERNAL_LINKAGE_VOCABS:
            linkage_rows.append(row + " [external linkage — not modeled by design]")
            continue
        total_src += len(want)
        total_recovered += len(recovered)
        rows.append(row)
    overall = 100.0 * total_recovered / total_src if total_src else 100.0
    detail = list(rows)
    if linkage_rows:
        detail += ["", "external linkage (excluded from headline):", *linkage_rows]
    return GateResult(
        name="round-trip-superset",
        passed=total_recovered == total_src,
        hard=False,  # scoreboard: red until coverage closes
        summary=(
            f"{total_recovered}/{total_src} addressable source triples "
            f"recovered ({overall:.0f}%)"
        ),
        metrics={"recall_pct": overall, "recovered": float(total_recovered)},
        detail=detail,
    )


def _gate_size_invariant(source: Graph, output: Graph) -> GateResult:
    """Gate 3: the maximal output strictly out-sizes the source (fan-out fired)."""
    passed = len(output) > len(source)
    return GateResult(
        name="size-invariant",
        passed=passed,
        hard=True,
        summary=f"output {len(output)} {'>' if passed else '≤'} source {len(source)}",
        metrics={"ratio": len(output) / len(source) if len(source) else 0.0},
    )


def _emitted_terms_by_vocab(output: Graph) -> dict[str, set[URIRef]]:
    """Every predicate + ``rdf:type`` object emitted, bucketed by vocabulary."""
    terms: dict[str, set[URIRef]] = {}
    for _s, p, o in output:
        for term in (p, o if p == RDF.type else None):
            if isinstance(term, URIRef) and term != RDF.type:
                vocab = _vocab_of(term)
                if vocab is not None:
                    terms.setdefault(vocab, set()).add(term)
    return terms


@cache
def _known_terms(prefix: str) -> set[URIRef] | None:
    """Every IRI attested anywhere in a vocabulary's vendored definition.

    Subject or object; ``None`` when no definition is vendored. The vendored
    snapshots are *minimal axiom graphs* (domain/range/character
    only — see :mod:`gmeow_tools.target_axioms`), not complete term catalogs: a
    class like ``foaf:Person`` shows up as a property's range, never as a subject.
    So membership is "mentioned in an axiom," and a term absent from every axiom
    is *reported* (not failed) — the snapshot cannot prove a term is fabricated,
    only that it is unattested by the vendored axioms.
    """
    from gmeow_tools.target_axioms import load_target_snapshot

    snapshot = load_target_snapshot(prefix)
    if snapshot is None:
        return None
    known: set[URIRef] = set()
    for s, _p, o in snapshot:
        if isinstance(s, URIRef):
            known.add(s)
        if isinstance(o, URIRef):
            known.add(o)
    return known


@cache
def _generate_range_shapes(prefix: str) -> Graph | None:
    """SHACL node-shapes generated from a vocabulary's own ``rdfs:range`` axioms.

    Every object of property ``?p`` must be of the declared range class. The
    constraints come from the vocabulary authors, vendored — not from us.
    """
    from gmeow_tools.target_axioms import load_target_snapshot

    snapshot = load_target_snapshot(prefix)
    if snapshot is None:
        return None
    sh = Namespace("http://www.w3.org/ns/shacl#")
    shapes = Graph()
    ns = PREFIXES[prefix]
    for prop, _r, rng in snapshot.triples((None, RDFS.range, None)):
        # only class ranges in the vocabulary's own namespace; skip datatypes and
        # cross-vocabulary ranges (rdfs:Literal, xsd:*, foreign classes).
        if not isinstance(rng, URIRef) or not str(rng).startswith(ns):
            continue
        shape = URIRef(str(prop) + "-rangeShape")
        shapes.add((shape, RDF.type, sh.NodeShape))
        shapes.add((shape, sh.targetObjectsOf, prop))
        shapes.add((shape, sh["class"], rng))
    return shapes if len(shapes) else None


def _gate_external_validator(output: Graph) -> GateResult:
    """Gate 4: no x-gmeow tags (HARD) + unattested terms + range SHACL (report-only).

    The data decided the strictness (#450, "how can we know until we run it"):
    the x-gmeow-tag ban is the one unambiguous hard line a consumer parser
    enforces. The vendored definitions are *minimal axiom graphs*, so neither the
    term-attestation check nor the open-world range SHACL can be a hard gate
    without false-failing legitimate real-world RDF — they are surfaced for
    inspection instead.
    """
    detail: list[str] = []

    # HARD: no internal x-gmeow-* tag may leak into the consumer serialization —
    # a parser reads `@x-gmeow-english` as nothing (issue point 7, a hard line).
    leaked = sum(
        1
        for _s, _p, o in output
        if isinstance(o, Literal) and (o.language or "").startswith("x-gmeow")
    )
    detail.append(
        f"x-gmeow tag leak: {leaked} literals" + (" (HARD FAIL)" if leaked else " ✓")
    )

    # REPORT-ONLY: terms we emit in a vendored vocabulary's namespace that are
    # unattested by its vendored axioms (a minimal graph — absence is a flag to
    # inspect, never proof of fabrication).
    emitted = _emitted_terms_by_vocab(output)
    unattested = 0
    for prefix in _VENDORED_DEFS:
        known = _known_terms(prefix)
        if known is None:
            continue
        missing = sorted(str(t) for t in emitted.get(prefix, set()) - known)
        if missing:
            unattested += len(missing)
            names = ", ".join(t.rsplit("/", 1)[-1].rsplit("#", 1)[-1] for t in missing)
            detail.append(
                f"{prefix}: {len(missing)} unattested term(s): {names} [report]"
            )

    # REPORT-ONLY: SHACL generated from the vocabularies' own range axioms.
    shacl_violations = _run_range_shacl(output, detail)

    return GateResult(
        name="external-validator",
        passed=leaked == 0,
        hard=True,
        summary=(
            f"x-gmeow leak={leaked} (hard); unattested terms={unattested}, "
            f"range-SHACL violations={shacl_violations} (report-only)"
        ),
        metrics={
            "x_gmeow_leak": float(leaked),
            "unattested_terms": float(unattested),
            "shacl_violations": float(shacl_violations),
        },
        detail=detail,
    )


def _run_range_shacl(output: Graph, detail: list[str]) -> int:
    """Run the generated range-SHACL over the consumer output.

    Returns the total violations (report-only), per-vocab so a noisy vocabulary
    is legible.
    """
    from pyshacl import validate

    total = 0
    for prefix in _VENDORED_DEFS:
        shapes = _generate_range_shapes(prefix)
        if shapes is None:
            continue
        conforms, results_graph, _text = validate(
            output, shacl_graph=shapes, inference="none", advanced=False
        )
        if conforms:
            continue
        sh_result = URIRef("http://www.w3.org/ns/shacl#ValidationResult")
        n = sum(1 for _ in results_graph.subjects(RDF.type, sh_result))
        total += n
        detail.append(f"{prefix}: {n} range-SHACL violation(s) [report-only]")
    return total


def _gate_coverage(
    source: Graph, output: Graph, lifted: int, gap_terms: int
) -> GateResult:
    """Gate 5: honest lifted-vs-gap + vocabulary coverage against the source."""
    from gmeow_tools.transform import vocab_coverage

    table = vocab_coverage(output, source) if len(output) and len(source) else ""
    return GateResult(
        name="honest-coverage",
        passed=True,  # a report, never a pass/fail
        hard=False,
        summary=f"{lifted} triples lifted to GMEOW, {gap_terms} gap term(s)",
        metrics={"lifted": float(lifted), "gap_terms": float(gap_terms)},
        detail=table.splitlines(),
    )


# --------------------------------------------------------------------------- #
# the harness
# --------------------------------------------------------------------------- #


def run_acceptance(source_path: Path, *, descend: bool = True) -> FileAcceptance:
    """Run every acceptance gate over one source file.

    Args:
        source_path: A non-GMEOW source RDF file (a verbatim real-world snapshot).
        descend: Use the context-aware graph-descent up-projection (default).

    Returns:
        The :class:`FileAcceptance` — every gate's verdict over this source.
    """
    from gmeow_tools.transpile import transpile_graph

    source = Graph()
    source.parse(source_path, format="turtle")

    with tempfile.TemporaryDirectory() as tmp:
        out_dir = Path(tmp)
        report = transpile_graph(
            source, source_path.stem, out_dir=out_dir, descend=descend
        )
        draft = Graph()
        draft.parse(report.draft_path, format="turtle")
        # index.ttl is the consumer tier: asserted base triples, public BCP-47.
        output = Graph()
        output.parse(out_dir / "index.ttl", format="turtle")

    gates = [
        _gate_pure_gmeow(draft),
        _gate_round_trip(source, output),
        _gate_size_invariant(source, output),
        _gate_external_validator(output),
        _gate_coverage(source, output, report.lifted, report.gap_terms),
    ]
    return FileAcceptance(
        source=source_path.name,
        source_triples=len(source),
        output_triples=len(output),
        gates=gates,
    )


def default_corpus() -> list[Path]:
    """The vendored real-world snapshots (the un-gameable parity targets)."""
    return sorted(EXTERNAL_FIXTURES_DIR.glob("*.ttl"))


def render_report(results: list[FileAcceptance]) -> str:
    """Render the human + machine-legible acceptance scoreboard as Markdown."""
    lines = ["# Transpile acceptance — real-data scoreboard\n"]
    for fa in results:
        verdict = "✅ PASS" if fa.passed else "❌ FAIL"
        lines.append(f"## {fa.source} — {verdict}\n")
        lines.append(
            f"source {fa.source_triples} triples → consumer output "
            f"{fa.output_triples} triples\n"
        )
        lines.append("| gate | kind | verdict | summary |")
        lines.append("|---|---|---|---|")
        for g in fa.gates:
            kind = "hard" if g.hard else "scoreboard"
            v = "✅" if g.passed else ("❌" if g.hard else "🔴")
            lines.append(f"| {g.name} | {kind} | {v} | {g.summary} |")
        lines.append("")
        for g in fa.gates:
            if g.detail:
                lines.append(f"<details><summary>{g.name}</summary>\n")
                lines.extend(g.detail)
                lines.append("\n</details>\n")
    return "\n".join(lines) + "\n"
