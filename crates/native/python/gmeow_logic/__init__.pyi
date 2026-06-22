# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Type stub for the gmeow_logic PyO3 extension. The signatures are transcribed
# verbatim from the `#[pyo3(signature = ...)]` annotations in crates/logic/src/py.rs —
# keep them in lockstep with that file (it is the ABI source of truth).
#
# Each function returns a freshly-built Python dict. The shapes are:
#   materialize -> {"facts": [...], "derivations": [...], "budget_status": str,
#                   "incomplete": bool, ...}
#   certify     -> the CertificationVerdict.to_json() dict
#   query       -> {"bindings": [{var: str, ...}, ...], "status": str}
# They are typed as ``dict[str, Any]`` here (mypy then checks the *call sites* —
# arity and argument types — which is where FFI mistakes hide).

from typing import Any, TypedDict

from gmeow_rdf import Quad

class LedgerEntry(TypedDict):
    preservation: str
    complexity: str
    lossy_drops: list[str]

class CompileLogicResult(TypedDict):
    owl_dl: str
    owl_el: str
    datalog: str
    n3: str
    gufo: str
    canonical_rdf12: str
    nemo: str
    report: str
    # The `% === Rules ===` section of `nemo` — the reasoning-engine rule surface.
    nemo_rules: str
    # Per-target preservation ledger, keyed by target short-name.
    preservation_ledger: dict[str, LedgerEntry]
    # The parse diagnostics as a live, normalized ``gmeow_diagnostics`` Report
    # (#856) — built in Rust, forwarded by the Python surface. ``Any`` because
    # ``gmeow_diagnostics`` is an untyped native extension.
    diagnostics_report: Any

def materialize(
    rules: str,
    input: str,
    max_rule_firings: int | None = ...,
    max_answers: int | None = ...,
    time_ms: int | None = ...,
    profile: str | None = ...,
) -> list[dict[str, Any]]: ...
def materialize_explained(
    rules: str,
    input: str,
    max_rule_firings: int | None = ...,
    max_answers: int | None = ...,
    time_ms: int | None = ...,
    profile: str | None = ...,
) -> dict[str, list[dict[str, Any]]]:
    """Fused materialize + explain in one native call (#630).

    Runs the SAME chase as ``materialize`` and the SAME explanation skeleton as
    ``explain`` over the in-memory derivation — no Rust→Python→Rust round-trip.
    Returns a dict with two keys:

    * ``quads`` — the ``list[dict]`` ``materialize`` returns (same keys).
    * ``explanations`` — the ``list[dict]`` ``explain`` returns (one per quad,
      in order).
    """
    ...

def foundation(
    input: str,
    anti_rigidity_policy: str | None = ...,
) -> list[dict[str, Any]]: ...
def explain(quads: list[dict[str, Any]]) -> list[dict[str, Any]]: ...
def reason_native(gts_bytes: bytes) -> dict[str, Any]:
    """Reason over a GTS bundle (native EL/DL, Java/Docker-free; #665).

    Returns a dict with the keys:

    * ``consistent`` (bool) — whether the ontology is consistent.
    * ``inferred`` — list of axiom dicts ``{subject, predicate, object, world,
      is_edb, rule_name}`` (told + derived; ``is_edb`` marks asserted axioms).
    * ``unsatisfiable_classes`` — list of ``{class, world}`` dicts.
    * ``inconsistencies`` — list of ``{individual, world}`` dicts.
    * ``gaps`` — list of ``{code, message}`` dicts naming the beyond-EL axioms
      whose consistency only the HermiT oracle decides.
    """
    ...

def reason_native_artifacts(gts_bytes: bytes, merge: bool = ...) -> dict[str, str]:
    """Reason a GTS bundle ONCE and emit the 3 native RDF 1.2 artifacts (#666).

    Runs the native EL/DL reasoning lane exactly once and serializes the three
    committed artifacts via the gmeow-rdf RDF 1.2 Turtle emitter. Returns a dict
    with three string keys:

    * ``closure`` — the told-vs-inferred inferred-closure Turtle (per-triple
      derivation provenance). When ``merge`` is true the asserted (told) graph
      is prepended so the document is the union of asserted and derived axioms.
    * ``explanations`` — per-axiom proof-skeleton Turtle (conclusion → premises
      → firing rule).
    * ``ledger`` — the report-only native↔oracle DL/EL crosscheck ledger Turtle.

    Raises ``ValueError`` if the GTS bundle cannot be read, ``RuntimeError`` if
    reasoning fails or a derived axiom is missing its rule name.
    """
    ...

def rl_closure_nt(input: str) -> str:
    """Compute the native OWL 2 RL/RDF deductive closure as N-Triples (#666 Task 5).

    The Docker-free PRIMARY entailment authority that replaces the ``owlrl``
    baseline. ``input`` is N-Quads (named-graph triples close in their world) or
    N-Triples (default-graph triples close in a single sentinel world). Computes
    the closure RDF-1.2-first via the generic 4-ary ``triple(?s,?p,?o,?w)``
    encoding (predicate-as-DATA) through the Nemo chase.

    Returns the full closure (asserted + derived) rendered as a byte-stable
    N-Triples document — skolem IRI → blank node, literal display, de-dup and sort
    all happen in Rust (``RlClosure::to_ntriples``).

    Raises ``ValueError`` on an N-Quads/N-Triples parse error and ``RuntimeError``
    on a chase or decode failure.
    """
    ...

