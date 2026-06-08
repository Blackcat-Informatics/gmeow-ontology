"""MCP server exposing GMEOW toolchain actions to AI agents.

The server is a thin interface layer over existing ``gmeow_tools`` modules.
No ontology logic lives here — it delegates to ``validate``, ``mapping_compile``,
``statement_compile``, ``reason``, ``export``, and ``graph``.

Principle 4 (One canonical source): this module only exposes; it never authors.
Principle 12 (Compute outside the logic): the MCP boundary is a solver/tooling layer.
"""

from __future__ import annotations

import json
from typing import Any

from fastmcp import FastMCP
from rdflib import OWL, RDF, RDFS, SKOS, URIRef

from gmeow_tools.config import NAMESPACE, PREFIXES, PROJECT_ROOT
from gmeow_tools.export import collect_terms
from gmeow_tools.graph import load_merged_graph

mcp = FastMCP("gmeow")


def _expand_curie(curie_str: str) -> str:
    """Expand a CURIE to a full IRI using the prefix registry.

    Args:
        curie_str: A CURIE like ``gmeow:Person`` or a raw IRI.

    Returns:
        The expanded IRI, or the input unchanged if no prefix matches.
    """
    if "://" in curie_str:
        return curie_str
    if ":" not in curie_str:
        return NAMESPACE + curie_str
    prefix, local = curie_str.split(":", 1)
    ns = PREFIXES.get(prefix)
    if ns is not None:
        return ns + local
    return curie_str


def _lookup_term(curie_str: str) -> dict[str, Any] | None:
    """Resolve a CURIE to its term metadata from the merged ontology graph.

    Args:
        curie_str: A CURIE like ``gmeow:Person``.

    Returns:
        A dict with ``iri``, ``label``, ``definition``, ``category``,
        ``parents``, ``domain``, ``range``, ``functional``, ``alignments``,
        or ``None`` if the term is not found.
    """
    iri = _expand_curie(curie_str)
    graph = load_merged_graph(include_imports=False)
    term = URIRef(iri)

    # Determine category and collect metadata
    if (term, RDF.type, OWL.Class) in graph:
        category = "class"
    elif (
        (term, RDF.type, OWL.ObjectProperty) in graph
        or (term, RDF.type, OWL.DatatypeProperty) in graph
        or (term, RDF.type, OWL.AnnotationProperty) in graph
    ):
        category = "property"
    elif any(
        (term, RDF.type, cls) in graph for cls in graph.subjects(RDF.type, OWL.Class)
    ):
        category = "individual"
    else:
        return None

    label = graph.value(term, RDFS.label)
    definition = graph.value(term, SKOS.definition)

    result: dict[str, Any] = {
        "iri": iri,
        "curie": curie_str,
        "label": str(label) if label else "",
        "definition": str(definition) if definition else "",
        "category": category,
    }

    if category == "class":
        parents = sorted(
            {
                _compact(str(o))
                for o in graph.objects(term, RDFS.subClassOf)
                if isinstance(o, URIRef)
            }
        )
        result["parents"] = parents
    elif category == "property":
        domain_val = graph.value(term, RDFS.domain)
        range_val = graph.value(term, RDFS.range)
        prop_kind = ""
        if (term, RDF.type, OWL.ObjectProperty) in graph:
            prop_kind = "object"
        elif (term, RDF.type, OWL.DatatypeProperty) in graph:
            prop_kind = "datatype"
        elif (term, RDF.type, OWL.AnnotationProperty) in graph:
            prop_kind = "annotation"
        result["propertyKind"] = prop_kind
        result["domain"] = _describe_node(graph, domain_val) if domain_val else ""
        result["range"] = _describe_node(graph, range_val) if range_val else ""
        result["functional"] = (term, RDF.type, OWL.FunctionalProperty) in graph
        result["subPropertyOf"] = sorted(
            {
                _compact(str(o))
                for o in graph.objects(term, RDFS.subPropertyOf)
                if isinstance(o, URIRef)
            }
        )
    else:
        types = sorted(
            {
                _compact(str(t))
                for t in graph.objects(term, RDF.type)
                if isinstance(t, URIRef) and str(t).startswith(NAMESPACE) and t != term
            }
        )
        result["types"] = types

    # Alignments from export module
    from gmeow_tools.mappings import build_alignment_graph, load_mappings

    alignments_graph = build_alignment_graph(load_mappings())
    aligns: list[str] = []
    for predicate, obj in alignments_graph.predicate_objects(term):
        tag = _ALIGN_TAGS.get(str(predicate), _compact(str(predicate)))
        aligns.append(f"{tag}={_compact(str(obj))}")
    result["alignments"] = sorted(aligns)

    return result


