"""The SHACL validation seam: rdflib data → pyoxigraph ingestion → ``gmeow_shacl``.

This module is the single dependency-inversion boundary every pySHACL entry
point now calls (#578, part of EPIC #575 — replace pySHACL with the Rust
``gmeow_shacl`` validator built on oxigraph). Callers hand in an rdflib
``Graph`` (the contract they already satisfy); this seam serializes it to
N-Triples, hands it plus the SHACL shapes to ``gmeow_shacl.validate``, and
returns the structured report. pySHACL/rdflib stay only as the cross-check twin
(#578) until #579 deletes them from the validation path entirely.

Why no RDF-1.2 round-trip here: every source the merged graph is built from —
``slices/*/*/module.ttl``, the root ontology, ``imports/*.ttl`` — and every
slice example is plain-triple Turtle (the RDF-1.2 statement layer is projected
to its OWL axiom-annotation form *before* it reaches validation). rdflib's
N-Triples codec cannot even represent RDF-1.2 reifying triples, so quoted-triple
data does not transit this seam; the crate's own ``corpus/21`` conformance case
covers RDF-1.2 validation under ``cargo test``.
"""

from __future__ import annotations

from collections.abc import Iterable
from importlib import metadata
from pathlib import Path
from typing import TypedDict, cast

from rdflib import Graph


class ShaclResult(TypedDict):
    """One structured SHACL validation result from ``gmeow_shacl``."""

    focus: str
    path: str | None
    value: str | None
    severity: str
    component: str
    source_shape: str
    message: str | None


class ShaclReport(TypedDict):
    """The structured report ``gmeow_shacl.validate`` returns."""

    conforms: bool
    results: list[ShaclResult]


#: SHACL severity IRIs (gmeow_shacl returns severity as a full IRI string).
_SH = "http://www.w3.org/ns/shacl#"
SH_VIOLATION = _SH + "Violation"
SH_WARNING = _SH + "Warning"
SH_INFO = _SH + "Info"


def gmeow_shacl_version() -> str:
    """Return the installed ``gmeow_shacl`` version for cache salting (#578).

    Raises:
        RuntimeError: If the extension is not installed (hard-fail, never a
            silent fallback — the validation path requires it).
    """
    try:
        return metadata.version("gmeow-shacl")
    except metadata.PackageNotFoundError as exc:  # pragma: no cover - env error
        raise RuntimeError(
            "gmeow_shacl extension not installed — run `make shacl-py` "
            "(uvx maturin develop --manifest-path crates/shacl/Cargo.toml)"
        ) from exc


def graph_to_ntriples(data_graph: Graph) -> str:
    """Serialize an rdflib data graph to N-Triples for oxigraph ingestion."""
    return data_graph.serialize(format="nt")


def shapes_files_to_turtle(paths: Iterable[Path]) -> str:
    """Merge SHACL shape files into one Turtle document, preserving prefixes.

    This is the rdflib-free shapes-ingestion seam (#578): the shapes never touch
    rdflib on the production path. The merge is a raw-text concatenation — NOT a
    triple-store round-trip — because ``gmeow_shacl`` resolves the prefixed names
    inside ``sh:select`` SHACL-AF queries from the document's lexical ``@prefix``
    declarations. A store round-trip discards prefix maps (they are not part of
    the RDF data model), which would break every SPARQL constraint. Turtle
    permits repeated ``@prefix`` declarations, so each file keeps its own header for
    its own queries; an explicit ``base`` per segment is unnecessary because the
    shapes use absolute IRIs.
    """
    return "\n".join(path.read_text(encoding="utf-8") for path in paths)


def validate_graph(data_graph: Graph, shapes_ttl: str) -> ShaclReport:
    """Validate an rdflib data graph against SHACL shapes via ``gmeow_shacl``.

    Args:
        data_graph: The data graph to validate (merged ontology, example, …).
        shapes_ttl: The SHACL shapes graph, serialized as Turtle.

    Returns:
        The structured report ``{"conforms": bool, "results": [...]}`` where each
        result carries ``focus``/``path``/``value``/``severity``/``component``/
        ``source_shape``/``message``.

    Raises:
        ValueError: Propagated from ``gmeow_shacl`` on a parse/validate error.
            This MUST surface — a parse error silently mapped to ``conforms``
            would be a false-negative, the worst possible SHACL outcome (P11/§11).
    """
    import gmeow_shacl

    return cast(
        ShaclReport,
        gmeow_shacl.validate(
            shapes_ttl=shapes_ttl, data_nt=graph_to_ntriples(data_graph)
        ),
    )


def term_to_str(term: str | None) -> str:
    """Render a gmeow_shacl N-Triples term as rdflib's ``str(term)`` would.

    ``<http://x>`` → ``http://x``; ``_:b0`` → ``b0``; literals/plain pass through.
    Keeps the report lines byte-identical to the legacy pySHACL path.
    """
    if term is None:
        return "None"
    if term.startswith("<") and term.endswith(">"):
        return term[1:-1]
    if term.startswith("_:"):
        return term[2:]
    return term


def partition_results(results: list[ShaclResult]) -> tuple[list[str], list[str]]:
    """Split structured ``gmeow_shacl`` results into (violations, warnings).

    Mirrors the legacy ``_partition_shacl_results``: ``sh:Violation`` →
    error lines, ``sh:Warning``/``sh:Info`` → warning lines, each formatted
    ``"<focusNode>: <message>"`` (or just the focus node when no message).
    """
    violations: list[str] = []
    warnings: list[str] = []
    for r in results:
        focus = term_to_str(r.get("focus"))
        message = r.get("message")
        line = f"{focus}: {message}" if message is not None else focus
        if r.get("severity") in (SH_WARNING, SH_INFO):
            warnings.append(line)
        else:
            violations.append(line)
    return violations, warnings