def rl_closure_quads(input: str) -> list[Quad]:
    """Compute the native OWL 2 RL/RDF closure as live ``gmeow_rdf.Quad`` objects.

    The structured twin of :func:`rl_closure_nt`: the same closure, returned as a
    list of ``gmeow_rdf.Quad`` so an rdflib adapter folds it straight back into a
    graph with no intermediate Python-side N-Triples render/parse (issue #630).
    Blank nodes round-trip as blank nodes; literals keep datatype/language; every
    quad is in the default graph.

    Raises ``ValueError`` on an N-Quads/N-Triples parse error and ``RuntimeError``
    on a chase or decode failure.
    """
    ...

def build_divergence_ledger(
    native_subsumptions: list[tuple[str, str, str]],
    elk_subsumptions: list[tuple[str, str, str]],
    native_consistent: bool,
    native_unsat: list[str],
    hermit_consistent: bool | None,
    hermit_unsat: list[str],
    gaps: list[tuple[str, str]],
) -> dict[str, Any]:
    """Build the native↔oracle divergence ledger (#666, ENFORCED lane).

    PyO3 surface over the authoritative Rust comparison logic
    (``crates/logic/src/reason/ledger.rs``); does not re-implement comparison.

    * ``native_subsumptions`` / ``elk_subsumptions`` — each a list of
      ``(subject, object, world)`` string triples.
    * ``native_consistent`` / ``hermit_consistent`` — DL consistency verdicts;
      ``hermit_consistent`` is ``None`` when HermiT was not run (recorded as a
      native-only note, never a divergence).
    * ``native_unsat`` / ``hermit_unsat`` — unsatisfiable-class IRIs.
    * ``gaps`` — list of ``(code, message)`` beyond-EL DL gaps; each becomes one
      honest, non-failing ``DlGap`` row.

    Returns a dict ``{"rows": [{kind, category, subject, object, world, detail},
    ...], "agree": int, "native_only": int, "oracle_only": int, "dl_gap": int}``
    where ``kind`` is one of ``"Agree"``, ``"NativeOnly"``, ``"OracleOnly"``,
    ``"DlGap"``.

    Raises ``ValueError`` for a malformed subsumption or gap row.
    """
    ...

def verify_native(gts_bytes: bytes, queries: list[tuple[str, str]]) -> Any:
    """Native reasoned-graph verify over a GTS bundle (Java/Docker-free; #695).

    Materializes the asserted graph unioned with the native EL/DL derived closure
    and runs each ``(repo_relative_rq_path, sparql_text)`` SELECT query over it;
    any returned row is a violation. Returns the resulting diagnostics
    :class:`gmeow_diagnostics.Report` as a **live pyclass** (one shared ``Report``
    type across the ``gmeow_native`` cdylib — no JSON round-trip, #630).
    ``report.ok`` is false iff any query returned a row.
    """
    ...

def extract_module(ontology_ttl: str, terms: list[str], method: str) -> dict[str, Any]:
    """Extract a syntactic-locality module (SLME) from Turtle (Java/Docker-free; #695).

    Computes a *module* of ``ontology_ttl`` around the seed signature ``terms``
    using bottom-/top-locality. ``method`` is one of ``"STAR"`` (default/unknown),
    ``"BOT"``, or ``"TOP"`` (case-insensitive). The module is *sound, not
    necessarily minimal*: any construct touching the signature is kept, and
    constructs not classified by exact locality are kept conservatively (with a
    ``slme.conservative-keep`` warning), so it may be a superset of ROBOT's output.

    Returns a dict with the keys:

    * ``module_ttl`` (str) — the extracted module as deterministic Turtle.
    * ``selected_axiom_count`` (int) — number of top-level (named-subject) kept triples.
    * ``method`` (str) — the normalized method actually used.
    * ``warnings`` — list of ``{code, message}`` dicts (conservative-keep /
      unknown-method findings).
    """
    ...

def certify(rules: str, profile: str) -> dict[str, Any]: ...
def stable_models(rules: str, input: str) -> dict[str, Any]: ...
def query(
    world_nquads: str,
    query_program: str,
    profile: str,
    world_iri: str | None = ...,
    max_answers: int | None = ...,
    max_steps: int | None = ...,
) -> dict[str, Any]: ...
def compile_logic(source_ttl: str) -> CompileLogicResult:
    """Compile logic: Turtle source → the 8 artifacts + a diagnostics Report (#664).

    Returns a dict with the following keys:

    * ``owl_dl``, ``owl_el``, ``datalog``, ``n3``, ``gufo``,
      ``canonical_rdf12``, ``nemo``, ``report`` — each the serialized content string.
    * ``nemo_rules`` — the ``% === Rules ===`` section of ``nemo``.
    * ``preservation_ledger`` — per-target ledger keyed by target short-name.
    * ``diagnostics_report`` — a live, normalized ``gmeow_diagnostics`` Report
      (#856) built in Rust: each parse diagnostic is a ``logic-compile.<code>``
      finding (``subject`` → logical location).  Recoverable parse issues are
      surfaced here as warnings and never block compilation.
    """
    ...
