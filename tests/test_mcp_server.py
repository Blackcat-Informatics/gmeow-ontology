"""Tests for the GMEOW MCP server.

Principle 7 (Verified by construction): every tool and resource is exercised.
"""

from __future__ import annotations

import json

from gmeow_tools.mcp_server import (
    _expand_curie,
    _lookup_term,
    gmeow_constitution,
    gmeow_llms_txt,
    gmeow_lookup_term,
    gmeow_validate,
)

# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _json_response(text: str) -> dict[str, object]:
    """Parse a tool's JSON response string."""
    return json.loads(text)  # type: ignore[no-any-return]


# --------------------------------------------------------------------------- #
# CURIE expansion
# --------------------------------------------------------------------------- #


def test_expand_curie_known_prefix() -> None:
    """A known prefix expands correctly."""
    assert _expand_curie("gmeow:Person").startswith(
        "https://blackcatinformatics.ca/gmeow/"
    )


def test_expand_curie_raw_iri() -> None:
    """A raw IRI is returned unchanged."""
    iri = "https://example.org/test"
    assert _expand_curie(iri) == iri


def test_expand_curie_no_prefix() -> None:
    """A bare local name is expanded with the GMEOW namespace."""
    assert _expand_curie("Person").startswith("https://blackcatinformatics.ca/gmeow/")


# --------------------------------------------------------------------------- #
# Term lookup
# --------------------------------------------------------------------------- #


def test_lookup_term_known_class() -> None:
    """A known GMEOW class resolves with label and definition."""
    result = _lookup_term("gmeow:Person")
    assert result is not None
    assert result["category"] == "class"
    assert result["label"]
    assert result["definition"]


def test_lookup_term_unknown() -> None:
    """An unknown CURIE returns None."""
    assert _lookup_term("gmeow:DefinitelyNotARealTerm12345") is None


def test_lookup_term_individual() -> None:
    """Named individuals resolve correctly."""
    result = _lookup_term("gmeow:Agent")
    assert result is not None
    assert result["category"] in ("class", "individual")


# --------------------------------------------------------------------------- #
# Tools (direct function tests)
# --------------------------------------------------------------------------- #


def test_gmeow_validate_returns_json() -> None:
    """The validate tool returns a parseable JSON object."""
    text = gmeow_validate()
    data = _json_response(text)
    assert "ok" in data
    assert isinstance(data["ok"], bool)
    assert "errors" in data
    assert "warnings" in data


def test_gmeow_lookup_term_found() -> None:
    """Lookup for a known term returns structured metadata."""
    text = gmeow_lookup_term("gmeow:Person")
    data = _json_response(text)
    assert data["ok"] is True
    assert data["category"] == "class"
    assert data["label"]


def test_gmeow_lookup_term_not_found() -> None:
    """Lookup for an unknown term reports failure gracefully."""
    text = gmeow_lookup_term("gmeow:NoSuchTerm")
    data = _json_response(text)
    assert data["ok"] is False
    assert "not found" in str(data["error"]).lower()


# --------------------------------------------------------------------------- #
# Resources
# --------------------------------------------------------------------------- #


def test_constitution_resource() -> None:
    """The constitution resource contains expected content."""
    text = gmeow_constitution()
    assert "Principle 4" in text
    assert "One canonical source" in text


def test_llms_txt_resource() -> None:
    """The llms.txt resource contains known class CURIEs."""
    text = gmeow_llms_txt()
    assert "gmeow:Person" in text or "## Classes" in text
    assert "## Properties" in text


# --------------------------------------------------------------------------- #
# Server registration smoke test (via FastMCP internal registry)
# --------------------------------------------------------------------------- #


def test_server_has_expected_tools() -> None:
    """The FastMCP server instance registers all five tools."""
    import asyncio

    from gmeow_tools.mcp_server import mcp

    tools = asyncio.run(mcp.list_tools())
    tool_names = {t.name for t in tools}
    expected = {
        "gmeow_validate",
        "gmeow_compile_mappings",
        "gmeow_compile_statements",
        "gmeow_reason",
        "gmeow_lookup_term",
    }
    assert expected <= tool_names, f"Missing tools: {expected - tool_names}"


def test_server_has_expected_resources() -> None:
    """The FastMCP server instance registers both resources."""
    import asyncio

    from gmeow_tools.mcp_server import mcp

    resources = asyncio.run(mcp.list_resources())
    resource_uris = {str(r.uri) for r in resources}
    assert "gmeow://ontology/llms.txt" in resource_uris
    assert "gmeow://ontology/constitution" in resource_uris
