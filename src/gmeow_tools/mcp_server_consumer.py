"""Consumer-safe MCP server backed by the bundled GMEOW GTS snapshot."""

from __future__ import annotations

import json
import os
from contextlib import suppress
from dataclasses import asdict
from pathlib import Path
from typing import TYPE_CHECKING, Any

from fastmcp import FastMCP
from gts import read

from gmeow_tools.config import GTS_SNAPSHOT_FILE, NAMESPACE
from gmeow_tools.export import Term, collect_terms, fold_meta, marked
from gmeow_tools.gts_views import FoldView
from gmeow_tools.language_tags import UnknownLanguageError

if TYPE_CHECKING:
    from gmeow_tools.language_tags import LangSelector

mcp = FastMCP("gmeow")

#: Cached language selector validated at server startup.
_STARTUP_SELECTOR: LangSelector | None = None

#: Cached collected terms keyed by resolved language tag list.
_TERMS_CACHE: dict[str | None, list[Term]] = {}


def _view() -> FoldView:
    """Load the bundled GTS snapshot into a fold view."""
    return FoldView(read(GTS_SNAPSHOT_FILE.read_bytes()))


def _selector(view: FoldView, lang: str | None = None) -> LangSelector:
    """Resolve ``lang`` or ``GMEOW_LANG`` against the snapshot's tag map.

    ``lang`` takes precedence over ``GMEOW_LANG``; both fall back to English.
    When ``lang`` is omitted, the selector validated at server startup is reused.
    An unknown tag raises :class:`~gmeow_tools.language_tags.UnknownLanguageError`.
    """
    from gmeow_tools.language_tags import resolve_lang_input

    if lang is None and _STARTUP_SELECTOR is not None:
        return _STARTUP_SELECTOR
    raw = lang if lang is not None else os.environ.get("GMEOW_LANG")
    return resolve_lang_input(raw, view.tag_map(), available=view.available_languages())


def _validate_startup_lang() -> None:
    """Validate ``GMEOW_LANG`` at server startup and cache the selector."""
    global _STARTUP_SELECTOR
    view = _view()
    _STARTUP_SELECTOR = _selector(view)


def _summary(term: Term) -> str:
    """Selected definition-or-label with a fallback marker when appropriate."""
    return marked(
        term.definition or term.label,
        term.definition_fallback or term.label_fallback,
    )


def _terms(lang: str | None = None) -> list[Term]:
    """Collect public GMEOW terms from the bundled GTS snapshot."""
    view = _view()
    selector = _selector(view, lang)
    cache_key = ",".join(selector.requested)
    if cache_key not in _TERMS_CACHE:
        _TERMS_CACHE[cache_key] = list(collect_terms(view, selector=selector))
    return list(_TERMS_CACHE[cache_key])


def _lookup_term(query: str, lang: str | None = None) -> dict[str, Any] | None:
    """Resolve a CURIE, local name, IRI, or unambiguous prefix."""
    needle = query.strip()
    if not needle:
        return None
    lower = needle.lower()
    matches: list[Term] = []
    for term in _terms(lang):
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
def gmeow_lookup_term(term: str, lang: str | None = None) -> str:
    """Resolve a bundled GMEOW term to its public metadata.

    Args:
        term: CURIE, local name, IRI, or label fragment to look up.
        lang: Optional BCP-47 language tag. Overrides ``GMEOW_LANG``.
    """
    try:
        result = _lookup_term(term, lang)
    except UnknownLanguageError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    if result is None:
        return json.dumps({"ok": False, "error": f"Term not found: {term}"})
    result["ok"] = True
    return json.dumps(result)


@mcp.resource("gmeow://ontology/llms.txt{?lang}")
def gmeow_llms_txt(lang: str | None = None) -> str:
    """Expose a compact bundled vocabulary index.

    Args:
        lang: Optional BCP-47 language tag. Overrides ``GMEOW_LANG``.
    """
    try:
        view = _view()
        selector = _selector(view, lang)
        title, version = fold_meta(view)
        terms = collect_terms(view, selector=selector)
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
            lines.append(f"- {term.curie}{parents}: {_summary(term)}")
        lines += ["", "## Properties", ""]
        for term in properties:
            signature = (
                f" [{term.domain or '?'} -> {term.range or '?'}]"
                if term.domain or term.range
                else ""
            )
            functional = " (functional)" if term.functional else ""
            lines.append(f"- {term.curie}{signature}{functional}: {_summary(term)}")
        if individuals:
            lines += ["", "## Individuals", ""]
            for term in individuals:
                types = f" (a {', '.join(term.types)})" if term.types else ""
                lines.append(f"- {term.curie}{types}: {_summary(term)}")
        return "\n".join(lines) + "\n"
    except UnknownLanguageError as exc:
        return f"# Error: {exc}\n"


@mcp.resource("gmeow://ontology/okf-index{?lang}")
def gmeow_okf_index(lang: str | None = None) -> str:
    """Expose the OKF (Open Knowledge Format) bundle index for agents (#780).

    OKF is the agent-facing surface: one Markdown concept document per term, with
    YAML frontmatter and ``[label](path)`` links — the form ``gts from-okf`` folds.
    Returns a JSON manifest ``{ok, format, lossy, count, documents:[…]}`` where each
    document is ``{path, type, title, resource}``; the bytes live in the bundle's
    ``REP_OKF`` blob (see :func:`gmeow_tools.bundle.bundled_okf`). A LOSSY surface:
    the flat term view only — the OWL axioms and statement layer stay canonical.

    Args:
        lang: Optional BCP-47 language tag. Overrides ``GMEOW_LANG``.
    """
    from gmeow_tools.okf_export import okf_index_records

    try:
        documents = okf_index_records(_terms(lang))
    except UnknownLanguageError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return json.dumps(
        {
            "ok": True,
            "format": "okf",
            "lossy": True,
            "count": len(documents),
            "documents": documents,
        },
        ensure_ascii=False,
    )


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
    _validate_startup_lang()
    mcp.run(transport="stdio")
