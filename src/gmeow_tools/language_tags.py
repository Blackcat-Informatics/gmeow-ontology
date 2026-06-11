"""Shared language-tag policy helper.

Recognises GMEOW internal private-use tags (``x-gmeow-*``), loads the
``gmeow:languageTag → gmeow:bcp47Tag`` mapping from a graph, and deterministically
retags literals for public-facing outputs.

Principle 4 (one canonical source) + Principle 9 (co-equal, non-privileged facets):
canonical authored literals use internal tags; public projections emit BCP-47.
"""

from __future__ import annotations

import re
from functools import lru_cache

from rdflib import RDF, RDFS, Graph, Literal, URIRef
from rdflib.namespace import DCTERMS, SKOS

from gmeow_tools.config import NAMESPACE

GM = URIRef(NAMESPACE)

#: GMEOW-internal language tag pattern (BCP-47 private-use subtag).
_INTERNAL_TAG_RE = re.compile(r"^x-gmeow-[a-z0-9\-]+$", re.IGNORECASE)

#: Standard annotation predicates whose literals are subject to the policy.
_ANNOTATION_PREDICATES: frozenset[URIRef] = frozenset(
    {
        RDFS.label,
        SKOS.definition,
        RDFS.comment,
        DCTERMS.title,
        DCTERMS.description,
    }
)


def is_internal_tag(lang: str | None) -> bool:
    """Return whether *lang* is a GMEOW internal private-use tag."""
    if lang is None:
        return False
    return bool(_INTERNAL_TAG_RE.match(lang))


def _language_tag_iri() -> URIRef:
    return URIRef(NAMESPACE + "languageTag")


def _bcp47_tag_iri() -> URIRef:
    return URIRef(NAMESPACE + "bcp47Tag")


def _language_class_iri() -> URIRef:
    return URIRef(NAMESPACE + "Language")


def load_tag_map(graph: Graph) -> dict[str, str]:
    """Build a mapping from internal language tag to BCP-47 tag.

    Scans every ``gmeow:Language`` in *graph* for ``gmeow:languageTag`` and
    ``gmeow:bcp47Tag`` values. Returns ``{internal_tag: bcp47_tag}``.

    Args:
        graph: An rdflib graph containing GMEOW language individuals.

    Returns:
        A dict mapping internal tag strings to BCP-47 tag strings.
    """
    lang_cls = _language_class_iri()
    tag_prop = _language_tag_iri()
    bcp_prop = _bcp47_tag_iri()

    tag_map: dict[str, str] = {}
    for lang in graph.subjects(RDF.type, lang_cls):
        if not isinstance(lang, URIRef):
            continue
        int_lit = graph.value(lang, tag_prop)
        bcp_lit = graph.value(lang, bcp_prop)
        if isinstance(int_lit, Literal) and isinstance(bcp_lit, Literal):
            tag_map[str(int_lit)] = str(bcp_lit)
    return tag_map


@lru_cache(maxsize=1)
def _default_tag_map() -> dict[str, str]:
    """Load the tag map from the merged ontology graph (cached)."""
    from gmeow_tools.graph import load_merged_graph

    return load_tag_map(load_merged_graph())


def retag_literal(lit: Literal, tag_map: dict[str, str] | None = None) -> Literal:
    """Retag a literal from internal to BCP-47 when a mapping exists.

    If ``lit.language`` is an internal tag and *tag_map* contains a mapping,
    returns a new ``Literal`` with the BCP-47 tag and the same lexical value.
    Otherwise returns *lit* unchanged.

    Args:
        lit: The source literal.
        tag_map: Optional pre-loaded tag map. If omitted, loads from the merged
            ontology graph.

    Returns:
        A retagged literal or the original.
    """
    if tag_map is None:
        tag_map = _default_tag_map()
    if not lit.language or not is_internal_tag(lit.language):
        return lit
    bcp = tag_map.get(lit.language)
    if bcp is None:
        return lit
    return Literal(str(lit), lang=bcp)


def rank_language(lang: str | None) -> tuple[int, str]:
    """The shared language-preference sort key (term-representation agnostic).

    Deterministic across multilingual labels (#287: the seed languages
    introduced fr/zh labels): the artifact carrier language
    (``x-gmeow-english``) wins, then the remaining tags in lexicographic
    order — never graph order. One function, used by BOTH the rdflib path
    (:func:`public_literal`) and the GTS fold view (#267 narrow waist), so
    their selections agree by construction.
    """
    norm = (lang or "").lower()
    return (0 if norm == "x-gmeow-english" else 1, norm)


