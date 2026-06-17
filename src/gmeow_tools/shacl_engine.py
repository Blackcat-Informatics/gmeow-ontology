"""The SHACL validation seam: N-Triples data → ``gmeow_shacl``.

This module is the single dependency-inversion boundary every SHACL entry point
on the validation path now calls (#578/#579, part of EPIC #575 — replace the
legacy Python SHACL engine with the Rust ``gmeow_shacl`` validator built on
oxigraph). Callers hand in the data graph **already serialized to N-Triples**
(the merged ontology, an example, the DSL graph — produced by
``gmeow_validate.merge_to_ntriples`` in Rust, never from a Python graph object);
this seam hands that plus the SHACL shapes to ``gmeow_shacl.validate`` and returns
the structured report. As of #579 this module imports no graph library at all:
the validation path is graph-free end to end.

Why no RDF-1.2 round-trip here: every source the merged graph is built from —
``slices/*/*/module.ttl``, the root ontology, ``imports/*.ttl`` — and every
slice example is plain-triple Turtle (the RDF-1.2 statement layer is projected
to its OWL axiom-annotation form *before* it reaches validation). oxigraph's
N-Triples codec carries the data losslessly; the crate's own ``corpus/21``
conformance case covers RDF-1.2 validation under ``cargo test``.
"""

from __future__ import annotations

from collections.abc import Iterable
from importlib import metadata
from pathlib import Path
from typing import NotRequired, TypedDict, cast


class ShaclResult(TypedDict):
    """One structured SHACL validation result from ``gmeow_shacl``."""

    focus: str
    path: str | None
    value: str | None
    severity: str
    component: str
    source_shape: str
    message: str | None
    source_box_roles: NotRequired[list[str]]
    path_box_roles: NotRequired[list[str]]
    result_box_roles: NotRequired[list[str]]


class ShaclReport(TypedDict):
    """The structured report ``gmeow_shacl.validate`` returns."""

    conforms: bool
    results: list[ShaclResult]


#: SHACL severity IRIs (gmeow_shacl returns severity as a full IRI string).
_SH = "http://www.w3.org/ns/shacl#"
SH_VIOLATION = _SH + "Violation"
SH_WARNING = _SH + "Warning"
SH_INFO = _SH + "Info"

_GMEOW = "https://blackcatinformatics.ca/gmeow/"
_BOX_LABELS = {
    _GMEOW + "boxABox": "ABox",
    _GMEOW + "boxTBox": "TBox",
    _GMEOW + "boxRBox": "RBox",
    _GMEOW + "boxCBox": "CBox",
}


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


def shapes_files_to_turtle(paths: Iterable[Path]) -> str:
    """Merge SHACL shape files into one Turtle document, preserving prefixes.

    This is the graph-free shapes-ingestion seam (#578): the shapes never touch a
    graph library on the production path. The merge is a raw-text concatenation — NOT a
    triple-store round-trip — because ``gmeow_shacl`` resolves the prefixed names
    inside ``sh:select`` SHACL-AF queries from the document's lexical ``@prefix``
    declarations. A store round-trip discards prefix maps (they are not part of
    the RDF data model), which would break every SPARQL constraint. Turtle
    permits repeated ``@prefix`` declarations, so each file keeps its own header for
    its own queries; an explicit ``base`` per segment is unnecessary because the
    shapes use absolute IRIs.
    """
    return "\n".join(path.read_text(encoding="utf-8") for path in paths)


def validate_nt(data_nt: str, shapes_ttl: str) -> ShaclReport:
    """Validate an N-Triples data graph against SHACL shapes via ``gmeow_shacl``.

    Args:
        data_nt: The data graph to validate (merged ontology, example, DSL graph),
            already serialized to N-Triples by ``gmeow_validate.merge_to_ntriples``
            (Rust/oxigraph — graph-free on the validation path, #579).
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
        gmeow_shacl.validate(shapes_ttl=shapes_ttl, data_nt=data_nt),
    )


def term_to_str(term: str | None) -> str:
    """Render a gmeow_shacl N-Triples term as a bare ``str(term)`` would.

    ``<http://x>`` → ``http://x``; ``_:b0`` → ``b0``; literals/plain pass through.
    Keeps the report lines byte-identical to the legacy validation path.
    """
    if term is None:
        return "None"
    if term.startswith("<") and term.endswith(">"):
        return term[1:-1]
    if term.startswith("_:"):
        return term[2:]
    return term


def _role_prefix(result: ShaclResult) -> str:
    roles = result.get("result_box_roles") or []
    labels = sorted({_BOX_LABELS.get(role, role.rsplit("/", 1)[-1]) for role in roles})
    return f"[{'/'.join(labels)}] " if labels else ""


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
        prefix = _role_prefix(r)
        line = (
            f"{prefix}{focus}: {message}" if message is not None else f"{prefix}{focus}"
        )
        if r.get("severity") in (SH_WARNING, SH_INFO):
            warnings.append(line)
        else:
            violations.append(line)
    return violations, warnings
