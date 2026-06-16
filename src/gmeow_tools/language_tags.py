"""Shared language-tag policy helper.

Recognises GMEOW internal private-use tags (``x-gmeow-*``), loads the
``gmeow:languageTag → gmeow:bcp47Tag`` mapping from a graph, and deterministically
retags literals for public-facing outputs.

Principle 4 (one canonical source) + Principle 9 (co-equal, non-privileged facets):
canonical authored literals use internal tags; public projections emit BCP-47.
"""

from __future__ import annotations

import re
from collections.abc import Iterable
from dataclasses import dataclass
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


def _formal_language_class_iri() -> URIRef:
    return URIRef(NAMESPACE + "FormalLanguage")


def _programming_language_class_iri() -> URIRef:
    return URIRef(NAMESPACE + "ProgrammingLanguage")


class _MissingTagError(ValueError):
    """Raised when a required language tag property is absent."""


class _AmbiguousTagError(ValueError):
    """Raised when a language tag property has more than one distinct value."""


def load_tag_map(
    graph: Graph, *, classes: list[URIRef] | None = None
) -> dict[str, str]:
    """Build a mapping from internal language tag to BCP-47 tag.

    Scans ``gmeow:Language`` and its explicit subclasses
    (``gmeow:FormalLanguage``, ``gmeow:ProgrammingLanguage``) in *graph* by
    default. Because rdflib does not perform OWL inference, each concrete class is
    queried directly and a seen-set deduplicates shared individuals.
    Returns ``{internal_tag: bcp47_tag}``.

    Args:
        graph: An rdflib graph containing GMEOW language individuals.
        classes: Restrict the scan to these language classes (default: all
            three). The inverse map uses natural ``gmeow:Language`` only — a
            programming language's code is tagged ``en`` too, so including them
            would make the ``en`` reverse ambiguous.

    Returns:
        A dict mapping internal tag strings to BCP-47 tag strings.
    """
    tag_prop = _language_tag_iri()
    bcp_prop = _bcp47_tag_iri()
    lang_classes = classes or [
        _language_class_iri(),
        _formal_language_class_iri(),
        _programming_language_class_iri(),
    ]

    def _single_value(subject: URIRef, predicate: URIRef) -> str:
        """Return the single lexical value for *predicate* on *subject*.

        ``rdflib.Graph.value`` is non-deterministic when multiple objects exist,
        so we collect all literal values, deduplicate by lexical form, and
        require exactly one distinct value. This keeps retagging deterministic
        and surfaces conflicting BCP-47 tags immediately.
        """
        values = sorted(
            {
                str(obj)
                for obj in graph.objects(subject, predicate)
                if isinstance(obj, Literal)
            }
        )
        if not values:
            raise _MissingTagError(f"{subject} has no value for {predicate}")
        if len(values) > 1:
            raise _AmbiguousTagError(
                f"{subject} has ambiguous values for {predicate}: {values}. "
                "Tag-map projection requires a single canonical value."
            )
        return values[0]

    tag_map: dict[str, str] = {}
    seen: set[URIRef] = set()
    for cls in lang_classes:
        for lang in graph.subjects(RDF.type, cls):
            if not isinstance(lang, URIRef) or lang in seen:
                continue
            seen.add(lang)
            try:
                int_val = _single_value(lang, tag_prop)
                bcp_val = _single_value(lang, bcp_prop)
            except _MissingTagError:
                # Skip language-like individuals that are missing one of the
                # required tags; the ontology/SHACL layers enforce completeness.
                continue
            tag_map[int_val] = bcp_val
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


def load_inverse_tag_map(graph: Graph) -> dict[str, str]:
    """Build the BCP-47 → internal mapping — the inverse of :func:`load_tag_map`.

    Built from **natural** ``gmeow:Language`` individuals only: a programming
    language's code carries an ``en`` BCP-47 tag too, so including them would make
    the ``en`` reverse ambiguous — but a consumer *prose* ``@en`` literal is
    natural English. A BCP-47 tag that several natural languages still share is
    dropped rather than guessed (the no-fabrication discipline). Keys are
    lowercased to match rdflib's normalized ``Literal.language``.
    """
    natural = load_tag_map(graph, classes=[_language_class_iri()])
    by_bcp: dict[str, set[str]] = {}
    for internal, bcp in natural.items():
        by_bcp.setdefault(bcp.lower(), set()).add(internal)
    return {bcp: next(iter(ints)) for bcp, ints in by_bcp.items() if len(ints) == 1}


@lru_cache(maxsize=1)
def _default_inverse_tag_map() -> dict[str, str]:
    """Load the inverse (BCP-47 → internal) tag map from the merged graph (cached)."""
    from gmeow_tools.graph import load_merged_graph

    return load_inverse_tag_map(load_merged_graph())