def public_literal(
    graph: Graph,
    subject: URIRef,
    predicate: URIRef,
    *,
    tag_map: dict[str, str] | None = None,
) -> Literal | None:
    """Select the public-facing literal for *subject*/*predicate*.

    Preference order:

    1. An internal-tagged literal that has a BCP-47 mapping → retagged.
    2. Any other literal (external tag or untagged) → returned as-is.
    3. No literal → ``None``.

    This is deterministic: internal-mapped literals win because they are the
    canonical GMEOW-authored values; everything else is a fallback.

    Args:
        graph: The data graph.
        subject: The subject node.
        predicate: The predicate to look up.
        tag_map: Optional pre-loaded tag map.

    Returns:
        The selected literal (possibly retagged) or ``None``.
    """
    if tag_map is None:
        tag_map = _default_tag_map()

    candidates: list[Literal] = []
    for obj in graph.objects(subject, predicate):
        if isinstance(obj, Literal):
            candidates.append(obj)

    if not candidates:
        return None

    # Pre-sort by (language, lexical) so ties WITHIN a language resolve
    # deterministically (sorted() is stable; raw candidate order is rdflib
    # iteration order, which is process-unstable) — and identically to the
    # fold path's FoldView.public_text.
    candidates.sort(key=lambda lit: (lit.language or "", str(lit)))
    for lit in sorted(candidates, key=lambda lit: rank_language(lit.language)):
        if lit.language and is_internal_tag(lit.language):
            bcp = tag_map.get(lit.language)
            if bcp is not None:
                return Literal(str(lit), lang=bcp)

    # Fall back to the first candidate after deterministic sorting.
    # rdflib iteration order is stable within a process but can vary across
    # processes due to PYTHONHASHSEED, so we sort by (language, value).
    candidates.sort(key=lambda lit: (lit.language or "", str(lit)))
    return candidates[0]


def public_text(
    graph: Graph,
    subject: URIRef,
    predicate: URIRef,
    *,
    tag_map: dict[str, str] | None = None,
) -> str:
    """Return the string value of the public-facing literal, or ``""``.

    Thin wrapper around :func:`public_literal`.
    """
    lit = public_literal(graph, subject, predicate, tag_map=tag_map)
    return str(lit) if lit is not None else ""


def check_annotation_literal(
    subject: URIRef,
    predicate: URIRef,
    obj: Literal,
) -> str | None:
    """Return an error message if *obj* violates the language-tag policy.

    Checks whether a literal on a standard annotation predicate carries an
    external (non-internal) language tag.  GMEOW-authored terms must use
    ``x-gmeow-*`` for language-tagged literals.

    Args:
        subject: The triple subject.
        predicate: The triple predicate.
        obj: The triple object (must be a ``Literal``).

    Returns:
        An error message string, or ``None`` if the literal is acceptable.
    """
    if not isinstance(obj, Literal) or not obj.language:
        return None
    if is_internal_tag(obj.language):
        return None

    ns = str(predicate)
    # Already checked by the GMEOW-predicate branch in structural_lint.
    if ns.startswith(NAMESPACE):
        return None

    if predicate not in _ANNOTATION_PREDICATES:
        return None

    return (
        f"literal {obj!r} (on subject {subject}, predicate {predicate}) carries "
        f"external language tag '{obj.language}'; GMEOW-authored terms must use "
        f"the private-use 'x-gmeow-' prefix on standard annotation predicates."
    )


def retag_graph(graph: Graph, *, tag_map: dict[str, str] | None = None) -> Graph:
    """Retag every internal-tagged literal in *graph* to its public BCP-47 form.

    The projection-boundary pass (#287): generated consumer artifacts carry
    standard tags; internal ``x-gmeow-*`` tags exist only in canonical
    sources (and the statements compilation, which is the canonical form).
    Mutates and returns *graph*.
    """
    if tag_map is None:
        tag_map = _default_tag_map()
    swaps = []
    for s_, p_, o_ in graph:
        if isinstance(o_, Literal) and o_.language and is_internal_tag(o_.language):
            swaps.append((s_, p_, o_, retag_literal(o_, tag_map=tag_map)))
    for s_, p_, old, new in swaps:
        if new != old:
            graph.remove((s_, p_, old))
            graph.add((s_, p_, new))
    return graph
