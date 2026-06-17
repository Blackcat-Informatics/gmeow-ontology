"""Dual-run SHACL cross-check: pySHACL vs gmeow_shacl (#578, EPIC #575).

The trust anchor that licenses the production swap. Every SHACL validation unit
— the merged ontology plus each slice example — is validated under BOTH the
legacy pySHACL engine and the new ``gmeow_shacl`` (Rust/oxigraph) engine, and
their result sets are compared. This gate is **report-only** for now: it writes
the divergence ledger and prints divergences but never fails. #579 makes it
blocking, then removes pySHACL (and this module) from the tree.

Comparison key (the #578 design decision — "component, IRI-shape only"):
``(focusNode, severity, constraintComponent)``, plus ``resultPath`` when present
(the property-shape discriminator), plus ``sourceShape`` only when it is a named
IRI **and** there is no path. ``sourceShape`` is otherwise excluded because the
engines represent it incompatibly — ``gmeow_shacl`` reports the parent
*node*-shape IRI for property constraints while pySHACL reports the (blank)
*property*-shape node — which is a representational difference, not a validation
divergence. Message **text** is never compared: it is Python-side formatting.

Examples are validated standalone (not merged with the ~37k-triple ontology):
the cross-check proves the two engines agree on *identical input*, and pySHACL
over the merged ontology per example would cost tens of minutes. The
merged-ontology unit is the primary parity anchor (it is where real warnings
arise).
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from rdflib import RDF, Graph
from rdflib.namespace import SH
from rdflib.term import Node, URIRef

from gmeow_tools import shacl_engine
from gmeow_tools.config import DIST_DIR, SHAPES_FILE, SLICES_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.slices import iter_slice_example_files
from gmeow_tools.validate import _shapes_turtle

#: The divergence ledger artifact (#575 ledger). Deterministic — no timestamp —
#: so it round-trips cleanly through any dist drift gate.
LEDGER_FILE = DIST_DIR / "shacl-divergence-ledger.ttl"

_XCK = "https://blackcatinformatics.ca/gmeow/shacl-crosscheck#"

#: A normalized comparison key: a tuple of strings (focus, severity, component)
#: optionally extended with a ("path"|"shape", iri) discriminator pair.
ResultKey = tuple[str, ...]


@dataclass(frozen=True, slots=True)
class Divergence:
    """One (focus, severity, component[, path|shape]) key seen by exactly one engine."""

    unit: str  # "merged" or the slice-example path
    side: str  # "only-pyshacl" | "only-gmeow_shacl"
    key: str  # the comparison key, rendered
    reason: str  # an explained cause — never a blind catalog entry


def _norm_iri(value: str | None) -> str | None:
    """Normalize an engine's term rendering to a bare IRI, or ``None`` if not one.

    ``gmeow_shacl`` emits ``<iri>``/``_:bnode``; pySHACL terms arrive as bare
    ``str(term)``. Only named IRIs are identity-comparable across engines.
    """
    if value is None:
        return None
    if value.startswith("<") and value.endswith(">"):  # gmeow_shacl IRI
        return value[1:-1]
    if value.startswith("_:"):  # gmeow_shacl blank node — not cross-comparable
        return None
    # A bare pySHACL term. Match on the IRI scheme rather than a loose ``":" in
    # value`` test, so a colon-bearing literal (datetime, message, urn-like text)
    # is never mis-keyed as an IRI → no false divergence. GMEOW focus nodes,
    # shapes, paths, and components are all http(s)/urn IRIs.
    if value.startswith(("http://", "https://", "urn:")):
        return value
    return None  # pySHACL blank node id / literal


def _build_key(
    focus: str | None,
    severity: str | None,
    component: str | None,
    path: str | None,
    source_shape: str | None,
) -> ResultKey:
    """Assemble the cross-engine comparison key from normalized term strings."""
    base: ResultKey = (
        _norm_iri(focus) or "?",
        _norm_iri(severity) or "?",
        _norm_iri(component) or "?",
    )
    path_iri = _norm_iri(path)
    if path_iri is not None:
        return (*base, "path", path_iri)
    shape_iri = _norm_iri(source_shape)
    if shape_iri is not None:
        return (*base, "shape", shape_iri)
    return base


def _gmeow_keys(data: Graph, shapes_ttl: str) -> set[ResultKey]:
    """The gmeow_shacl result key-set for one validation unit."""
    # The crosscheck keeps its rdflib graph (it is the pySHACL twin); the seam is
    # N-Triples now (#579), so serialize here before handing it to gmeow_shacl.
    data_nt = data.serialize(format="nt", encoding="utf-8").decode("utf-8")
    report = shacl_engine.validate_nt(data_nt, shapes_ttl)
    return {
        _build_key(
            r.get("focus"),
            r.get("severity"),
            r.get("component"),
            r.get("path"),
            r.get("source_shape"),
        )
        for r in report["results"]
    }


def _pyshacl_keys(data: Graph, shapes_graph: Graph) -> set[ResultKey]:
    """The pySHACL result key-set for one validation unit."""
    from pyshacl import validate as shacl_validate

    _conforms, report, _text = shacl_validate(
        data,
        shacl_graph=shapes_graph,
        advanced=True,
        inference="none",
        abort_on_first=False,
        meta_shacl=False,
    )
    keys: set[ResultKey] = set()
    for node in report.subjects(RDF.type, SH.ValidationResult):
        keys.add(
            _build_key(
                _str_or_none(report.value(node, SH.focusNode)),
                _str_or_none(report.value(node, SH.resultSeverity)),
                _str_or_none(report.value(node, SH.sourceConstraintComponent)),
                _str_or_none(report.value(node, SH.resultPath)),
                _str_or_none(report.value(node, SH.sourceShape)),
            )
        )
    return keys


def _str_or_none(term: Node | None) -> str | None:
    return str(term) if term is not None else None


def _diff_unit(
    unit: str, gmeow: set[ResultKey], pyshacl: set[ResultKey]
) -> list[Divergence]:
    """Diff two engines' key-sets for one unit into explained divergences."""
    divergences: list[Divergence] = []
    for key in sorted(pyshacl - gmeow):
        divergences.append(
            Divergence(unit, "only-pyshacl", " | ".join(key), _explain(key))
        )
    for key in sorted(gmeow - pyshacl):
        divergences.append(
            Divergence(unit, "only-gmeow_shacl", " | ".join(key), _explain(key))
        )
    return divergences