def retag_graph_to_internal(
    graph: Graph, *, tag_map: dict[str, str] | None = None
) -> Graph:
    """Retag every public BCP-47 literal to its canonical ``x-gmeow-*`` form.

    The **inverse** of :func:`retag_graph` — the up-projection boundary pass
    (#451 invertible-FnO, ``fnComposeBcp`` read backwards): a consumer source
    carries public tags (``en``/``fr``/``zh``), but the pure-GMEOW intermediate
    is canonical, so an ``@en`` literal becomes ``@x-gmeow-english``. A public tag
    with no internal counterpart (no GMEOW language individual) is left as-is.
    Mutates and returns *graph*.
    """
    if tag_map is None:
        tag_map = _default_inverse_tag_map()
    swaps = []
    for s_, p_, o_ in graph:
        if (
            isinstance(o_, Literal)
            and o_.language
            and not is_internal_tag(o_.language)
            and o_.language.lower() in tag_map
        ):
            new = Literal(str(o_), lang=tag_map[o_.language.lower()])
            swaps.append((s_, p_, o_, new))
    for s_, p_, old, new in swaps:
        if new != old:
            graph.remove((s_, p_, old))
            graph.add((s_, p_, new))
    return graph


class UnknownLanguageError(ValueError):
    """Raised when a requested language tag is not available in the tag map."""

    def __init__(self, tag: str, available: list[str]) -> None:
        """Build a helpful error with the list of available BCP-47 tags."""
        self.tag = tag
        self.available = available
        super().__init__(
            f"unknown language tag '{tag}'. Available languages: {', '.join(available)}"
        )


@dataclass(frozen=True, slots=True)
class LangSelector:
    """A resolved, validated user language request.

    Holds the requested BCP-47 tags in precedence order and the set of tags
    known to the current snapshot. All CLI/env resolution funnels through this
    object so the fold and rdflib paths agree by construction.
    """

    requested: tuple[str, ...]
    available: frozenset[str]

    def is_requested(self, bcp47: str | None) -> bool:
        """Whether *bcp47* (lower-cased) is one of the requested languages."""
        if bcp47 is None:
            return False
        return bcp47.lower() in self.requested

    def fallback_tag(self) -> str:
        """The carrier-language tag used when a requested literal is absent."""
        return "en"


def resolve_lang_input(raw: str | None, tag_map: dict[str, str]) -> LangSelector:
    """Resolve CLI/env language input into a :class:`LangSelector`.

    * ``None``/empty → default ``(en,)``.
    * Internal tags (``x-gmeow-english``) are normalized to their BCP-47 form.
    * Public BCP-47 tags are lower-cased.
    * Comma-separated lists preserve order and are de-duplicated.
    * Unknown tags raise :class:`UnknownLanguageError` with the available list.
    """
    available = frozenset(tag_map.values())
    if not raw or not raw.strip():
        return LangSelector(requested=("en",), available=available)

    tokens = [token.strip() for token in raw.split(",") if token.strip()]
    resolved: list[str] = []
    seen: set[str] = set()
    for token in tokens:
        if is_internal_tag(token):
            bcp = tag_map.get(token)
            if bcp is None:
                raise UnknownLanguageError(
                    token, sorted(available, key=lambda t: (t != "en", t))
                )
            normalized = bcp.lower()
        else:
            normalized = token.lower()
        if normalized not in seen:
            if normalized not in available:
                raise UnknownLanguageError(
                    token, sorted(available, key=lambda t: (t != "en", t))
                )
            seen.add(normalized)
            resolved.append(normalized)

    if not resolved:
        return LangSelector(requested=("en",), available=available)
    return LangSelector(requested=tuple(resolved), available=available)


def _bcp47_for_literal(lit: Literal, tag_map: dict[str, str]) -> str | None:
    """Return the public BCP-47 tag for a literal, if any."""
    lang = lit.language
    if not lang:
        return None
    if is_internal_tag(lang):
        return tag_map.get(lang, lang)
    return lang


def _retagged(lit: Literal, tag_map: dict[str, str]) -> Literal:
    """Return a BCP-47-retagged copy of *lit* if it carries an internal tag."""
    if not lit.language or not is_internal_tag(lit.language):
        return lit
    bcp = tag_map.get(lit.language)
    if bcp is None:
        return lit
    return Literal(str(lit), lang=bcp)


