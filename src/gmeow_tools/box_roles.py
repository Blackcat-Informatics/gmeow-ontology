"""Audit explicit graph-box role coverage in authored GMEOW sources."""

from __future__ import annotations

import json
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    SLICE_VOCABULARY_FILE,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import bind_prefixes, iter_source_files

_GRAPH_BOX_ROLE = URIRef(NAMESPACE + "graphBoxRole")
_GRAPH_BOX_ROLE_CLASS = URIRef(NAMESPACE + "GraphBoxRole")
_KNOWN_TYPES = (
    OWL.Ontology,
    OWL.Class,
    OWL.ObjectProperty,
    OWL.DatatypeProperty,
    OWL.AnnotationProperty,
    RDFS.Datatype,
)
_KIND_ORDER = (
    "ontology",
    "class",
    "annotation property",
    "property",
    "datatype",
    "individual",
)


@dataclass(frozen=True, slots=True)
class RoleFinding:
    """One box-role audit finding."""

    term: str
    kind: str
    source: str
    message: str


@dataclass(slots=True)
class BoxRoleAudit:
    """Graph-box role coverage report."""

    term_count: int
    role_counts: Counter[str] = field(default_factory=Counter)
    missing: list[RoleFinding] = field(default_factory=list)
    invalid: list[RoleFinding] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        """Whether the audit found complete, valid role coverage."""
        return not self.missing and not self.invalid

    def as_dict(self) -> dict[str, object]:
        """Return a stable JSON-serializable report."""
        return {
            "ok": self.ok,
            "termCount": self.term_count,
            "roleCounts": dict(sorted(self.role_counts.items())),
            "missing": [_finding_dict(finding) for finding in self.missing],
            "invalid": [_finding_dict(finding) for finding in self.invalid],
        }


def audit_box_roles(paths: list[Path] | None = None) -> BoxRoleAudit:
    """Audit explicit ``gmeow:graphBoxRole`` coverage for typed GMEOW terms."""
    source_paths = default_audit_paths() if paths is None else paths
    graph = Graph()
    bind_prefixes(graph)
    source_by_term: dict[URIRef, Path] = {}
    types_by_term: dict[URIRef, set[URIRef]] = {}
    for path in source_paths:
        local = Graph()
        bind_prefixes(local)
        local.parse(path, format="turtle")
        for term, _, rdf_type in local.triples((None, RDF.type, None)):
            if not isinstance(term, URIRef) or not isinstance(rdf_type, URIRef):
                continue
            if not _is_gmeow_term(term):
                continue
            source_by_term.setdefault(term, path)
            types_by_term.setdefault(term, set()).add(rdf_type)
        graph += local

    terms = {
        term: _term_kind(types)
        for term, types in sorted(types_by_term.items(), key=lambda item: str(item[0]))
    }
    report = BoxRoleAudit(term_count=len(terms))
    for term, kind in terms.items():
        source = _source(source_by_term[term])
        roles = list(graph.objects(term, _GRAPH_BOX_ROLE))
        if not roles:
            report.missing.append(
                RoleFinding(str(term), kind, source, "missing gmeow:graphBoxRole")
            )
            continue
        for role in roles:
            if not isinstance(role, URIRef):
                report.invalid.append(
                    RoleFinding(
                        str(term),
                        kind,
                        source,
                        f"non-IRI gmeow:graphBoxRole value {role.n3()}",
                    )
                )
                continue
            if (role, RDF.type, _GRAPH_BOX_ROLE_CLASS) not in graph:
                report.invalid.append(
                    RoleFinding(
                        str(term),
                        kind,
                        source,
                        f"{role} is not typed gmeow:GraphBoxRole",
                    )
                )
                continue
            report.role_counts[_curie(role)] += 1
    return report


def default_audit_paths() -> list[Path]:
    """Authored term sources covered by the default repo-only audit."""
    paths = list(iter_source_files())
    if SLICE_VOCABULARY_FILE.exists():
        paths.append(SLICE_VOCABULARY_FILE)
    for dsl_dir in (MAPPING_DSL_DIR, STATEMENT_DSL_DIR):
        path = dsl_dir / "vocabulary.ttl"
        if path.exists():
            paths.append(path)
    return paths


def render_text(report: BoxRoleAudit) -> str:
    """Render a concise human-facing audit report."""
    lines = [
        f"Typed GMEOW terms: {report.term_count}",
        "Role distribution:",
    ]
    if report.role_counts:
        for role, count in sorted(report.role_counts.items()):
            lines.append(f"  {role}: {count}")
    else:
        lines.append("  none")
    if report.missing:
        lines.append("")
        lines.append(f"Missing roles ({len(report.missing)}):")
        lines.extend(_finding_lines(report.missing))
    if report.invalid:
        lines.append("")
        lines.append(f"Invalid roles ({len(report.invalid)}):")
        lines.extend(_finding_lines(report.invalid))
    if report.ok:
        lines.append("")
        lines.append("All typed GMEOW terms have explicit typed graph-box roles.")
    return "\n".join(lines)


def render_json(report: BoxRoleAudit) -> str:
    """Render the audit as stable JSON."""
    return json.dumps(report.as_dict(), indent=2, sort_keys=True)


def _finding_lines(findings: list[RoleFinding], *, limit: int = 50) -> list[str]:
    lines = [
        f"  {_curie(URIRef(f.term))} ({f.kind}, {f.source}): {f.message}"
        for f in findings[:limit]
    ]
    if len(findings) > limit:
        lines.append(f"  ... {len(findings) - limit} more")
    return lines


def _finding_dict(finding: RoleFinding) -> dict[str, str]:
    return {
        "term": finding.term,
        "kind": finding.kind,
        "source": finding.source,
        "message": finding.message,
    }


def _is_gmeow_term(term: URIRef) -> bool:
    iri = str(term)
    return iri == ONTOLOGY_IRI or iri.startswith(NAMESPACE)


def _term_kind(types: set[URIRef]) -> str:
    if OWL.Ontology in types:
        return "ontology"
    if OWL.Class in types:
        return "class"
    if OWL.AnnotationProperty in types:
        return "annotation property"
    if OWL.ObjectProperty in types or OWL.DatatypeProperty in types:
        return "property"
    if RDFS.Datatype in types:
        return "datatype"
    return _KIND_ORDER[-1]


def _curie(term: URIRef) -> str:
    iri = str(term)
    if iri == ONTOLOGY_IRI:
        return "gmeow:"
    if iri.startswith(NAMESPACE):
        return "gmeow:" + iri.removeprefix(NAMESPACE)
    return iri


def _source(path: Path) -> str:
    try:
        return str(path.relative_to(Path.cwd()))
    except ValueError:
        return str(path)
