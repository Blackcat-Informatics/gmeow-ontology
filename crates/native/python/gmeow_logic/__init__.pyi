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

class CompileDiagnostic(TypedDict):
    severity: str
    code: str
    message: str
    subject: str

class CompileLogicResult(TypedDict):
    owl_dl: str
    owl_el: str
    datalog: str
    n3: str
    gufo: str
    canonical_rdf12: str
    nemo: str
    report: str
    diagnostics: list[CompileDiagnostic]

def materialize(
    rules: str,
    input: str,
    max_rule_firings: int | None = ...,
    max_answers: int | None = ...,
    time_ms: int | None = ...,
    profile: str | None = ...,
) -> list[dict[str, Any]]: ...
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

def rl_closure(input: str) -> list[tuple[str, str, str, str, bool]]:
    """Compute the native OWL 2 RL/RDF deductive closure of a graph (#666 Task 5).

    The Docker-free PRIMARY entailment authority that replaces the ``owlrl``
    baseline. ``input`` is N-Quads (named-graph triples close in their world) or
    N-Triples (default-graph triples close in a single sentinel world). Computes
    the closure RDF-1.2-first via the generic 4-ary ``triple(?s,?p,?o,?w)``
    encoding (predicate-as-DATA) through the Nemo chase.

    Returns a list of ``(subject, predicate, object_nt, world, is_edb)`` tuples —
    the full closure (asserted + derived). ``subject``/``predicate`` are bare IRI
    strings; ``object_nt`` is the N-Triples object form (``<iri>`` or a quoted
    literal); ``world`` is the named-graph IRI; ``is_edb`` is true for asserted
    facts.

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

def verify_native(gts_bytes: bytes, queries: list[tuple[str, str]]) -> str:
    """Native reasoned-graph verify over a GTS bundle (Java/Docker-free; #695).

    Materializes the asserted graph unioned with the native EL/DL derived closure
    and runs each ``(repo_relative_rq_path, sparql_text)`` SELECT query over it;
    any returned row is a violation. Returns the resulting diagnostics
    :class:`gmeow_diagnostics.Report` serialized as JSON (rehydrate with
    ``gmeow_diagnostics.Report.from_json``). ``report.ok`` is false iff any query
    returned a row.
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
    """Compile logic: Turtle source → the 8 artifacts + parse diagnostics (#664).

    Returns a dict with the following keys:

    * ``owl_dl``, ``owl_el``, ``datalog``, ``n3``, ``gufo``,
      ``canonical_rdf12``, ``nemo``, ``report`` — each the serialized content string.
    * ``diagnostics`` — a list of dicts, each carrying ``severity`` (str),
      ``code`` (str), ``message`` (str), and ``subject`` (str, empty string when
      no subject).  Recoverable parse issues are surfaced here as warnings and
      never block compilation.
    """
    ...
