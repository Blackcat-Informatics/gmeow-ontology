"""Documentation generation: pyLODE (fast, pure-Python) and WIDOCO (rich).

pyLODE produces clean conventional HTML for dev/CI and runs in-process. WIDOCO
adds WebVOWL diagrams, a changelog and an embedded OOPS! report, and runs as a
pinned Docker image (gated — skipped with a warning if the image is absent).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import DIST_DIR, DOCS_DIR, PROJECT_ROOT, WIDOCO_IMAGE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.runner import image_available, run_container


def _gmeow_only_source() -> Path:
    """Write a GMEOW-only graph (header + modules, no imports) for documentation.

    pyLODE documents every term in the input file; rendering the full gUFO
    import closure trips on gUFO's ``owl:unionOf`` class expressions and is
    redundant. Documenting GMEOW's own terms (which only *reference* gUFO by
    name) is both robust and the intended scope.

    Returns:
        Path to the GMEOW-only Turtle file under ``dist/``.
    """
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    out = DIST_DIR / "gmeow-vocab.ttl"
    load_merged_graph(include_imports=False).serialize(destination=out, format="turtle")
    return out


def pylode_html(source: Path | None = None, *, output: Path | None = None) -> Path:
    """Generate HTML documentation with pyLODE.

    Args:
        source: Ontology Turtle file to document. Defaults to a GMEOW-only graph
            (the imported gUFO axioms are out of scope and break pyLODE).
        output: Output HTML path (defaults to ``docs/_generated/index.html``).

    Returns:
        The path to the generated HTML.
    """
    src = source or _gmeow_only_source()
    out = output or (DOCS_DIR / "index.html")
    out.parent.mkdir(parents=True, exist_ok=True)
    # Imported lazily: pyLODE pulls a sizeable dependency tree.
    from pylode import OntPub

    doc = OntPub(ontology=str(src))
    doc.make_html(destination=str(out))
    return out


def widoco_available() -> bool:
    """Return whether the pinned WIDOCO image is present locally."""
    return image_available(WIDOCO_IMAGE)


def widoco_docs(source: Path, *, outdir: Path | None = None) -> Path:
    """Generate rich documentation with WIDOCO (Docker).

    Args:
        source: The ontology Turtle file to document.
        outdir: Output directory (defaults to ``docs/_generated/widoco``).

    Returns:
        The output directory.

    Raises:
        ToolUnavailableError: If Docker or the WIDOCO image is unavailable.
    """
    out = outdir or (DOCS_DIR / "widoco")
    out.mkdir(parents=True, exist_ok=True)
    run_container(
        WIDOCO_IMAGE,
        [
            "-ontFile",
            str(source.relative_to(PROJECT_ROOT)),
            "-outFolder",
            str(out.relative_to(PROJECT_ROOT)),
            "-rewriteAll",
            "-uniteSections",
            "-includeAnnotationProperties",
            "-noPlaceHolderText",
        ],
    )
    return out
