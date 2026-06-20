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


def _load_generator_modules() -> None:
    # Trigger @register side effects for all generators.
    from gmeow_tools import (  # noqa: F401
        apache,
        export,
        lpg,
        mapping_compile,
        metadata,
        schema_compile,
        statement_compile,
    )


@mcp.tool()
def gmeow_regenerate(names: list[str] | None = None) -> str:
    """Rebuild all checked-in generated artifacts from canonical sources.

    Args:
        names: Generator names to run (default: all in dependency order).
    """
    _load_generator_modules()
    from gmeow_tools.generator import regenerate
    from gmeow_tools.mapping_dsl import CompileError

    try:
        results = regenerate(names or None)
    except CompileError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    except Exception as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps(
        {
            "ok": True,
            "generators": {
                name: {
                    "written": [str(p) for p in report.written],
                    "drifted": report.drifted,
                    "orphans": report.orphans,
                }
                for name, report in results.items()
            },
        }
    )


@mcp.tool()
def gmeow_check_generated(names: list[str] | None = None) -> str:
    """Drift + orphan check for all registered generators.

    Args:
        names: Generator names to check (default: all).
    """
    _load_generator_modules()
    from gmeow_tools.generator import check_all

    try:
        results = check_all(names or None)
    except Exception as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    total_drift = sum(len(r.drifted) for r in results.values())
    total_orphans = sum(len(r.orphans) for r in results.values())
    return json.dumps(
        {
            "ok": total_drift == 0 and total_orphans == 0,
            "drifted": total_drift,
            "orphaned": total_orphans,
            "generators": {
                name: {
                    "drifted": report.drifted,
                    "orphans": report.orphans,
                }
                for name, report in results.items()
            },
        }
    )