def select_literal(
    literals: Iterable[Literal],
    selector: LangSelector,
    *,
    tag_map: dict[str, str] | None = None,
) -> tuple[Literal | None, bool]:
    """Select the single best literal for *selector*.

    Returns ``(literal, is_fallback)``. The literal is retagged to its public
    BCP-47 form. Requested languages are tried in order; if none match, the
    English carrier language is returned as a fallback.
    """
    if tag_map is None:
        tag_map = _default_tag_map()

    candidates = list(literals)
    if not candidates:
        return None, False

    # Build public-tag index. Within each public tag, prefer internal-tagged
    # canonical literals (the carrier language wins) over external-tagged or
    # untagged co-existing values — same discipline as public_literal.
    by_bcp: dict[str, list[tuple[Literal, str]]] = {}
    for lit in candidates:
        bcp = _bcp47_for_literal(lit, tag_map)
        retagged = _retagged(lit, tag_map)
        orig_lang = lit.language or ""
        bucket = bcp.lower() if bcp is not None else ""
        by_bcp.setdefault(bucket, []).append((retagged, orig_lang))
    for key in by_bcp:
        by_bcp[key].sort(key=lambda item: (rank_language(item[1]), str(item[0])))

    for req in selector.requested:
        if req in by_bcp:
            return by_bcp[req][0][0], False

    # Fallback to English, then the deterministic first tagged literal, then
    # any untagged literal.
    if "en" in by_bcp:
        return by_bcp["en"][0][0], True

    tagged = sorted(
        (bcp, literal_lists[0]) for bcp, literal_lists in by_bcp.items() if bcp
    )
    if tagged:
        # Use the same rank_language ordering as public_literal for stability.
        best = min(tagged, key=lambda item: rank_language(item[0]))
        return best[1][0], True

    if "" in by_bcp:
        return by_bcp[""][0][0], True
    return None, False


def filter_literals(
    literals: Iterable[Literal],
    selector: LangSelector,
    *,
    tag_map: dict[str, str] | None = None,
) -> list[tuple[Literal, bool]]:
    """Return literals matching the requested languages, or the ``en`` fallback.

    Each result is ``(retagged_literal, is_fallback)``. If no requested language
    is present, the English carrier-language literal is returned with
    ``is_fallback=True``. If English is also absent, the deterministic first
    tagged literal is returned as fallback.
    """
    if tag_map is None:
        tag_map = _default_tag_map()

    candidates = list(literals)
    if not candidates:
        return []

    by_bcp: dict[str, list[tuple[Literal, str]]] = {}
    for lit in candidates:
        bcp = _bcp47_for_literal(lit, tag_map)
        retagged = _retagged(lit, tag_map)
        orig_lang = lit.language or ""
        bucket = bcp.lower() if bcp is not None else ""
        by_bcp.setdefault(bucket, []).append((retagged, orig_lang))
    for key in by_bcp:
        # Internal-tagged canonical literals sort first; within that, lexical
        # order keeps multi-valued advisories deterministic.
        by_bcp[key].sort(key=lambda item: (rank_language(item[1]), str(item[0])))

    results: list[tuple[Literal, bool]] = []
    for req in selector.requested:
        for lit, _orig in by_bcp.get(req, []):
            results.append((lit, False))

    if results:
        return results

    # Fallback chain.
    if "en" in by_bcp:
        return [(by_bcp["en"][0][0], True)]
    tagged = sorted(
        (bcp, literal_lists[0]) for bcp, literal_lists in by_bcp.items() if bcp
    )
    if tagged:
        best = min(tagged, key=lambda item: rank_language(item[0]))
        return [(best[1][0], True)]
    if "" in by_bcp:
        return [(by_bcp[""][0][0], True)]
    return []


def filter_graph(
    graph: Graph,
    selector: LangSelector,
    *,
    tag_map: dict[str, str] | None = None,
    predicates: Iterable[URIRef] | None = None,
) -> Graph:
    """Retain only language-selected literals for the given predicates.

    For every ``(s, p)`` where *p* is in *predicates* and the objects include
    language-tagged literals, the objects are replaced by the literals selected
    by *selector* (or the English fallback). Non-language objects and triples
    whose predicate is not in *predicates* are left untouched. Mutates and
    returns *graph*.
    """
    if tag_map is None:
        tag_map = _default_tag_map()
    target_preds = set(predicates) if predicates is not None else _ANNOTATION_PREDICATES

    # Group language-tagged objects by (s, p).
    grouped: dict[tuple[URIRef, URIRef], list[Literal]] = {}
    for s_, p_, o_ in graph:
        if (
            isinstance(s_, URIRef)
            and isinstance(p_, URIRef)
            and p_ in target_preds
            and isinstance(o_, Literal)
            and o_.language
        ):
            grouped.setdefault((s_, p_), []).append(o_)

    for (s_, p_), literals in grouped.items():
        selected = filter_literals(literals, selector, tag_map=tag_map)
        if not selected:
            continue
        # Only mutate if the chosen set differs from the current set.
        current = set(literals)
        chosen = {lit for lit, _fallback in selected}
        if chosen == current:
            continue
        for old in current:
            graph.remove((s_, p_, old))
        for lit in sorted(chosen, key=lambda lit: str(lit)):
            graph.add((s_, p_, lit))
    return graph
