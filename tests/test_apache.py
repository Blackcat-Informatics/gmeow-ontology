"""Tests for the Apache content-negotiation include renderer."""

from __future__ import annotations

from gmeow_tools.apache import render_conf


def test_render_conf_has_conneg_directives() -> None:
    conf = render_conf()
    assert '<Location "/gmeow">' in conf
    assert "Access-Control-Allow-Origin" in conf
    assert "text/turtle" in conf
    assert "application/ld+json" in conf
    assert "RewriteEngine On" in conf
    # Per-term slash dereferencing.
    assert "/gmeow/index.html#$1" in conf


def test_render_conf_has_all_formats() -> None:
    conf = render_conf()
    for snippet in ("/gmeow.ttl", "/gmeow.rdf", "/gmeow.nt", "/gmeow.jsonld"):
        assert snippet in conf