def _compact(iri: str) -> str:
    """Compact an IRI to a CURIE using the prefix registry."""
    best_prefix = ""
    best_ns = ""
    for prefix, namespace in PREFIXES.items():
        if iri.startswith(namespace) and len(namespace) > len(best_ns):
            best_prefix, best_ns = prefix, namespace
    if best_ns:
        return f"{best_prefix}:{iri[len(best_ns) :]}"
    return iri


_ALIGN_TAGS: dict[str, str] = {
    str(OWL.equivalentClass): "equivalentClass",
    str(OWL.equivalentProperty): "equivalentProperty",
    str(RDFS.subClassOf): "subClassOf",
    str(RDFS.subPropertyOf): "subPropertyOf",
    str(SKOS.closeMatch): "closeMatch",
    str(SKOS.exactMatch): "exactMatch",
    str(SKOS.relatedMatch): "relatedMatch",
}


def _describe_node(graph: Any, node: Any) -> str:
    """Return a CURIE or description of a class expression node."""
    from rdflib import BNode

    if isinstance(node, URIRef):
        return _compact(str(node))
    if isinstance(node, BNode):
        union_list = graph.value(node, OWL.unionOf)
        if union_list:
            elements = []
            curr = union_list
            while curr and curr != RDF.nil:
                first = graph.value(curr, RDF.first)
                if first:
                    elements.append(_describe_node(graph, first))
                curr = graph.value(curr, RDF.rest)
            return " | ".join(elements)
        intersection_list = graph.value(node, OWL.intersectionOf)
        if intersection_list:
            elements = []
            curr = intersection_list
            while curr and curr != RDF.nil:
                first = graph.value(curr, RDF.first)
                if first:
                    elements.append(_describe_node(graph, first))
                curr = graph.value(curr, RDF.rest)
            return " & ".join(elements)
    return str(node)


@mcp.tool()
def gmeow_validate() -> str:
    """Validate Turtle syntax, term annotations, and SHACL conformance."""
    from gmeow_tools.validate import validate_all

    try:
        result = validate_all()
    except Exception as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps(
        {
            "ok": result.ok,
            "errors": result.errors,
            "warnings": result.warnings,
        }
    )


@mcp.tool()
def gmeow_compile_mappings(check: bool = False) -> str:
    """Compile the mapping DSL to SSSOM/EDOAL/FnO and check for drift.

    Args:
        check: If True, verify committed artifacts match a fresh compile
            and write nothing.
    """
    from gmeow_tools.mapping_compile import compile_all
    from gmeow_tools.mapping_dsl import CompileError

    try:
        report = compile_all(check=check)
    except CompileError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    except Exception as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps(
        {
            "ok": True,
            "written": [str(p) for p in report.written],
            "drifted": [str(p) for p in report.drifted],
        }
    )


@mcp.tool()
def gmeow_compile_statements(check: bool = False) -> str:
    """Compile statement DSL to RDF 1.2 and OWL downcasts and check for drift.

    Args:
        check: If True, verify committed artifacts match a fresh compile
            and write nothing.
    """
    from gmeow_tools.mapping_dsl import CompileError
    from gmeow_tools.statement_compile import compile_statements as run

    try:
        report = run(check=check)
    except CompileError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    except Exception as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps(
        {
            "ok": True,
            "written": [str(p) for p in report.written],
            "drifted": [str(p) for p in report.drifted],
        }
    )


