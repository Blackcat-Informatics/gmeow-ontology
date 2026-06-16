"""Consumer-safe MCP server backed by the bundled GMEOW GTS snapshot."""

from __future__ import annotations

import json
import os
from contextlib import suppress
from dataclasses import asdict
from pathlib import Path
from typing import Any

from fastmcp import FastMCP
from gts import read

from gmeow_tools.config import GTS_SNAPSHOT_FILE, NAMESPACE
from gmeow_tools.export import Term, collect_terms, fold_meta
from gmeow_tools.gts_views import FoldView

mcp = FastMCP("gmeow")


def _view() -> FoldView:
    """Load the bundled GTS snapshot into a fold view."""
    return FoldView(read(GTS_SNAPSHOT_FILE.read_bytes()))


def _terms() -> list[Term]:
    """Collect public GMEOW terms from the bundled GTS snapshot."""
    return collect_terms(_view())


def _lookup_term(query: str) -> dict[str, Any] | None:
    """Resolve a CURIE, local name, IRI, or unambiguous prefix."""
    needle = query.strip()
    if not needle:
        return None
    lower = needle.lower()
    matches: list[Term] = []
    for term in _terms():
        candidates = {
            term.curie,
            term.iri,
            term.iri.removeprefix(NAMESPACE),
            term.label,
        }
        if lower in {candidate.lower() for candidate in candidates if candidate}:
            matches = [term]
            break
        if term.curie.lower().startswith(lower) or term.label.lower().startswith(lower):
            matches.append(term)
    if len(matches) != 1:
        return None
    return matches[0].as_record()


@mcp.tool()
def gmeow_lookup_term(term: str) -> str:
    """Resolve a bundled GMEOW term to its public metadata."""
    result = _lookup_term(term)
    if result is None:
        return json.dumps({"ok": False, "error": f"Term not found: {term}"})
    result["ok"] = True
    return json.dumps(result)


@mcp.resource("gmeow://ontology/llms.txt")
def gmeow_llms_txt() -> str:
    """Expose a compact bundled vocabulary index."""
    view = _view()
    title, version = fold_meta(view)
    terms = collect_terms(view)
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]
    lines = [
        f"# {title}",
        "",
        f"Vocabulary {version}. Namespace: {NAMESPACE}.",
        "",
        "## Classes",
        "",
    ]
    for term in classes:
        parents = f" (subClassOf {', '.join(term.parents)})" if term.parents else ""
        lines.append(f"- {term.curie}{parents}: {term.definition or term.label}")
    lines += ["", "## Properties", ""]
    for term in properties:
        signature = (
            f" [{term.domain or '?'} -> {term.range or '?'}]"
            if term.domain or term.range
            else ""
        )
        functional = " (functional)" if term.functional else ""
        lines.append(
            f"- {term.curie}{signature}{functional}: {term.definition or term.label}"
        )
    if individuals:
        lines += ["", "## Individuals", ""]
        for term in individuals:
            types = f" (a {', '.join(term.types)})" if term.types else ""
            lines.append(f"- {term.curie}{types}: {term.definition or term.label}")
    return "\n".join(lines) + "\n"


def _memory() -> Any:
    """Open the configured append-only GTS memory package."""
    from gts.examples.agent_memory import Memory

    path = Path(
        os.environ.get("GMEOW_MEMORY_PATH", "") or Path.home() / ".gmeow" / "memory.gts"
    ).expanduser()
    path.parent.mkdir(parents=True, exist_ok=True)
    return Memory(path)


def _claim_dict(claim: Any) -> dict[str, Any]:
    """Return a JSON-serializable claim record."""
    return asdict(claim)


_TOOL_AGENT_NS = "urn:gmeow:tool:"


def _record_tool_call(
    memory: Any,
    tool: str,
    arguments: dict[str, Any],
    *,
    result: str | None = None,
    generated: tuple[str, ...] = (),
) -> None:
    """Best-effort provenance recording for memory writes."""
    with suppress(Exception):
        memory.record_tool_call(
            _TOOL_AGENT_NS + tool,
            arguments=json.dumps(
                {key: value for key, value in arguments.items() if value is not None},
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
    """Store one attributed, optionally confidence-weighted memory claim."""
    try:
        memory = _memory()
        claim = memory.store(
            text,
            source=source,
            confidence=confidence,
            according_to=according_to,
        )
    except (OSError, ValueError) as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    response = json.dumps({"ok": True, "claim": _claim_dict(claim)})
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
def recall_claims(
    query: str = "",
    min_confidence: float | None = None,
    limit: int = 10,
    include_suppressed: bool = False,
) -> str:
    """Recall stored memory claims from the configured GTS package."""
    if limit < 0:
        return json.dumps({"ok": False, "error": "limit must be non-negative"})
    try:
        claims = _memory().recall(
            query,
            min_confidence=min_confidence,
            limit=limit,
            include_suppressed=include_suppressed,
        )
    except (OSError, ValueError) as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps({"ok": True, "claims": [_claim_dict(claim) for claim in claims]})


@mcp.tool()
def revise_belief(
    claim_id: str,
    reason: str | None = None,
    superseded_by: str | None = None,
) -> str:
    """Suppress a stored memory claim without deleting its audit trail."""
    try:
        memory = _memory()
        known = {claim.id for claim in memory.claims()}
        if claim_id not in known:
            return json.dumps({"ok": False, "error": f"unknown claim id: {claim_id}"})
        if superseded_by is not None and superseded_by not in known:
            return json.dumps(
                {"ok": False, "error": f"unknown superseded_by id: {superseded_by}"}
            )
        memory.revise(claim_id, reason=reason, superseded_by=superseded_by)
    except (OSError, ValueError) as exc:
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
