"""Reasoning pipeline over the GMEOW ontology, via pinned ROBOT (Docker).

The pipeline always *merges the import closure into a single ontology first*,
then reasons/validates that product. This is deliberate: ROBOT's
``validate-profile`` reports spurious "undeclared entity" violations when terms
are declared in a sibling imported module rather than the local import closure;
collapsing to one ontology resolves it (verified against the skeleton).

Reasoner choice follows the plan: ELK for fast incoherence checks in CI,
HermiT for sound-and-complete OWL 2 DL consistency at release time.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from gmeow_tools.config import (
    CATALOG_FILE,
    DIST_DIR,
    GTS_SNAPSHOT_FILE,
    NAMESPACE,
    ONTOLOGY_FILE,
    PROJECT_ROOT,
    ROBOT_IMAGE,
    STATEMENT_OWL_FILE,
    VERIFY_DIR,
)
from gmeow_tools.runner import run_container
from gmeow_tools.slices import iter_slice_query_files

if TYPE_CHECKING:
    from gmeow_tools.diagnostics import DiagnosticsReport

#: Canonical merged (asserted) release product.
MERGED_FILE = DIST_DIR / "gmeow-merged.ttl"
#: Reasoned product carrying inferred axioms (release closure).
FULL_FILE = DIST_DIR / "gmeow-full.ttl"

#: The native reasoning lane's inferred-closure artifact (told-vs-inferred, in
#: RDF 1.2 with per-triple derivation provenance). Java/Docker-free authority.
INFERRED_CLOSURE_FILE = DIST_DIR / "gmeow-inferred-closure.rdf12.ttl"
#: Diagnostics-artifact stem for the native lane (JSON / SARIF / HTML).
NATIVE_REASON_STEM = "gmeow-reason-native"

#: IRI base for a reasoning rule, minted under the gmeow namespace so the
#: ``gmeow:viaRule`` annotation points at a dereferenceable, namespaced term
#: rather than a bare rule label.
_RULE_IRI_BASE = NAMESPACE + "rule/"


def _rel(path: Path) -> str:
    """Return a container path (relative to the repo root mounted at /work)."""
    return str(path.relative_to(PROJECT_ROOT))


#: HermiT sound+complete consistency over the full merged ontology runs ~15 min
#: and grows with the ontology; the default 900s container ceiling sits right at
#: that cliff (a trivial property addition has tipped main's 879s over). HermiT
#: gets a wider ceiling; every other (fast) ROBOT op keeps the default. Speeding
#: HermiT up is tracked for the gate-health pass (#433).
_HERMIT_TIMEOUT: float = 1800.0


def _robot(args: list[str], *, timeout: float = 900.0) -> str:
    """Run a ROBOT command and return combined stdout+stderr."""
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    result = run_container(ROBOT_IMAGE, ["robot", *args], timeout=timeout)
    return result.stdout + result.stderr


def merge_release(
    output: Path = MERGED_FILE, *, include_statements: bool = True
) -> Path:
    """Merge the import closure into a single, self-contained ontology.

    The generated OWL axiom-annotation downcast of the canonical RDF 1.2 statement
    metadata (``statements/gmeow-statements.owl.ttl``) is merged in too, so the
    reasoner consumes the statement layer as a *generated downcast* — never an
    authored one (CONSTITUTION Principles 2-3). Its freshness is guarded
    separately by ``gmeow compile-statements --check``; merging the committed file
    keeps reasoning a pure-ROBOT step (no Jena dependency here).

    Args:
        output: Destination for the merged Turtle file.
        include_statements: Merge the statement-metadata OWL downcast when present.

    Returns:
        The path to the merged ontology.
    """
    # The root IRI is the CORE profile (#330); the global gate keeps
    # covering everything by merging the generated FULL profile instead.
    from gmeow_tools.config import FULL_PROFILE_FILE

    merge_input = FULL_PROFILE_FILE if FULL_PROFILE_FILE.exists() else ONTOLOGY_FILE
    args = ["merge", "--catalog", _rel(CATALOG_FILE), "--input", _rel(merge_input)]
    if include_statements and STATEMENT_OWL_FILE.exists():
        args += ["--input", _rel(STATEMENT_OWL_FILE)]
    import uuid

    tmp = output.with_name(f"{output.stem}.{uuid.uuid4().hex}{output.suffix}")
    args += ["--collapse-import-closure", "true", "--output", _rel(tmp)]
    try:
        _robot(args)
        tmp.replace(output)
    finally:
        tmp.unlink(missing_ok=True)
    return output


def validate_profile(profile: str = "DL", *, merged: Path = MERGED_FILE) -> str:
    """Validate the merged ontology against an OWL 2 profile.

    Args:
        profile: OWL 2 profile (``DL``, ``EL``, ``QL``, ``RL``, ``Full``).
        merged: The merged ontology to validate (produced if absent).

    Returns:
        The ROBOT report text.

    Raises:
        ToolExecutionError: If the ontology violates the profile.
    """
    if not merged.exists():
        merge_release(merged)
    return _robot(["validate-profile", "--profile", profile, "--input", _rel(merged)])


def reason(
    reasoner: str = "ELK",
    *,
    merged: Path = MERGED_FILE,
    exclude_tautologies: str | None = None,
) -> Path:
    """Run a reasoner over the merged ontology to check coherence.

    ROBOT exits non-zero if the ontology is inconsistent or has unsatisfiable
    classes, which surfaces as :class:`ToolExecutionError`.

    Args:
        reasoner: ``ELK`` (fast, EL) or ``hermit`` (sound+complete DL).
        merged: The merged ontology (produced if absent).
        exclude_tautologies: If given, pass ``--exclude-tautologies`` to the
            reason step. ``"structural"`` is used by the verify pipeline so the
            pre-reasoned graph matches what the chained ``reason ... verify``
            command would have produced.

    Returns:
        Path to the reasoned output written under ``dist/``.
    """
    if not merged.exists():
        merge_release(merged)
    output = DIST_DIR / f"gmeow-reasoned-{reasoner.lower()}.ttl"
    timeout = _HERMIT_TIMEOUT if reasoner.lower() == "hermit" else 900.0
    args = [
        "reason",
        "--reasoner",
        reasoner,
        "--input",
        _rel(merged),
    ]
    if exclude_tautologies:
        args += ["--exclude-tautologies", exclude_tautologies]
    args += ["--output", _rel(output)]
    _robot(args, timeout=timeout)
    return output


#: OWL-native release syntaxes (#12): extension → ROBOT convert format.
#: ofn and owx are lossless OWL 2 forms; omn (Manchester) is itself LOSSY —
#: it cannot express every OWL 2 axiom (GCIs etc.; ROBOT warns and drops).
OWL_SYNTAXES: dict[str, str] = {"ofn": "ofn", "owx": "owx", "omn": "omn"}


def convert_owl_syntaxes(*, merged: Path = MERGED_FILE) -> list[Path]:
    """Emit the merged release in the OWL-native syntaxes via pinned ROBOT.

    Functional (``gmeow.ofn``), OWL/XML (``gmeow.owx``), and Manchester
    (``gmeow.omn``, declared lossy) — the release-tier companions to the
    RDF serializations of :func:`gmeow_tools.serialize.serialize_graph`.

    Returns:
        The written paths under ``dist/``.
    """
    if not merged.exists():
        merge_release(merged)
    written: list[Path] = []
    for ext, fmt in OWL_SYNTAXES.items():
        output = DIST_DIR / f"gmeow.{ext}"
        _robot(
            [
                "convert",
                "--input",
                _rel(merged),
                "--format",
                fmt,
                "--output",
                _rel(output),
            ]
        )
        written.append(output)
    return written


def explain_unsatisfiable(
    *, merged: Path = MERGED_FILE, output: Path = DIST_DIR / "gmeow-explanation.md"
) -> str:
    """Explain unsatisfiable classes / inconsistency, if any.

    Args:
        merged: The merged ontology (produced if absent).
        output: Markdown file ROBOT writes the explanation to.

    Returns:
        The ROBOT explain report text (empty problem set if coherent).
    """
    if not merged.exists():
        merge_release(merged)
    # ROBOT writes the justification to the --explanation file (not stdout) and
    # needs --unsatisfiable to say which classes to explain ("all" = every
    # unsatisfiable class).
    _robot(
        [
            "explain",
            "--input",
            _rel(merged),
            "--reasoner",
            "hermit",
            "--mode",
            "unsatisfiability",
            "--unsatisfiable",
            "all",
            "--explanation",
            _rel(output),
        ]
    )
    if not output.exists():
        return ""
    text = output.read_text(encoding="utf-8").strip()
    return "" if text == "No explanations found." else text


def verify(
    *,
    merged: Path = MERGED_FILE,
    queries: Path = VERIFY_DIR,
    reasoner: str = "ELK",
    output_dir: Path = DIST_DIR / "verify",
    reasoned: Path | None = None,
) -> str:
    """Run the reasoned-graph negative tests (ROBOT ``verify``).

    The closed-world half of the OWL-infers / SHACL-validates split: ROBOT
    ``reason`` materializes the ontology, then ``verify`` runs the SPARQL SELECT
    "bad-example" queries in ``queries/verify/`` over it (the OBO QC pattern). Any
    query that returns a row is a violation, so ROBOT exits non-zero and the
    failure surfaces as :class:`ToolExecutionError`. Reasoning runs with
    ``--exclude-tautologies structural`` so trivial entailments (e.g.
    ``X subClassOf owl:Thing``) never trip a verify query. Unlike the
    ``gmeow_shacl`` SHACL lane (asserted graph only), these checks see the
    reasoned closure and
    so catch violations that only appear after inference. See docs/reasoning.md.

    Args:
        merged: The merged ontology (produced if absent). Ignored when
            *reasoned* is provided.
        queries: Directory of ``*.rq`` SELECT verify queries.
        reasoner: ``ELK`` (fast, EL) or ``hermit`` (sound+complete DL).
        output_dir: Directory ROBOT writes the per-query violation reports to.
        reasoned: Pre-computed reasoned ontology. When given, ``verify`` runs
            only the SPARQL queries against it, avoiding a second reasoning
            pass. The caller is responsible for ensuring the reasoned file was
            produced with the same reasoner and tautology settings expected by
            the verify queries.

    Returns:
        The ROBOT report text (empty problem set if every query is clean).

    Raises:
        ToolExecutionError: If any verify query returns offending rows.
    """
    query_files = sorted(queries.glob("*.rq"))
    if queries == VERIFY_DIR:
        # Slices carry their own verify queries (slices/*/*/queries/verify/).
        query_files += iter_slice_query_files("verify")
    if not query_files:
        raise FileNotFoundError(f"no verify queries found in {queries}")
    output_dir.mkdir(parents=True, exist_ok=True)
    timeout = _HERMIT_TIMEOUT if reasoner.lower() == "hermit" else 900.0

    if reasoned is not None:
        reasoned = reasoned.resolve()
        if not reasoned.exists():
            raise FileNotFoundError(
                f"pre-computed reasoned input not found: {reasoned}"
            )
    if reasoned is not None:
        # Fast path: use a previously materialized reasoned graph.
        return _robot(
            [
                "verify",
                "--input",
                _rel(reasoned),
                "--queries",
                *[_rel(q) for q in query_files],
                "--output-dir",
                _rel(output_dir),
            ],
            timeout=timeout,
        )

    if not merged.exists():
        merge_release(merged)
    return _robot(
        [
            "reason",
            "--reasoner",
            reasoner,
            "--exclude-tautologies",
            "structural",
            "--input",
            _rel(merged),
            "verify",
            "--queries",
            *[_rel(q) for q in query_files],
            "--output-dir",
            _rel(output_dir),
        ],
        timeout=timeout,
    )


def build_full(*, merged: Path = MERGED_FILE, output: Path = FULL_FILE) -> Path:
    """Build the release closure: reason (HermiT), relax, reduce, annotate.

    Produces ``gmeow-full.ttl`` with inferred subsumptions made explicit and
    annotated as inferred — the publishable reasoned artifact.

    Args:
        merged: The merged ontology (produced if absent).
        output: Destination for the reasoned closure.

    Returns:
        The path to the reasoned closure.
    """
    if not merged.exists():
        merge_release(merged)
    _robot(
        [
            "reason",
            "--reasoner",
            "hermit",
            "--input",
            _rel(merged),
            "relax",
            "reduce",
            "--reasoner",
            "hermit",
            "annotate",
            "--ontology-iri",
            "https://blackcatinformatics.ca/gmeow",
            "--output",
            _rel(output),
        ],
        timeout=_HERMIT_TIMEOUT,
    )
    return output


# --------------------------------------------------------------------------- #
# Native reasoning lane (Rust, Java/Docker-free authority)
# --------------------------------------------------------------------------- #

#: Minimal prefix block for the inferred-closure RDF 1.2 artifact.
_CLOSURE_PREFIXES = (
    "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"
    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
    "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n"
    "@prefix prov: <http://www.w3.org/ns/prov#> .\n"
    f"@prefix gmeow: <{NAMESPACE}> .\n"
)

#: Banner prepended to the inferred-closure artifact.
_CLOSURE_BANNER = (
    "# GMEOW native inferred closure (RDF 1.2).\n"
    "# The told-vs-inferred derived axioms produced by the native EL/DL\n"
    "# reasoning lane (gmeow_logic.reason_native, Java/Docker-free). Each\n"
    "# inferred triple carries an RDF 1.2 reifier annotated with its\n"
    "# derivation provenance (prov:wasDerivedBy / gmeow:viaRule). DO NOT EDIT.\n"
)


def _iri_term(value: str) -> str:
    """Normalize a native-engine term into an angle-bracketed full IRI.

    The native engine emits subjects/predicates as bare IRI strings and objects
    already wrapped in ``<...>``; this collapses both to one ``<iri>`` form.
    """
    inner = value[1:-1] if value.startswith("<") and value.endswith(">") else value
    return f"<{inner}>"


def _rule_iri(rule_name: str) -> str:
    """Mint a namespaced, percent-encoded IRI for a reasoning rule label."""
    from urllib.parse import quote

    return f"<{_RULE_IRI_BASE}{quote(rule_name, safe='')}>"


def build_inferred_closure_ttl(
    result: dict[str, Any], *, merge_asserted_from: bytes | None = None
) -> str:
    """Render the native told-vs-inferred closure as an RDF 1.2 Turtle document.

    For every entailment in ``result['inferred']`` whose ``is_edb`` is false (a
    *derived*, not asserted, axiom) this emits the triple plus an RDF 1.2
    reifier carrying its derivation provenance: ``prov:wasDerivedBy`` and
    ``gmeow:viaRule`` (both pointing at the namespaced rule IRI),
    ``gmeow:inferenceKind gmeow:Deduction``, and ``gmeow:inWorld`` recording the
    world (named graph) the entailment holds in. The world is carried as an
    annotation rather than a Turtle named graph so the artifact stays valid
    Turtle (it parses under ``RdfFormat.TURTLE``).

    This function is pure (dict in, string out) so the Task 5 generator can reuse
    it without any I/O.

    Args:
        result: The dict returned by ``gmeow_logic.reason_native``.
        merge_asserted_from: When given, the asserted GTS bundle bytes; their
            told graph is prepended so the document is the *union* of asserted
            and derived axioms (the ``--merge`` mode).

    Returns:
        A valid RDF 1.2 Turtle document.
    """
    parts: list[str] = [_CLOSURE_BANNER, _CLOSURE_PREFIXES]

    if merge_asserted_from is not None:
        asserted = _asserted_turtle(merge_asserted_from)
        if asserted:
            parts.append(
                "\n# --- asserted (told) graph (union; --merge) ---\n" + asserted
            )

    parts.append("\n# --- derived (inferred) closure ---\n")
    for axiom in result.get("inferred", []):
        if axiom.get("is_edb"):
            continue
        subject = _iri_term(axiom["subject"])
        predicate = _iri_term(axiom["predicate"])
        obj = _iri_term(axiom["object"])
        rule_iri = _rule_iri(axiom["rule_name"])
        world_iri = _iri_term(axiom["world"])
        parts.append(f"{subject} {predicate} {obj} .\n")
        parts.append(
            f"[] rdf:reifies << {subject} {predicate} {obj} >> ;\n"
            f"   prov:wasDerivedBy {rule_iri} ;\n"
            f"   gmeow:viaRule {rule_iri} ;\n"
            f"   gmeow:inferenceKind gmeow:Deduction ;\n"
            f"   gmeow:inWorld {world_iri} .\n"
        )
    return "".join(parts)


#: Banner prepended to the proof-skeleton explanations artifact.
_EXPLANATIONS_BANNER = (
    "# GMEOW native reasoning explanations (RDF 1.2 proof skeletons).\n"
    "# For every derived axiom the native EL/DL lane produced, a derivation\n"
    "# node links the conclusion (an RDF 1.2 reifier) to its premises and the\n"
    "# rule that fired (gmeow:viaRule). Pure native-lane output. DO NOT EDIT.\n"
)

#: Banner prepended to the native↔oracle divergence ledger (report-only; #666).
_LEDGER_BANNER = (
    "# GMEOW native vs ELK/HermiT DL/EL crosscheck ledger (REPORT-ONLY).\n"
    "# Built from the native EL/DL reasoning lane ONLY (Java/Docker-free). The\n"
    "# oracle comparison and divergence ENFORCEMENT are deferred to the\n"
    "# classic-cross-check lane (#666); this ledger records the native verdict,\n"
    "# the native-only subsumption entailments, and the beyond-EL gaps. DO NOT\n"
    "# EDIT.\n"
)


def build_explanations_ttl(result: dict[str, Any]) -> str:
    """Render an RDF 1.2 proof skeleton for every derived axiom.

    For each entailment in ``result['inferred']`` whose ``is_edb`` is false (a
    *derived*, not asserted, axiom) this emits a derivation node that links the
    conclusion triple — carried as an RDF 1.2 reifier (``rdf:reifies``) — to its
    premises (each premise triple, also reified) via ``gmeow:hasPremise`` and to
    the rule that fired via ``gmeow:viaRule``. The conclusion reifier is attached
    with ``gmeow:concludes``. The world the derivation holds in is recorded with
    ``gmeow:inWorld``. Premises are taken from the entailment's antecedent triples
    when the native engine supplies them; otherwise the derivation records only
    the rule and conclusion (a one-step justification skeleton).

    This function is pure (dict in, string out); it parses under
    ``RdfFormat.TURTLE``. Human-readable literals carry an explicit ``@en`` tag.

    Args:
        result: The dict returned by ``gmeow_logic.reason_native``.

    Returns:
        A valid RDF 1.2 Turtle document of proof skeletons.
    """
    parts: list[str] = [_EXPLANATIONS_BANNER, _CLOSURE_PREFIXES]
    parts.append("\n# --- derivation proof skeletons ---\n")
    for axiom in result.get("inferred", []):
        if axiom.get("is_edb"):
            continue
        subject = _iri_term(axiom["subject"])
        predicate = _iri_term(axiom["predicate"])
        obj = _iri_term(axiom["object"])
        rule_iri = _rule_iri(axiom["rule_name"])
        world_iri = _iri_term(axiom["world"])
        conclusion = f"<< {subject} {predicate} {obj} >>"
        premise_lines = ""
        for premise in axiom.get("premises", []):
            p_subject = _iri_term(premise["subject"])
            p_predicate = _iri_term(premise["predicate"])
            p_object = _iri_term(premise["object"])
            premise_lines += (
                f"   gmeow:hasPremise << {p_subject} {p_predicate} {p_object} >> ;\n"
            )
        parts.append(
            f"[] a gmeow:Derivation ;\n"
            f"   gmeow:concludes {conclusion} ;\n"
            f"{premise_lines}"
            f"   gmeow:viaRule {rule_iri} ;\n"
            f"   gmeow:inferenceKind gmeow:Deduction ;\n"
            f'   rdfs:label "derivation of an inferred axiom"@en ;\n'
            f"   gmeow:inWorld {world_iri} .\n"
        )
    return "".join(parts)


def build_dl_el_ledger_ttl(result: dict[str, Any]) -> str:
    """Render the report-only native↔oracle DL/EL crosscheck ledger as Turtle.

    Built from the NATIVE results ONLY (the gate must stay Java/Docker-free): the
    oracle (ELK/HermiT) comparison and divergence *enforcement* are deferred to
    the ``classic-cross-check`` lane (#666). The ledger therefore records, for the
    native EL/DL lane:

    * one ``gmeow:LedgerEntry`` of kind ``gmeow:NativeOnly`` per derived
      subsumption (``rdfs:subClassOf``) entailment, each annotated with a note
      that oracle comparison is deferred to ``classic-cross-check`` (#666);
    * the native consistency verdict (``gmeow:consistent``);
    * one ``gmeow:DlGap`` resource per beyond-EL gap (its code + message);
    * the entailment / gap counts;
    * a top-level ``gmeow:oracleCrosscheck`` note marking the ledger report-only.

    This function is pure (dict in, string out); it parses under
    ``RdfFormat.TURTLE``. Human-readable literals carry an explicit ``@en`` tag.

    Args:
        result: The dict returned by ``gmeow_logic.reason_native``.

    Returns:
        A valid RDF 1.2 Turtle document (the report-only ledger).
    """
    deferred_note = "oracle comparison deferred to classic-cross-check #666"
    parts: list[str] = [_LEDGER_BANNER, _CLOSURE_PREFIXES]
    parts.append(
        "\n# --- ledger header (report-only; #666 enforces) ---\n"
        f"gmeow:dl-el-crosscheck a gmeow:CrosscheckLedger ;\n"
        f"   gmeow:consistent {'true' if result.get('consistent') else 'false'} ;\n"
        '   gmeow:oracleCrosscheck "deferred to classic-cross-check (#666); '
        'ledger is report-only"@en .\n'
    )

    subsumptions = [
        axiom
        for axiom in result.get("inferred", [])
        if not axiom.get("is_edb")
        and _iri_term(axiom["predicate"])
        == "<http://www.w3.org/2000/01/rdf-schema#subClassOf>"
    ]
    gaps = result.get("gaps", [])

    parts.append("\n# --- native-only subsumption entailments ---\n")
    for index, axiom in enumerate(subsumptions):
        subject = _iri_term(axiom["subject"])
        obj = _iri_term(axiom["object"])
        world_iri = _iri_term(axiom["world"])
        parts.append(
            f"gmeow:ledger-entry-{index} a gmeow:LedgerEntry ;\n"
            f"   gmeow:entryKind gmeow:NativeOnly ;\n"
            f"   gmeow:subsumes << {subject} "
            f"<http://www.w3.org/2000/01/rdf-schema#subClassOf> {obj} >> ;\n"
            f"   gmeow:inWorld {world_iri} ;\n"
            f'   rdfs:comment "{deferred_note}"@en .\n'
        )

    parts.append("\n# --- beyond-EL DL gaps ---\n")
    for index, gap in enumerate(gaps):
        message = _escape_literal(str(gap.get("message", "")))
        code = _escape_literal(str(gap.get("code", "")))
        parts.append(
            f"gmeow:dl-gap-{index} a gmeow:DlGap ;\n"
            f'   gmeow:gapCode "{code}"@en ;\n'
            f'   rdfs:comment "{message}"@en .\n'
        )

    parts.append(
        "\n# --- counts ---\n"
        f"gmeow:dl-el-crosscheck gmeow:entailmentCount {len(subsumptions)} ;\n"
        f"   gmeow:gapCount {len(gaps)} .\n"
    )
    return "".join(parts)


def _escape_literal(value: str) -> str:
    """Escape a string for embedding in a double-quoted Turtle literal."""
    return value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def _asserted_turtle(gts_bytes: bytes) -> str:
    """Serialize the asserted GTS bundle's triples to Turtle (RDF 1.2 / star).

    The bundle's named-graph quads are flattened into the closure document's
    default graph (the graph component is dropped) so the artifact is a single
    Turtle document. pyoxigraph serializes and re-parses any RDF-star triple
    terms the asserted statement layer carries, so the round-trip stays valid
    under ``RdfFormat.TURTLE``. Uses gts + pyoxigraph (no rdflib; #629).
    """
    from io import BytesIO

    import gts as _gts
    import pyoxigraph

    quads = pyoxigraph.Store()
    nquads = _gts.to_nquads(_gts.read(gts_bytes))
    quads.load(nquads.encode("utf-8"), format=pyoxigraph.RdfFormat.N_QUADS)
    flat = pyoxigraph.Store()
    flat.extend(
        pyoxigraph.Quad(quad.subject, quad.predicate, quad.object) for quad in quads
    )
    buffer = BytesIO()
    pyoxigraph.serialize(flat, buffer, format=pyoxigraph.RdfFormat.TURTLE)
    return buffer.getvalue().decode("utf-8")


def reason_native(
    *,
    gts: Path = GTS_SNAPSHOT_FILE,
    merge: bool = False,
    output_dir: Path = DIST_DIR,
    run_box_roles: bool = True,
) -> DiagnosticsReport:
    """Run the native EL/DL reasoning lane and emit its diagnostics + closure.

    The Java/Docker-free authority lane (Principle 17): the Rust engine reasons
    the bundle, this builds the diagnostics report (consistency verdict,
    beyond-EL gaps, any inconsistency/unsatisfiability), folds in the four-box
    role audit, writes the inferred-closure RDF 1.2 artifact, and writes the
    JSON / SARIF / HTML diagnostics artifacts. It never raises on an
    inconsistent ontology — the caller inspects ``report.ok``.

    Args:
        gts: The committed GTS bundle to reason over.
        merge: When true, the closure artifact is the union of the asserted and
            derived graphs; otherwise it carries only the derived axioms.
        output_dir: Destination directory for all artifacts.
        run_box_roles: When true, fold the four-box graph-role audit findings in.

    Returns:
        The diagnostics report (its ``ok`` reflects reasoning consistency).
    """
    import gmeow_logic

    from gmeow_tools import diagnostics
    from gmeow_tools.box_roles import audit_box_roles

    gts_bytes = gts.read_bytes()
    result = gmeow_logic.reason_native(gts_bytes)

    derived = [a for a in result.get("inferred", []) if not a.get("is_edb")]
    gaps = result.get("gaps", [])
    report = diagnostics.report(tool="reason")
    report.add(
        diagnostics.finding(
            severity="note",
            code="reason.native.summary",
            message=(
                f"native EL/DL reasoning: consistent={result['consistent']}, "
                f"{len(derived)} entailments, {len(gaps)} beyond-EL gaps"
            ),
            tool="reason",
        )
    )
    report.add(
        diagnostics.finding(
            severity="info",
            code="reason.native.shacl",
            message=(
                "structural SHACL conformance is enforced by the validate gate "
                "(gmeow_shacl); the native reasoning lane composes with it in "
                "make check"
            ),
            tool="reason",
        )
    )
    for gap in gaps:
        report.add(
            diagnostics.finding(
                severity="note",
                code=gap["code"],
                message=gap["message"],
                tool="reason",
            )
        )
    for incon in result.get("inconsistencies", []):
        report.add(
            diagnostics.finding(
                severity="error",
                code="reason.inconsistent",
                message=(
                    f"individual {incon['individual']} forced into owl:Nothing "
                    f"in world {incon['world']}"
                ),
                tool="reason",
            )
        )
    for unsat in result.get("unsatisfiable_classes", []):
        report.add(
            diagnostics.finding(
                severity="warning",
                code="reason.unsatisfiable",
                message=(
                    f"class {unsat['class']} is unsatisfiable in world {unsat['world']}"
                ),
                tool="reason",
            )
        )

    if run_box_roles:
        try:
            audit = audit_box_roles()
            for role_finding in (*audit.missing, *audit.invalid):
                report.add(
                    diagnostics.finding(
                        severity="warning",
                        code="box_roles",
                        message=(
                            f"{role_finding.term} ({role_finding.kind}): "
                            f"{role_finding.message}"
                        ),
                        tool="reason",
                        path=role_finding.source,
                    )
                )
        except Exception as exc:  # must never crash the authority lane
            report.add(
                diagnostics.finding(
                    severity="warning",
                    code="box_roles.unavailable",
                    message=f"four-box role audit skipped: {exc}",
                    tool="reason",
                )
            )

    output_dir.mkdir(parents=True, exist_ok=True)
    closure_ttl = build_inferred_closure_ttl(
        result, merge_asserted_from=(gts_bytes if merge else None)
    )
    (output_dir / "gmeow-inferred-closure.rdf12.ttl").write_text(
        closure_ttl, encoding="utf-8"
    )
    diagnostics.write_report_artifacts(
        report, output_dir=output_dir, stem=NATIVE_REASON_STEM
    )
    return report
