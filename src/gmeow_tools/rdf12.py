"""Derived RDF 1.2 / rdf-star publication view (PREVIEW, gated on Apache Jena).

The OWL 2 axiom-annotation form is canonical; this module projects it into an
RDF 1.2 reifying-triple serialization for graph/SPARQL consumers, using Apache
Jena (the only mainstream engine with RDF 1.2 triple-term support as of 2026).

RDF 1.2 Turtle/N-Triples syntax is still a W3C Working Draft, so this output is
explicitly experimental and the step is **gated**: if the pinned Jena image is
absent it is skipped (not a release blocker), raising
:class:`~gmeow_tools.runner.ToolUnavailableError` for the caller to handle.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import DIST_DIR, JENA_IMAGE, PROJECT_ROOT, QUERIES_DIR
from gmeow_tools.runner import (
    ToolUnavailableError,
    image_available,
    run_container,
)

#: SPARQL 1.2 CONSTRUCT that maps owl:Axiom reification → RDF 1.2 triple terms.
PROJECTION_QUERY = QUERIES_DIR / "rdf12-project.rq"
#: Header prepended to the emitted file (the syntax is non-final).
_PREVIEW_BANNER = (
    "# PREVIEW: RDF 1.2 / rdf-star view derived from the canonical OWL axiom\n"
    "# annotations via Apache Jena. RDF 1.2 Turtle syntax is a W3C Working Draft\n"
    "# and may change; do not treat this artifact as stable.\n"
)


def jena_available() -> bool:
    """Return whether the pinned Apache Jena image is present locally."""
    return image_available(JENA_IMAGE)


def project_rdf12(*, merged: Path, output: Path = DIST_DIR / "gmeow.rdf12.ttl") -> Path:
    """Project the merged ontology into the RDF 1.2 preview serialization.

    Args:
        merged: The merged (asserted) ontology with OWL axiom annotations.
        output: Destination for the RDF 1.2 Turtle view.

    Returns:
        The path to the generated RDF 1.2 view.

    Raises:
        ToolUnavailableError: If the Jena image is unavailable (caller skips).
        FileNotFoundError: If inputs are missing.
    """
    if not jena_available():
        raise ToolUnavailableError(
            f"Apache Jena image not present ({JENA_IMAGE}); "
            "skipping the experimental RDF 1.2 view"
        )
    if not merged.exists():
        raise FileNotFoundError(f"merged ontology not found: {merged}")
    if not PROJECTION_QUERY.exists():
        raise FileNotFoundError(f"projection query not found: {PROJECTION_QUERY}")

    output.parent.mkdir(parents=True, exist_ok=True)
    result = run_container(
        JENA_IMAGE,
        [
            "sparql",
            "--data",
            str(merged.relative_to(PROJECT_ROOT)),
            "--query",
            str(PROJECTION_QUERY.relative_to(PROJECT_ROOT)),
            "--results",
            "ttl",
        ],
    )
    output.write_text(_PREVIEW_BANNER + result.stdout, encoding="utf-8")
    return output
