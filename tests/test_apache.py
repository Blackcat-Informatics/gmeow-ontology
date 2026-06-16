"""Tests for the Apache content-negotiation include renderer."""

from __future__ import annotations

from gmeow_tools.apache import render_conf
from gmeow_tools.self_desc import load_self_description


def test_render_conf_has_conneg_directives() -> None:
    conf = render_conf()
    assert "<IfModule mod_rewrite.c>" in conf
    assert "Access-Control-Allow-Origin" in conf
    assert "text/turtle" in conf
    assert "application/ld+json" in conf
    assert "RewriteEngine On" in conf
    assert "/gmeow/terms/$1/" in conf


def test_render_conf_has_all_formats() -> None:
    conf = render_conf()
    for snippet in ("/gmeow.ttl", "/gmeow.rdf", "/gmeow.nt", "/gmeow.jsonld"):
        assert snippet in conf


def test_render_conf_dereferences_slice_iris() -> None:
    """#329: slice IRIs resolve to RDF serializations or slice docs."""
    conf = render_conf()
    assert "^/?gmeow/slices/([a-z][a-z0-9-]*)$" in conf
    assert "/gmeow/slices/$1/ [R=303,L]" in conf


def test_signposting_cite_as_uses_concept_doi() -> None:
    """The /gmeow landing page advertises the concept DOI, never a version DOI."""
    meta = load_self_description()
    conf = render_conf()
    assert f'<https://doi.org/{meta.concept_doi}>; rel=\\"cite-as\\"' in conf
    assert 'rel=\\"describedby\\"' in conf
    assert 'rel=\\"item\\"' in conf
    assert '.gts>; rel=\\"item\\"' in conf  # GTS package is an item link


def test_render_conf_dereferences_full_profile_iri() -> None:
    """#330: /gmeow/full negotiates to the full-profile serialization."""
    conf = render_conf()
    for snippet in (
        "/gmeow/full.ttl",
        "/gmeow/full.rdf",
        "/gmeow/full.nt",
        "/gmeow/full.jsonld",
    ):
        assert snippet in conf, f"missing {snippet}"
    assert "RewriteRule ^/?gmeow/full$ /gmeow/full.ttl [R=303,L]" in conf
    assert "RewriteRule ^/?gmeow/full$ /gmeow/profiles/full/ [R=303,L]" in conf
    assert (
        "RewriteCond %{HTTP_ACCEPT} text/html\n"
        "    RewriteRule ^/?gmeow/full$ /gmeow/profiles/full/ [R=303,L]"
    ) in conf


def test_render_conf_dereferences_named_profile_iris() -> None:
    """#330: /gmeow/profiles/<name> negotiates to the named-profile serialization."""
    conf = render_conf()
    for snippet in (
        "/gmeow/profiles/$1.ttl",
        "/gmeow/profiles/$1.rdf",
        "/gmeow/profiles/$1.nt",
        "/gmeow/profiles/$1.jsonld",
    ):
        assert snippet in conf, f"missing {snippet}"
    assert "^/?gmeow/profiles/([a-z][a-z0-9-]*)$" in conf
    assert "/gmeow/profiles/$1/ [R=303,L]" in conf
    assert (
        "RewriteCond %{HTTP_ACCEPT} text/html\n"
        "    RewriteRule ^/?gmeow/profiles/([a-z][a-z0-9-]*)$ "
        "/gmeow/profiles/$1/ [R=303,L]"
    ) in conf


def test_render_conf_profile_media_types() -> None:
    """Profile serializations get the correct Content-Type headers."""
    conf = render_conf()
    # .ttl already covered by the existing catch-all; assert the others were widened.
    assert '<LocationMatch "^/gmeow(\\.rdf|/.+\\.rdf)$">' in conf
    assert '<LocationMatch "^/gmeow(\\.nt|/.+\\.nt)$">' in conf
    jsonld = '<LocationMatch "^/gmeow(\\.jsonld|/.+\\.jsonld|/context\\.jsonld)$">'
    assert jsonld in conf


def test_render_conf_redirects_legacy_module_iris() -> None:
    """Pre-slice releases published modules/* IRIs; they 301 to slices/*."""
    conf = render_conf()
    assert "^/?gmeow/modules/([a-z][a-z0-9-]*)$ /gmeow/slices/$1 [R=301,L]" in conf


def test_render_conf_conneg_cache_semantics() -> None:
    """The negotiated endpoint must Vary: Accept and stay uncached."""
    conf = render_conf()
    assert 'Header always set Vary "Accept"' in conf
    assert "private, no-store" in conf
    assert "^/gmeow(/?$|/(slices|profiles)/[a-z][a-z0-9-]*$|/full$)" in conf
