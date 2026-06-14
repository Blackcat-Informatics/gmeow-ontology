# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Small documentation helpers shared by generated profile surfaces."""

from __future__ import annotations


def profiles_section_html() -> str:
    """Return the profile-composition HTML section used by legacy consumers.

    The current ontology-docs generator renders first-party Markdown and HTML
    directly. This helper remains for tests and any caller that needs only the
    profile list fragment without invoking a site generator.
    """
    from gmeow_tools.config import FULL_PROFILE_IRI, NAMED_PROFILE_NS, ONTOLOGY_IRI
    from gmeow_tools.profiles_gen import dependency_closure, group_named_profiles
    from gmeow_tools.slices import discover_slices

    slices = discover_slices()
    core_n = sum(1 for s in slices.values() if s.is_core)
    rows = [
        (
            ONTOLOGY_IRI,
            "core",
            f"the root IRI is the core profile - {core_n} tierCore slices",
        ),
        (
            FULL_PROFILE_IRI,
            "full",
            f"everything: core plus {len(slices) - core_n} extension slices",
        ),
    ]
    for name, members in group_named_profiles(slices).items():
        closure = dependency_closure(members, slices)
        rows.append(
            (
                NAMED_PROFILE_NS + name,
                name,
                f"{len(members)} declared slice(s), {len(closure)} in the "
                "dependency-closed import set",
            )
        )
    items = "\n".join(
        f'<li><a href="{iri}"><code>{iri}</code></a> - '
        f"<strong>{name}</strong>: {desc}</li>"
        for iri, name, desc in rows
    )
    return (
        '<section id="profiles">\n<h2>Profiles</h2>\n'
        "<p>Composition lives in profile IRIs (#330): each profile is a "
        "generated <code>owl:imports</code> aggregation - dereferenceable "
        "via content negotiation, citable, and reasonable on its own. "
        "Named profiles are slim: declared members plus their dependency "
        "closure, never the whole core.</p>\n"
        f"<ul>\n{items}\n</ul>\n</section>"
    )
