"""Reasoning pipeline over the GMEOW ontology, via pinned ROBOT (Docker).

**classic-cross-check only.** This module is the lane's ROBOT/Docker reasoning
plumbing — the *sole* Java+Docker reasoning surface (Principle 18). It is NOT on
the primary path: ``make check`` and the required ``quality`` gate reason
natively (``make reason`` / ``gmeow-dev reason --mode native``, Java/Docker-free),
and every pytest that drives these functions is ``docker``/``classic_cross_check``
marked or mocks them out. The only live callers are the classic-cross-check lane
(``gmeow_tools.oracles.classic_cross_check``), the maintainer ``gmeow-dev``
docker subcommands, and the divergence-ledger oracle — never normal repo use.

The pipeline always *merges the import closure into a single ontology first*,
then reasons/validates that product. This is deliberate: ROBOT's
``validate-profile`` reports spurious "undeclared entity" violations when terms
are declared in a sibling imported module rather than the local import closure;
collapsing to one ontology resolves it (verified against the skeleton).

Reasoner choice follows the plan: ELK for fast incoherence checks, HermiT for
sound-and-complete OWL 2 DL consistency — both as classic oracles the native
authority is cross-checked against, not as a gate dependency.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from gmeow_tools.config import (
    CATALOG_FILE,
    DIST_DIR,
    GTS_SNAPSHOT_FILE,
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
#: Diagnostics-artifact stem for the native reasoned-graph verify lane (#695).
NATIVE_VERIFY_STEM = "gmeow-verify-native"


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
    separately by ``make check-generated``; merging the committed file
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


def reason_native(
    *,
    gts: Path = GTS_SNAPSHOT_FILE,
    merge: bool = False,
    output_dir: Path = DIST_DIR,
    run_box_roles: bool = True,
) -> DiagnosticsReport:
    """Run the native EL/DL reasoning lane and emit its diagnostics + closure.

    The Java/Docker-free authority lane (Principles 17 and 18): the Rust engine reasons
    the bundle, this builds the diagnostics report (consistency verdict,
    native DL coverage defects, any inconsistency/unsatisfiability), folds in
    the four-box role audit, writes the inferred-closure RDF 1.2 artifact, and
    writes the JSON / SARIF / HTML diagnostics artifacts. It never raises on an
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
                f"{len(derived)} entailments, {len(gaps)} DL coverage defects"
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
        # Severity is decided by Rust (py.rs builds the "severity" key for every
        # gap dict); Python must not hardcode "error" here — read the field the
        # Rust side emits so the decision authority lives in Rust only.
        report.add(
            diagnostics.finding(
                severity=gap["severity"],
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
    # The RDF 1.2 closure Turtle is emitted natively (Rust + gmeow-rdf): the
    # gmeow-logic engine reasons the bundle once and serializes the artifact
    # through the gmeow-rdf RDF 1.2 Turtle emitter. `--merge` prepends the
    # asserted (told) graph so the document is the union of asserted and derived
    # axioms; otherwise it carries only the derived closure.
    artifacts = gmeow_logic.reason_native_artifacts(gts_bytes, merge)
    (output_dir / "gmeow-inferred-closure.rdf12.ttl").write_text(
        artifacts["closure"], encoding="utf-8"
    )
    diagnostics.write_report_artifacts(
        report, output_dir=output_dir, stem=NATIVE_REASON_STEM
    )
    return report


def verify_native(
    *,
    gts: Path = GTS_SNAPSHOT_FILE,
    queries: Path = VERIFY_DIR,
    output_dir: Path = DIST_DIR,
) -> DiagnosticsReport:
    """Run the native reasoned-graph negative tests (Java/Docker-free, #695).

    The Rust authority (``gmeow_logic.verify_native``) materializes the asserted
    graph unioned with the native EL/DL derived closure and runs every verify
    query over it; this thin wrapper does only what belongs to Python: discover
    the query files (repo + slice layout), hand the Rust core their text, rehydrate
    the returned diagnostics report, and write its JSON / SARIF / HTML artifacts.
    It never raises on a violation — the caller inspects ``report.ok``.

    Args:
        gts: The committed GTS bundle to verify over.
        queries: Directory of ``*.rq`` SELECT verify queries. When it is the
            canonical :data:`~gmeow_tools.config.VERIFY_DIR`, the per-slice verify
            queries (``slices/*/*/queries/verify/``) are appended too.
        output_dir: Destination directory for the diagnostics artifacts.

    Returns:
        The diagnostics report (its ``ok`` is false iff any query returned rows).

    Raises:
        FileNotFoundError: If no verify queries are found.
    """
    import gmeow_logic

    from gmeow_tools import diagnostics

    query_files = sorted(queries.glob("*.rq"))
    if queries == VERIFY_DIR:
        # Slices carry their own verify queries (slices/*/*/queries/verify/).
        query_files += iter_slice_query_files("verify")
    if not query_files:
        raise FileNotFoundError(f"no verify queries found in {queries}")

    # Pass (repo-relative path, query text): the path anchors each finding's
    # SARIF physicalLocation; the text is what the native engine evaluates.
    pairs = [
        (str(qf.relative_to(PROJECT_ROOT)), qf.read_text(encoding="utf-8"))
        for qf in query_files
    ]
    # verify_native hands back a live diagnostics Report pyclass directly — no
    # JSON round-trip (#630).
    report = gmeow_logic.verify_native(gts.read_bytes(), pairs)

    output_dir.mkdir(parents=True, exist_ok=True)
    diagnostics.write_report_artifacts(
        report, output_dir=output_dir, stem=NATIVE_VERIFY_STEM
    )
    return report