def _explain(key: ResultKey) -> str:
    """Best-effort explanation for a divergence so the ledger is never blind."""
    if "?" in key:
        return "blank-node or non-IRI term not identity-comparable across engines"
    return "engine result-set difference — review before #579 makes the gate blocking"


def crosscheck_all() -> list[Divergence]:
    """Run both engines over the merged ontology + every slice example.

    Returns every observed divergence (empty == the engines agree everywhere).
    """
    shapes_ttl = _shapes_turtle(SHAPES_FILE)
    # One rdflib parse of the same merged shape text feeds pySHACL — single
    # source of truth for the shape set both engines validate against.
    shapes_graph = Graph().parse(data=shapes_ttl, format="turtle")

    divergences: list[Divergence] = []

    merged = load_merged_graph(include_imports=True)
    divergences += _diff_unit(
        "merged",
        _gmeow_keys(merged, shapes_ttl),
        _pyshacl_keys(merged, shapes_graph),
    )

    for example in iter_slice_example_files():
        name = example.relative_to(SLICES_DIR).as_posix()
        data = Graph()
        try:
            data.parse(example, format="turtle")
        except Exception as exc:  # a bad example is itself a finding
            divergences.append(
                Divergence(name, "parse-error", "", f"example does not parse: {exc}")
            )
            continue
        divergences += _diff_unit(
            name,
            _gmeow_keys(data, shapes_ttl),
            _pyshacl_keys(data, shapes_graph),
        )

    return divergences


def write_ledger(divergences: list[Divergence], path: Path = LEDGER_FILE) -> Path:
    """Write the divergence ledger as deterministic Turtle (#575 ledger)."""
    lines = [
        "# SHACL dual-run divergence ledger (#578, EPIC #575).",
        "# Generated by `gmeow-dev shacl-crosscheck`. Each entry is a (focusNode,",
        "# severity, constraintComponent[, path|shape]) key seen by exactly one",
        "# engine, with an explained reason. An EMPTY ledger (no xck:Divergence) is",
        "# the goal — it is what licenses #579 to make the gate blocking and remove",
        "# pySHACL. DO NOT hand-edit; regenerate via the cross-check.",
        f"@prefix xck: <{_XCK}> .",
        "",
    ]
    ordered = sorted(divergences, key=lambda d: (d.unit, d.side, d.key))
    if not ordered:
        lines.append("# No divergences: pySHACL ≡ gmeow_shacl across all units.")
    for i, d in enumerate(ordered):
        lines += [
            f"xck:divergence-{i:04d} a xck:Divergence ;",
            f'    xck:unit "{_escape(d.unit)}" ;',
            f'    xck:side "{d.side}" ;',
            f'    xck:key "{_escape(d.key)}" ;',
            f'    xck:reason "{_escape(d.reason)}" .',
            "",
        ]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines).rstrip("\n") + "\n", encoding="utf-8")
    return path


def _escape(text: str) -> str:
    """Escape a string for a single-line Turtle literal.

    Control characters must be escaped too — the ``reason`` field can carry
    exception text (the parse-error path interpolates ``{exc}``), and a raw
    newline/tab would produce invalid Turtle in the ledger artifact.
    """
    return (
        text.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


# A namespace handle kept importable for tests/tools that introspect the ledger.
XCK = URIRef(_XCK)