@mcp.tool()
def gmeow_reason(
    mode: str = "native", reasoner: str = "ELK", profile: str = "DL"
) -> str:
    """Run consistency reasoning over the merged ontology.

    Native EL/DL reasoning (Rust, Java/Docker-free) is the default and the
    authority lane. The classic Docker/Java ELK/HermiT oracle is available as
    an explicit opt-in via ``mode="docker"``.

    Args:
        mode: Reasoning backend — ``native`` (default) or ``docker``.
        reasoner: Docker mode only — ``ELK`` (fast) or ``hermit`` (sound+complete).
        profile: Docker mode only — OWL 2 profile to validate against: ``DL``,
            ``EL``, ``QL``, ``RL``, or ``Full``.
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    if mode == "native":
        try:
            report = reasoning.reason_native(merge=False, run_box_roles=False)
        except (ImportError, ValueError, RuntimeError, OSError) as exc:
            return json.dumps({"ok": False, "error": str(exc)})
        if hasattr(report, "warning_count"):
            warnings = report.warning_count
        else:
            warnings = len(list(report.warnings))
        return json.dumps(
            {
                "ok": report.ok,
                "mode": "native",
                "message": (
                    f"native EL/DL reasoning: {report.error_count} error(s), "
                    f"{warnings} warning(s)"
                ),
                "errors": report.error_count,
                "warnings": warnings,
            }
        )

    if mode == "docker":
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
        return json.dumps(
            {
                "ok": True,
                "mode": "docker",
                "message": f"{reasoner} consistency check passed",
            }
        )

    return json.dumps(
        {
            "ok": False,
            "error": f"unknown reasoning mode: {mode!r} (expected native or docker)",
        }
    )


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


# --------------------------------------------------------------------------- #
# The grounded-memory triad (#297, D2 — CONSTITUTION P14): store / recall /
# revise as the agent-native interface, wrapping the GTS example Memory
# (a content-addressed, append-only GTS ai-package on disk). The server only
# exposes: every behavior — claim reification, suppression-not-deletion,
# token-overlap recall — lives in gts.examples.agent_memory. The one config
# knob is the GMEOW_MEMORY_PATH environment variable (default
# ~/.gmeow/memory.gts), set in the mcpServers block. See docs/mcp-server.md.
# --------------------------------------------------------------------------- #


def _memory() -> Any:
    """The Memory over the configured package path (env-selected)."""
    import os
    from pathlib import Path as _Path

    from gts.examples.agent_memory import Memory

    path = _Path(
        os.environ.get("GMEOW_MEMORY_PATH", "")
        or _Path.home() / ".gmeow" / "memory.gts"
    ).expanduser()
    path.parent.mkdir(parents=True, exist_ok=True)
    return Memory(path)


def _claim_dict(claim: Any) -> dict[str, Any]:
    from dataclasses import asdict

    return asdict(claim)


#: Tool agent IRIs for the triad's own provenance records (#390): tools are
#: SoftwareAgents; tool-ness is the role they play in the call event.
_TOOL_AGENT_NS = "urn:gmeow:tool:"


def _record_tool_call(
    memory: Any,
    tool: str,
    arguments: dict[str, Any],
    *,
    result: str | None = None,
    generated: tuple[str, ...] = (),
) -> None:
    """Record the triad's own call as ToolCall provenance (#390).

    The memory MCP is the agentic slice's first live producer: every
    store/revise appends a gmeow:ToolCall record alongside the claims it
    touched, and produced claims link back via gmeow:wasGeneratedBy (P5).
    Best-effort by design — provenance recording must never fail the
    user-facing operation it describes. recall is the read path and
    records nothing.
    """
    import contextlib

    with contextlib.suppress(Exception):
        memory.record_tool_call(
            _TOOL_AGENT_NS + tool,
            arguments=json.dumps(
                {k: v for k, v in arguments.items() if v is not None},
                sort_keys=True,
            ),
            result=result,
            generated=generated,
        )


@mcp.tool()
def store_claim(
    text: str,
    source: str | None = None,
    confidence: float | None = None,
    according_to: str | None = None,
) -> str:
    """Store one claim in the agent's grounded memory.

    The claim is appended as a reified RDF 1.2 statement — attributed,
    optionally confidence-weighted ([0, 1]) and standpoint-indexed
    (according_to), never asserted as bare truth. Returns the stored claim
    (its ``id`` is the handle for ``revise_belief``).
    """
    try:
        memory = _memory()
        claim = memory.store(
            text,
            source=source,
            confidence=confidence,
            according_to=according_to,
        )
    except (ValueError, OSError) as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    response = json.dumps({"ok": True, "claim": _claim_dict(claim)})
    # toolResult is the VERBATIM payload the tool returned (byte-faithful);
    # the produced claim links back via wasGeneratedBy (P5).
    _record_tool_call(
        memory,
        "store_claim",
        {
            "text": text,
            "source": source,
            "confidence": confidence,
            "according_to": according_to,
        },
        result=response,
        generated=(claim.id,),
    )
    return response


@mcp.tool()
def recall(
    query: str = "",
    min_confidence: float | None = None,
    limit: int = 10,
    include_suppressed: bool = False,
) -> str:
    """Recall claims from the agent's grounded memory.

    Empty query returns the most recent claims; otherwise case-insensitive
    token-overlap ranking. Revised (suppressed) claims are EXCLUDED by
    default — suppression is honored on every recall path (P10); pass
    include_suppressed=true only for audit views, where each claim's
    ``suppressed`` flag tells you what you are looking at.
    """
    if limit < 0:
        return json.dumps({"ok": False, "error": "limit must be non-negative"})
    try:
        claims = _memory().recall(
            query,
            min_confidence=min_confidence,
            limit=limit,
            include_suppressed=include_suppressed,
        )
    except (ValueError, OSError) as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps({"ok": True, "claims": [_claim_dict(c) for c in claims]})


@mcp.tool()
def revise_belief(
    claim_id: str,
    reason: str | None = None,
    superseded_by: str | None = None,
) -> str:
    """Revise a belief: suppress the claim, never delete it (P10).

    The claim is retained with a suppression frame (the audit trail of what
    the agent believed WHEN survives); ``reason`` records why, and
    ``superseded_by`` links the successor claim for the derivation chain.
    Recall stops returning it unless include_suppressed is requested.
    """
    try:
        memory = _memory()
        known = {c.id for c in memory.claims()}
        if claim_id not in known:
            return json.dumps({"ok": False, "error": f"unknown claim id: {claim_id}"})
        if superseded_by is not None and superseded_by not in known:
            return json.dumps(
                {"ok": False, "error": f"unknown superseded_by id: {superseded_by}"}
            )
        memory.revise(claim_id, reason=reason, superseded_by=superseded_by)
    except (ValueError, OSError) as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    response = json.dumps(
        {"ok": True, "suppressed": claim_id, "superseded_by": superseded_by}
    )
    _record_tool_call(
        memory,
        "revise_belief",
        {"claim_id": claim_id, "reason": reason, "superseded_by": superseded_by},
        result=response,
    )
    return response


def run() -> None:
    """Start the MCP stdio server."""
    mcp.run(transport="stdio")