@mcp.tool()
def gmeow_reason(reasoner: str = "ELK", profile: str = "DL") -> str:
    """Run ELK/HermiT consistency check over the merged ontology.

    Args:
        reasoner: Reasoner to use — ``ELK`` (fast) or ``hermit`` (sound+complete).
        profile: OWL 2 profile to validate against — ``DL``, ``EL``, ``QL``,
            ``RL``, or ``Full``.
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    try:
        reasoning.merge_release()
        reasoning.validate_profile(profile)
        reasoning.reason(reasoner)
    except ToolUnavailableError as exc:
        return json.dumps({"ok": False, "error": f"Tool unavailable: {exc}"})
    except ToolExecutionError as exc:
        return json.dumps({"ok": False, "error": f"Reasoning failed: {exc.output}"})
    except Exception as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps({"ok": True, "message": f"{reasoner} consistency check passed"})


@mcp.tool()
def gmeow_lookup_term(curie: str) -> str:
    """Resolve a term CURIE to its definition and metadata.

    Args:
        curie: A CURIE such as ``gmeow:Person``.

    Returns:
        JSON with ``iri``, ``label``, ``definition``, ``category``,
        ``parents``, ``domain``, ``range``, ``alignments``, etc.
    """
    result = _lookup_term(curie)
    if result is None:
        return json.dumps({"ok": False, "error": f"Term not found: {curie}"})
    result["ok"] = True
    return json.dumps(result)


@mcp.resource("gmeow://ontology/llms.txt")
def gmeow_llms_txt() -> str:
    """Dynamically expose the flat vocabulary index (llms.txt)."""
    terms = collect_terms()
    # write_llms_txt expects a directory; render in-memory instead
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]
    from gmeow_tools.config import NAMESPACE
    from gmeow_tools.self_desc import load_self_description

    meta = load_self_description()
    lines = [
        f"# {meta.title}",
        "",
        "> A reasoning-centric, OWL 2 DL, gUFO-grounded super-vocabulary that "
        "unifies a person's or organization's digital existence (entities, "
        "contacts, email, trust/keys, time) and aligns it to schema.org, FOAF, "
        "PROV, the WOT schema, Wikidata, and more.",
        "",
        f"Vocabulary {meta.version}. Namespace: {NAMESPACE}. Each term below is "
        "`curie` — definition; the OWL source is canonical.",
        "",
        "## Classes",
        "",
    ]
    for t in classes:
        sub = f" (⊑ {', '.join(t.parents)})" if t.parents else ""
        lines.append(f"- {t.curie}{sub}: {t.definition or t.label}")
    lines += ["", "## Properties", ""]
    for t in properties:
        sig = (
            f" [{t.domain or '?'} → {t.range or '?'}]" if (t.domain or t.range) else ""
        )
        fn = " (functional)" if t.functional else ""
        lines.append(f"- {t.curie}{sig}{fn}: {t.definition or t.label}")
    if individuals:
        lines += ["", "## Individuals", ""]
        for t in individuals:
            types = f" (a {', '.join(t.types)})" if t.types else ""
            lines.append(f"- {t.curie}{types}: {t.definition or t.label}")
    return "\n".join(lines) + "\n"


@mcp.resource("gmeow://ontology/constitution")
def gmeow_constitution() -> str:
    """Expose the GMEOW Constitution (CONSTITUTION.md)."""
    path = PROJECT_ROOT / "CONSTITUTION.md"
    if not path.exists():
        return "# Constitution not found\n"
    return path.read_text(encoding="utf-8")


def run() -> None:
    """Start the MCP stdio server."""
    mcp.run(transport="stdio")
