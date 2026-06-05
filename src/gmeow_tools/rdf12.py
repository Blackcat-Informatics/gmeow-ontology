"""RDF 1.2 / RDF-star publication view (gated on Apache Jena).

GMEOW's model is **RDF 1.2 / RDF-star-first**: statement-level metadata is native
triple-term content. Because today's OWL 2 DL reasoners cannot yet consume RDF 1.2,
the working form GMEOW reasons over is the plain-RDF **compatibility encoding** (OWL
2 axiom annotations), and this module materializes the RDF 1.2 serialization from it
for graph/SPARQL consumers, using Apache Jena (the only mainstream engine with RDF
1.2 triple-term support as of 2026).

RDF 1.2 Turtle/N-Triples syntax is still finalizing at the W3C, so the step is
**gated**: if the pinned Jena image is absent it is skipped (not a release
blocker), raising :class:`~gmeow_tools.runner.ToolUnavailableError` for the caller
to handle.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import DIST_DIR, JENA_IMAGE, PROJECT_ROOT, QUERIES_DIR
from gmeow_tools.runner import run_container

#: SPARQL 1.2 CONSTRUCT that maps owl:Axiom reification → RDF 1.2 triple terms.
PROJECTION_QUERY = QUERIES_DIR / "rdf12-project.rq"
#: Header prepended to the emitted file (the RDF 1.2 syntax is still finalizing).
_RDF12_BANNER = (
    "# RDF 1.2 / RDF-star view — GMEOW's primary statement-level model, materialized\n"
    "# from the OWL 2 axiom-annotation compatibility encoding via Apache Jena. The\n"
    "# RDF 1.2 Turtle syntax is finalizing at the W3C and may still change.\n"
)


def project_rdf12(*, merged: Path, output: Path = DIST_DIR / "gmeow.rdf12.ttl") -> Path:
    """Materialize the RDF 1.2 / RDF-star view from the merged ontology.

    Apache Jena is a **required** tool: RDF 1.2 is GMEOW's primary statement-level
    model, not an optional add-on, so a missing Jena image is a hard failure (via
    :func:`~gmeow_tools.runner.run_container`), never a silent skip.

    Args:
        merged: The merged (asserted) ontology with OWL axiom annotations.
        output: Destination for the RDF 1.2 Turtle view.

    Returns:
        The path to the generated RDF 1.2 view.

    Raises:
        ToolUnavailableError: If Docker / the pinned Jena image is unavailable.
        FileNotFoundError: If inputs are missing.
    """
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
    output.write_text(_RDF12_BANNER + result.stdout, encoding="utf-8")
    return output
