"""Tests for the consumer-safe MCP server backed by the bundled GTS snapshot."""

from __future__ import annotations

import asyncio
import json
from unittest.mock import patch

import pytest

from gmeow_tools.gts_views import FoldView
from gmeow_tools.language_tags import LangSelector, UnknownLanguageError
from gmeow_tools.mcp_server_consumer import (
    _validate_startup_lang,
    gmeow_llms_txt,
    gmeow_lookup_term,
    mcp,
    run,
)


@pytest.fixture(autouse=True)
def _reset_startup_selector(monkeypatch: pytest.MonkeyPatch) -> None:
    """Clear the cached startup selector and GMEOW_LANG between tests."""
    import gmeow_tools.mcp_server_consumer as consumer

    consumer._STARTUP_SELECTOR = None
    monkeypatch.delenv("GMEOW_LANG", raising=False)


def _json_response(text: str) -> dict[str, object]:
    """Parse a tool's JSON response string."""
    return json.loads(text)  # type: ignore[no-any-return]


# --------------------------------------------------------------------------- #
# Startup language validation
# --------------------------------------------------------------------------- #


def test_startup_validation_raises_on_unknown_lang(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """An invalid GMEOW_LANG must raise UnknownLanguageError immediately."""
    monkeypatch.setenv("GMEOW_LANG", "xx-unknown")
    with pytest.raises(UnknownLanguageError):
        _validate_startup_lang()


def test_run_exits_loudly_on_bad_gmeow_lang(monkeypatch: pytest.MonkeyPatch) -> None:
    """run() must validate GMEOW_LANG before starting the MCP server."""
    monkeypatch.setenv("GMEOW_LANG", "xx-unknown")
    with (
        patch.object(mcp, "run") as mock_run,
        pytest.raises(UnknownLanguageError),
    ):
        run()
    mock_run.assert_not_called()


# --------------------------------------------------------------------------- #
# Per-call language parameter
# --------------------------------------------------------------------------- #


def test_gmeow_lookup_term_default_is_english() -> None:
    """Without a lang argument the lookup returns the English label."""
    data = _json_response(gmeow_lookup_term("gmeow:langFrench"))
    assert data["ok"] is True
    assert data["label"] == "French"


def test_gmeow_lookup_term_honors_per_call_lang() -> None:
    """A per-call lang argument overrides the default English selection."""
    data = _json_response(gmeow_lookup_term("gmeow:langFrench", lang="fr"))
    assert data["ok"] is True
    assert data["label"] == "français"


def test_gmeow_llms_txt_default_returns_content() -> None:
    """The resource still works when no lang parameter is supplied."""
    text = gmeow_llms_txt()
    assert "gmeow:langFrench" in text
    assert "French" in text


def test_gmeow_llms_txt_threads_lang_to_selector(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A per-call lang argument is forwarded to the language selector."""
    import gmeow_tools.mcp_server_consumer as consumer

    seen: list[str | None] = []
    original_selector = consumer._selector

    def _capturing_selector(view: FoldView, lang: str | None = None) -> LangSelector:
        seen.append(lang)
        return original_selector(view, lang)

    monkeypatch.setattr(consumer, "_selector", _capturing_selector)
    gmeow_llms_txt()
    gmeow_llms_txt(lang="fr")
    assert seen == [None, "fr"]


def test_gmeow_llms_txt_rejects_unknown_per_call_lang() -> None:
    """An unknown per-call lang raises UnknownLanguageError."""
    with pytest.raises(UnknownLanguageError):
        gmeow_llms_txt(lang="xx-unknown")


async def _read_resource(uri: str) -> str:
    """Helper to read a resource through the FastMCP server."""
    result = await mcp.read_resource(uri)
    content = result.contents[0].content
    assert isinstance(content, str)
    return content


def test_llms_txt_resource_default_and_lang_via_mcp() -> None:
    """The parameterized resource URI works with and without the lang query."""
    default = asyncio.run(_read_resource("gmeow://ontology/llms.txt"))
    french = asyncio.run(_read_resource("gmeow://ontology/llms.txt?lang=fr"))
    assert "French" in default
    assert "# GMEOW" in french
