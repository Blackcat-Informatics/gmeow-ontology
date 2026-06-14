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
    html = out.read_text(encoding="utf-8")
    if "</body>" not in html:
        msg = (
            "pyLODE output has no </body> tag — cannot inject the Profiles "
            "section (#330); the landing page would silently lose its "
            "composition listing. Inspect the pylode_html output step."
        )
        raise RuntimeError(msg)
    out.write_text(
        html.replace("</body>", profiles_section_html() + "\n</body>", 1),
        encoding="utf-8",
    )
    return out


def profiles_section_html() -> str:
    """The landing page's Profiles section (#330's recorded sub-decision).

    The human page stays unified — conneg splits machines from humans —
    and this section explains the composition: the root IRI is the core
    profile, ``full`` aggregates everything, and each named profile is a
    slim dependency-closed import set.
    """
    from gmeow_tools.config import FULL_PROFILE_IRI, NAMED_PROFILE_NS, ONTOLOGY_IRI
    from gmeow_tools.profiles_gen import dependency_closure, group_named_profiles
    from gmeow_tools.slices import discover_slices

    slices = discover_slices()
    core_n = sum(1 for s in slices.values() if s.is_core)
    rows = [
        (
            ONTOLOGY_IRI,
            "core",
            f"the root IRI is the core profile — {core_n} tierCore slices",
        ),
        (
            FULL_PROFILE_IRI,
            "full",
            f"everything: core plus {len(slices) - core_n} extension slices",
        ),
    ]
    for name, members in group_named_profiles(slices).items():
        closure = dependency_closure(members, slices)
        rows.append(
            (
                NAMED_PROFILE_NS + name,
                name,
                f"{len(members)} declared slice(s), {len(closure)} in the "
                f"dependency-closed import set",
            )
        )
    items = "\n".join(
        f'<li><a href="{iri}"><code>{iri}</code></a> — '
        f"<strong>{name}</strong>: {desc}</li>"
        for iri, name, desc in rows
    )
    return (
        '<section id="profiles">\n<h2>Profiles</h2>\n'
        "<p>Composition lives in profile IRIs (#330): each profile is a "
        "generated <code>owl:imports</code> aggregation — dereferenceable "
        "via content negotiation, citable, and reasonable on its own. "
        "Named profiles are slim: declared members plus their dependency "
        "closure, never the whole core.</p>\n"
        f"<ul>\n{items}\n</ul>\n</section>"
    )


def _container_path(path: Path) -> str:
    """Return the ``/work/...`` path seen by Docker for a host *path*.

    Relative paths are resolved against the current working directory; the
    result must live under ``PROJECT_ROOT`` because that is what the
    container mounts at ``/work``.
    """
    abs_path = path.resolve()
    return f"/work/{abs_path.relative_to(PROJECT_ROOT).as_posix()}"


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
            _container_path(source),
            "-outFolder",
            _container_path(out),
            "-rewriteAll",
            "-uniteSections",
            "-includeAnnotationProperties",
            "-noPlaceHolderText",
        ],
        image_workdir=True,
    )
    return out
