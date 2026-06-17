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

from gmeow_tools.config import (
    CATALOG_FILE,
    DIST_DIR,
    ONTOLOGY_FILE,
    PROJECT_ROOT,
    ROBOT_IMAGE,
    STATEMENT_OWL_FILE,
    VERIFY_DIR,
)
from gmeow_tools.runner import run_container
from gmeow_tools.slices import iter_slice_query_files

#: Canonical merged (asserted) release product.
MERGED_FILE = DIST_DIR / "gmeow-merged.ttl"
#: Reasoned product carrying inferred axioms (release closure).
FULL_FILE = DIST_DIR / "gmeow-full.ttl"


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
