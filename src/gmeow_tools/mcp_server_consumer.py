"""Consumer-safe MCP server backed by the bundled GMEOW GTS snapshot.

Thin FastMCP wiring only: the term-lookup / llms.txt / OKF-index business logic
lives in Rust (``gmeow_native.pipeline.McpView``, #1031). Python keeps just the
language-resolution layer (reusing the shared ``language_tags`` selector) and the
memory tools, then delegates each surface to the Rust handle. The selector is
resolved here (raising :class:`UnknownLanguageError` on a bad tag) and its
``requested`` tag list is threaded into the Rust renderers, which reproduce the
prior wire format byte-for-byte.
"""

from __future__ import annotations

import json
import os
from contextlib import suppress
from dataclasses import asdict
from functools import lru_cache
from pathlib import Path
from typing import TYPE_CHECKING, Any, Protocol, cast

from fastmcp import FastMCP
from gts import read

from gmeow_tools.config import GTS_SNAPSHOT_FILE
from gmeow_tools.gts_views import FoldView
from gmeow_tools.language_tags import UnknownLanguageError

if TYPE_CHECKING:
    from gmeow_tools.language_tags import LangSelector

mcp = FastMCP("gmeow")

#: Cached language selector validated at server startup.
_STARTUP_SELECTOR: LangSelector | None = None


@lru_cache(maxsize=1)
def _view() -> FoldView:
    """Load the bundled GTS snapshot into a fold view (for language resolution)."""
    return FoldView(read(GTS_SNAPSHOT_FILE.read_bytes()))


class _McpView(Protocol):
    """The three Rust MCP surfaces (``gmeow_native.pipeline.McpView``, #1031)."""

    def lookup_term(self, term: str, requested: list[str]) -> str: ...
    def llms_txt(self, requested: list[str]) -> str: ...
    def okf_index(self, requested: list[str]) -> str: ...


@lru_cache(maxsize=1)
def _rust_view() -> _McpView:
    """The Rust MCP surface over the bundled snapshot (loaded once, #1031)."""
    from gmeow_native import pipeline

    return cast("_McpView", pipeline.McpView(GTS_SNAPSHOT_FILE.read_bytes()))


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


def _requested(lang: str | None) -> list[str]:
    """The resolved requested-tag list for ``lang`` (raises on an unknown tag)."""
    return list(_selector(_view(), lang).requested)


@mcp.tool()
def gmeow_lookup_term(term: str, lang: str | None = None) -> str:
    """Resolve a bundled GMEOW term to its public metadata.

    Args:
        term: CURIE, local name, IRI, or label fragment to look up.
        lang: Optional BCP-47 language tag. Overrides ``GMEOW_LANG``.
    """
    try:
        requested = _requested(lang)
    except UnknownLanguageError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return _rust_view().lookup_term(term, requested)


@mcp.resource("gmeow://ontology/llms.txt{?lang}")
def gmeow_llms_txt(lang: str | None = None) -> str:
    """Expose a compact bundled vocabulary index.

    Args:
        lang: Optional BCP-47 language tag. Overrides ``GMEOW_LANG``.
    """
    try:
        requested = _requested(lang)
    except UnknownLanguageError as exc:
        return f"# Error: {exc}\n"
    return _rust_view().llms_txt(requested)


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
    try:
        requested = _requested(lang)
    except UnknownLanguageError as exc:
        return json.dumps({"ok": False, "error": str(exc)})
    return _rust_view().okf_index(requested)


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
