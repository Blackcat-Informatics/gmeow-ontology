# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Enforced native↔oracle divergence cross-check (issue #666, Task 4).

This is the FINAL, ENFORCING step of the ``classic-cross-check`` lane — the sole
Docker/Java surface (Principle 18). It is **never** part of ``make check`` or the
required ``quality`` gate, and never required to use the repo normally.

What it does
------------
1. Reasons the committed GTS bundle natively (Rust, Java/Docker-free) — the
   authority. From that run it takes the subsumption closure
   (``rdfs:subClassOf``), the DL consistency verdict + unsatisfiable classes, and
   the beyond-EL DL *gaps* (the honest limits of the ternary encoding).
2. Runs the classic Docker oracles, **timing each**:
   * ELK materializes the inferred class hierarchy; its ``rdfs:subClassOf``
     closure is the subsumption oracle.
   * HermiT decides overall consistency and the unsatisfiable classes (the DL
     oracle).
3. Calls the AUTHORITATIVE Rust comparator
   (``gmeow_logic.build_divergence_ledger``) — Rust owns the classification, this
   module owns only the Docker orchestration, the timing, the SARIF emission, and
   the enforcement decision.
4. Emits the agreement matrix + per-tool timing as SARIF + JSON via the
   ``gmeow_diagnostics`` rail (the gate taxonomy #666 owns / #662 consumes).
5. ENFORCES strict-by-default (no knob): exits NON-ZERO if ANY ``NativeOnly`` or
   ``OracleOnly`` row exists. ``DlGap`` is the ONLY honest-expected, non-failing
   class.

World alignment
---------------
The native engine reasons world-scoped (named graphs = worlds), so its
subsumptions carry a world IRI. The classic ELK oracle reasons the single merged
ontology and has no notion of worlds. Subsumption is world-independent TBox, so
the comparison collapses both sides to a single canonical world sentinel
(:data:`CROSSCHECK_WORLD`) — comparing on ``(subject, object)`` honestly, never
inventing per-world ELK results.
"""

from __future__ import annotations

import time
from collections.abc import Iterable
from pathlib import Path
from typing import TYPE_CHECKING, Any

import gmeow_rdf

from gmeow_tools.config import DIST_DIR, GTS_SNAPSHOT_FILE

if TYPE_CHECKING:
    from gmeow_tools.diagnostics import DiagnosticsReport

#: The canonical world sentinel both sides collapse to for the (world-independent)
#: subsumption comparison. ELK has no world notion; subsumption is TBox.
CROSSCHECK_WORLD = "https://blackcatinformatics.ca/gmeow/graph/crosscheck"

#: ``rdfs:subClassOf`` — the subsumption predicate the cross-check compares.
RDFS_SUBCLASSOF = "http://www.w3.org/2000/01/rdf-schema#subClassOf"
#: ``owl:Nothing`` — the unsatisfiable-class marker.
OWL_NOTHING = "http://www.w3.org/2002/07/owl#Nothing"

#: Diagnostics-artifact stem for the enforced cross-check (JSON / SARIF / HTML).
CROSSCHECK_STEM = "gmeow-classic-cross-check"

# Gate taxonomy rule-ids (#666 owns these; #662 consumes them).
RULE_SUBSUMPTION_DIVERGENCE = "classic-cross-check/subsumption-divergence"
RULE_CONSISTENCY_DIVERGENCE = "classic-cross-check/consistency-divergence"
RULE_DL_GAP = "classic-cross-check/dl-gap"
RULE_AGREEMENT = "classic-cross-check/agreement"
RULE_TIMING = "classic-cross-check/timing"


def _bare(iri: str) -> str:
    """Strip one surrounding pair of angle brackets (native display form)."""
    if iri.startswith("<") and iri.endswith(">"):
        return iri[1:-1]
    return iri


def native_subsumptions(result: dict[str, Any]) -> list[tuple[str, str, str]]:
    """Collapse the native ``rdfs:subClassOf`` closure to the cross-check world.

    Includes BOTH asserted (told) and derived subsumptions so the comparison is
    against ELK's full inferred hierarchy (told + inferred), on equal footing.
    Self-subsumptions (``X ⊑ X``) are dropped — ELK's ``rdfs:subClassOf``
    serialization may or may not include reflexive edges, so they are not a fair
    point of comparison.
    """
    rows: set[tuple[str, str, str]] = set()
    for axiom in result.get("inferred", []):
        if _bare(axiom["predicate"]) != RDFS_SUBCLASSOF:
            continue
        subject = _bare(axiom["subject"])
        obj = _bare(axiom["object"])
        if subject == obj:
            continue
        rows.add((subject, obj, CROSSCHECK_WORLD))
    return sorted(rows)


def native_unsat_classes(result: dict[str, Any]) -> list[str]:
    """Extract the native unsatisfiable-class IRIs (bare)."""
    return sorted(_bare(u["class"]) for u in result.get("unsatisfiable_classes", []))


def native_gaps(result: dict[str, Any]) -> list[tuple[str, str]]:
    """Extract the native beyond-EL DL gaps as ``(code, message)`` tuples."""
    return [(g["code"], g["message"]) for g in result.get("gaps", [])]


def reason_native(gts: Path = GTS_SNAPSHOT_FILE) -> dict[str, Any]:
    """Run the native (authority) reasoner over the GTS bundle."""
    import gmeow_logic

    return gmeow_logic.reason_native(gts.read_bytes())


# --------------------------------------------------------------------------- #
# ELK subsumption extraction (Docker oracle)
# --------------------------------------------------------------------------- #


#: Where the native told-facts ontology is staged for the Docker oracles. It is a
#: LANE scratch file (under dist/), NOT a committed artifact.
TOLD_FACTS_FILE = DIST_DIR / "gmeow-crosscheck-told.ttl"
#: The ontology IRI stamped on the told-facts product so ROBOT treats it as one
#: self-contained ontology (no import closure to resolve).
CROSSCHECK_ONTOLOGY_IRI = "https://blackcatinformatics.ca/gmeow/crosscheck"


def _term_nt(value: str) -> str:
    """Render a native term string as an N-Triples term.

    Native IRI objects arrive angle-bracketed (``<iri>``); subjects/predicates
    arrive bare. Literals arrive already quoted (``"..."`` / ``"..."@en`` /
    ``"..."^^<dt>``). Bare IRIs are wrapped in angle brackets.
    """
    value = value.strip()
    if value.startswith("<") and value.endswith(">"):
        return value
    if value.startswith('"'):
        return value
    return f"<{value}>"


def write_told_facts(native: dict[str, Any], *, path: Path = TOLD_FACTS_FILE) -> Path:
    """Serialize the native TOLD (asserted) axioms to a single OWL/Turtle ontology.

    This is the corpus the native engine actually reasons (every ``is_edb`` axiom
    of the GTS bundle's reasoning graphs, flattened — worlds collapsed). Feeding
    THIS to the Docker oracles makes the cross-check apples-to-apples: native and
    ELK/HermiT close the SAME told TBox, so any divergence is a genuine
    reasoner disagreement, not an input mismatch. (The previous, separately-built
    ``gmeow-merged.ttl`` product is a DIFFERENT corpus and would compare two
    different ontologies, not two reasoners.)
    """
    lines: list[str] = [
        f"<{CROSSCHECK_ONTOLOGY_IRI}> "
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> "
        "<http://www.w3.org/2002/07/owl#Ontology> .",
    ]
    for axiom in native.get("inferred", []):
        if not axiom["is_edb"]:
            continue
        subject = _term_nt(axiom["subject"])
        predicate = _bare(axiom["predicate"])
        obj = _term_nt(axiom["object"])
        lines.append(f"{subject} <{predicate}> {obj} .")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


#: Subsumption / equivalence predicate IRIs the cross-check reads.
_OWL_EQUIVALENT_CLASS = "http://www.w3.org/2002/07/owl#equivalentClass"


def _subsumption_edges(store: gmeow_rdf.Store) -> set[tuple[str, str]]:
    """Extract named subsumption edges from a store.

    Pulls both ``rdfs:subClassOf`` and ``owl:equivalentClass``. The native engine
    expands an equivalence ``A ≡ B`` into the bidirectional subsumption pair
    (``A ⊑ B``, ``B ⊑ A``) and materializes the closure; ROBOT/ELK collapses an
    equivalence clique to a single representative and serializes only one
    ``owl:equivalentClass`` triple. Treating equivalence as bidirectional
    subsumption recovers the same EL semantics on both sides (honest, not a fudge:
    every edge here is genuinely EL-entailed by the asserted axioms).
    """
    query = """
        SELECT ?s ?p ?o WHERE {
          VALUES ?p {
            <http://www.w3.org/2000/01/rdf-schema#subClassOf>
            <http://www.w3.org/2002/07/owl#equivalentClass>
          }
          ?s ?p ?o .
          FILTER(isIRI(?s) && isIRI(?o))
          FILTER(?s != ?o)
          FILTER(?o != <http://www.w3.org/2002/07/owl#Thing>)
        }
    """
    edges: set[tuple[str, str]] = set()
    results = store.query(query)
    assert isinstance(results, gmeow_rdf.QuerySolutions)
    for solution in results:
        subject = solution["s"]
        predicate = solution["p"]
        obj = solution["o"]
        assert isinstance(subject, gmeow_rdf.NamedNode)
        assert isinstance(predicate, gmeow_rdf.NamedNode)
        assert isinstance(obj, gmeow_rdf.NamedNode)
        edges.add((subject.value, obj.value))
        if predicate.value == _OWL_EQUIVALENT_CLASS:
            edges.add((obj.value, subject.value))  # equivalence is bidirectional
    return edges


def _subclassof_closure(*turtle_paths: Path) -> set[tuple[str, str, str]]:
    """Load the given Turtle files into ONE store and return the closed subsumptions.

    Uses gmeow_rdf (no rdflib). Both the ELK-reasoned output AND the told-facts
    input are loaded so the closure basis includes the asserted equivalences that
    ROBOT collapses away in its reasoned serialization. The native engine emits
    the FULL transitive subsumption closure; ROBOT reason may serialize only the
    non-redundant (direct) inferred edges. Subsumption is transitive, so we close
    the oracle's edge set to the SAME basis before comparing. Reflexive /
    ``owl:Thing`` edges are excluded — the comparison key is named-class
    subsumption.
    """
    store = gmeow_rdf.Store()
    for path in turtle_paths:
        store.load(path=str(path), format=gmeow_rdf.RdfFormat.TURTLE)
    edges = _subsumption_edges(store)
    closed = _transitive_closure(edges)
    return {(s, o, CROSSCHECK_WORLD) for (s, o) in closed if s != o}


def _transitive_closure(edges: set[tuple[str, str]]) -> set[tuple[str, str]]:
    """Transitively close a set of ``(subject, object)`` subsumption edges.

    Subsumption is transitive (``A ⊑ B`` ∧ ``B ⊑ C`` ⟹ ``A ⊑ C``); the native
    engine materializes the full closure, so the oracle side is closed to match.
    Self-edges are left in (the caller drops reflexive rows).
    """
    successors: dict[str, set[str]] = {}
    for subject, obj in edges:
        successors.setdefault(subject, set()).add(obj)
    closed: set[tuple[str, str]] = set(edges)
    changed = True
    while changed:
        changed = False
        for subject, obj in list(closed):
            for grandparent in successors.get(obj, set()):
                pair = (subject, grandparent)
                if pair not in closed:
                    closed.add(pair)
                    successors.setdefault(subject, set()).add(grandparent)
                    changed = True
    return closed


def elk_subsumptions(
    told_facts: Path,
) -> tuple[list[tuple[str, str, str]], float]:
    """Run ELK over the native TOLD facts and extract the subsumption closure.

    ``told_facts`` is the SAME corpus the native engine reasons (see
    :func:`write_told_facts`). ELK closes it and we read back the told + inferred
    ``rdfs:subClassOf`` hierarchy. Returns ``(subsumptions, elapsed_seconds)``.
    """
    from gmeow_tools import reason as reasoning

    start = time.monotonic()
    reasoned = reasoning.reason(
        "ELK", merged=told_facts, exclude_tautologies="structural"
    )
    elapsed = time.monotonic() - start
    # Close over BOTH the ELK-reasoned output and the told-facts input so the
    # closure basis includes the asserted equivalences ROBOT collapses away.
    rows = sorted(_subclassof_closure(reasoned, told_facts))
    return rows, elapsed


# --------------------------------------------------------------------------- #
# HermiT verdict extraction (Docker oracle)
# --------------------------------------------------------------------------- #


def hermit_verdict(told_facts: Path) -> tuple[bool, list[str], float]:
    """Run HermiT over the native TOLD facts → ``(consistent, unsat, elapsed_s)``.

    ``told_facts`` is the SAME corpus the native engine reasons (see
    :func:`write_told_facts`). HermiT's ``reason`` exits non-zero on an
    inconsistent/incoherent ontology (surfaced as :class:`ToolExecutionError`); a
    clean exit means consistent and coherent. On incoherence we recover the
    unsatisfiable classes from :func:`reason.explain_unsatisfiable` and report
    ``consistent=False``.
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError

    start = time.monotonic()
    try:
        reasoning.reason("hermit", merged=told_facts)
        consistent = True
        unsat: list[str] = []
    except ToolExecutionError:
        consistent = False
        unsat = _hermit_unsat_classes(told_facts)
    elapsed = time.monotonic() - start
    return consistent, unsat, elapsed


def _hermit_unsat_classes(told_facts: Path) -> list[str]:
    """Recover the unsatisfiable-class IRIs from HermiT's explanation output.

    A *fully inconsistent* (not merely incoherent) ontology cannot be explained
    class-by-class — ROBOT ``explain`` itself errors. In that case the load-bearing
    signal is already the ``consistent=False`` verdict (the comparator flags the
    consistency divergence on the boolean alone), so an explain failure degrades to
    an empty unsat-class list rather than crashing the lane.
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError

    try:
        explanation = reasoning.explain_unsatisfiable(merged=told_facts)
    except ToolExecutionError:
        return []
    if not explanation:
        return []
    # The explanation markdown cites the unsatisfiable classes as IRIs; pull any
    # bare http(s) IRIs out so the set is comparable to native. The character
    # class already drops whitespace and < > ", but a trailing sentence or
    # markdown delimiter (".", ")", "]", …) would otherwise ride along and forge
    # a phantom IRI. In the strict-fail lane that becomes a false divergence and
    # a spurious build failure, so strip trailing punctuation to land the exact IRI.
    import re

    trailing = ".,;:!?)]}'\""
    iris = (
        match.rstrip(trailing)
        for match in re.findall(r"https?://[^\s<>\"]+", explanation)
    )
    return sorted({iri for iri in iris if iri})


# --------------------------------------------------------------------------- #
# Ledger + SARIF emission + enforcement
# --------------------------------------------------------------------------- #


def build_ledger(
    *,
    native: dict[str, Any],
    elk_subs: Iterable[tuple[str, str, str]],
    hermit_consistent: bool,
    hermit_unsat: list[str],
) -> dict[str, Any]:
    """Call the AUTHORITATIVE Rust comparator over native + oracle results."""
    import gmeow_logic

    return gmeow_logic.build_divergence_ledger(
        native_subsumptions(native),
        list(elk_subs),
        bool(native["consistent"]),
        native_unsat_classes(native),
        hermit_consistent,
        hermit_unsat,
        native_gaps(native),
    )


def _rule_id_for(row: dict[str, Any]) -> str:
    """Map a classified ledger row to its gate-taxonomy rule-id."""
    if row["kind"] == "DlGap":
        return RULE_DL_GAP
    if row["category"] == "subsumption":
        return RULE_SUBSUMPTION_DIVERGENCE
    return RULE_CONSISTENCY_DIVERGENCE


def build_report(
    ledger: dict[str, Any],
    *,
    elk_seconds: float,
    hermit_seconds: float,
) -> DiagnosticsReport:
    """Build the diagnostics report carrying the agreement matrix + timing.

    Every ledger row becomes a finding tagged with its kind; ``NativeOnly`` /
    ``OracleOnly`` are ``error`` severity (they FAIL the lane), ``DlGap`` is a
    ``note`` (honest-expected), and ``Agree`` is ``info``. The per-tool wall-clock
    is recorded as ``note`` findings so the artifact carries the timing too.
    """
    from gmeow_tools import diagnostics

    report = diagnostics.report(tool="classic-cross-check")

    # Per-tool timing (the artifact carries the per-tool wall-clock).
    report.add(
        diagnostics.finding(
            severity="note",
            code=RULE_TIMING,
            message=f"ELK subsumption oracle ran in {elk_seconds:.2f}s",
            tool="elk",
            tags=["timing"],
        )
    )
    report.add(
        diagnostics.finding(
            severity="note",
            code=RULE_TIMING,
            message=f"HermiT consistency oracle ran in {hermit_seconds:.2f}s",
            tool="hermit",
            tags=["timing"],
        )
    )

    # Agreement-matrix summary.
    report.add(
        diagnostics.finding(
            severity="note",
            code=RULE_AGREEMENT,
            message=(
                f"agreement matrix: agree={ledger['agree']} "
                f"native_only={ledger['native_only']} "
                f"oracle_only={ledger['oracle_only']} dl_gap={ledger['dl_gap']}"
            ),
            tool="classic-cross-check",
            tags=["agreement-matrix"],
        )
    )

    # One finding per classified row; severity by kind (enforcement taxonomy).
    severity_by_kind = {
        "Agree": "info",
        "NativeOnly": "error",
        "OracleOnly": "error",
        "DlGap": "note",
    }
    for row in ledger["rows"]:
        kind = row["kind"]
        report.add(
            diagnostics.finding(
                severity=severity_by_kind[kind],
                code=_rule_id_for(row),
                message=row["detail"],
                tool="classic-cross-check",
                detail=(
                    f"kind={kind} category={row['category']} "
                    f"subject={row['subject']} object={row['object']}"
                ),
                tags=[kind, row["category"]],
            )
        )
    return report


def enforce(ledger: dict[str, Any]) -> bool:
    """Strict-by-default verdict: True (pass) unless a real divergence exists.

    A ``NativeOnly`` or ``OracleOnly`` row is a real divergence and FAILS the
    lane. ``DlGap`` is the ONLY honest-expected, non-failing class; ``Agree``
    passes. There is no severity knob (ETHOS §5/§19).
    """
    return int(ledger["native_only"]) == 0 and int(ledger["oracle_only"]) == 0


def run(
    *,
    gts: Path = GTS_SNAPSHOT_FILE,
    output_dir: Path = DIST_DIR,
) -> tuple[bool, dict[str, Any], DiagnosticsReport]:
    """Run the full enforced cross-check and write the SARIF/JSON artifacts.

    Returns ``(passed, ledger, report)``. ``passed`` is the strict enforcement
    verdict; the caller exits non-zero when it is False. The SARIF/JSON/HTML
    artifacts are a LANE output (under ``output_dir``), NOT a committed
    drift-gated generator.
    """
    from gmeow_tools import diagnostics

    native = reason_native(gts)
    # Stage the native TOLD facts as a single ontology so the Docker oracles close
    # the SAME corpus the native engine reasons (apples-to-apples; see
    # write_told_facts). Without this they would reason a different ontology.
    told_facts = write_told_facts(native)
    elk_subs, elk_seconds = elk_subsumptions(told_facts)
    hermit_consistent, hermit_unsat, hermit_seconds = hermit_verdict(told_facts)

    ledger = build_ledger(
        native=native,
        elk_subs=elk_subs,
        hermit_consistent=hermit_consistent,
        hermit_unsat=hermit_unsat,
    )
    report = build_report(
        ledger, elk_seconds=elk_seconds, hermit_seconds=hermit_seconds
    )

    output_dir.mkdir(parents=True, exist_ok=True)
    diagnostics.write_report_artifacts(
        report, output_dir=output_dir, stem=CROSSCHECK_STEM
    )

    passed = enforce(ledger)
    return passed, ledger, report
