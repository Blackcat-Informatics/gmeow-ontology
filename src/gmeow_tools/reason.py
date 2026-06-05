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
)
from gmeow_tools.runner import run_container

#: Canonical merged (asserted) release product.
MERGED_FILE = DIST_DIR / "gmeow-merged.ttl"
#: Reasoned product carrying inferred axioms (release closure).
FULL_FILE = DIST_DIR / "gmeow-full.ttl"


def _rel(path: Path) -> str:
    """Return a container path (relative to the repo root mounted at /work)."""
    return str(path.relative_to(PROJECT_ROOT))


def _robot(args: list[str]) -> str:
    """Run a ROBOT command and return combined stdout+stderr."""
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    result = run_container(ROBOT_IMAGE, ["robot", *args])
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
    args = ["merge", "--catalog", _rel(CATALOG_FILE), "--input", _rel(ONTOLOGY_FILE)]
    if include_statements and STATEMENT_OWL_FILE.exists():
        args += ["--input", _rel(STATEMENT_OWL_FILE)]
    args += ["--collapse-import-closure", "true", "--output", _rel(output)]
    _robot(args)
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


def reason(reasoner: str = "ELK", *, merged: Path = MERGED_FILE) -> Path:
    """Run a reasoner over the merged ontology to check coherence.

    ROBOT exits non-zero if the ontology is inconsistent or has unsatisfiable
    classes, which surfaces as :class:`ToolExecutionError`.

    Args:
        reasoner: ``ELK`` (fast, EL) or ``hermit`` (sound+complete DL).
        merged: The merged ontology (produced if absent).

    Returns:
        Path to the reasoned output written under ``dist/``.
    """
    if not merged.exists():
        merge_release(merged)
    output = DIST_DIR / f"gmeow-reasoned-{reasoner.lower()}.ttl"
    _robot(
        [
            "reason",
            "--reasoner",
            reasoner,
            "--input",
            _rel(merged),
            "--output",
            _rel(output),
        ]
    )
    return output


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
        ]
    )
    return output
