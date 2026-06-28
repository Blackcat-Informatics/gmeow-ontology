"""Shared language-tag policy helper.

Recognises GMEOW internal private-use tags (``x-gmeow-*``), loads the
``gmeow:languageTag → gmeow:bcp47Tag`` mapping from a graph, and deterministically
retags literals for public-facing outputs.

Principle 4 (one canonical source) + Principle 9 (co-equal, non-privileged facets):
canonical authored literals use internal tags; public projections emit BCP-47.

The policy is owned by the Rust ``gmeow_validate`` crate; this module is a thin UI
wrapper that marshals rdflib graphs and literals to and from the native authority
and reconstructs rdflib objects from its verdicts. No selection, filtering, or
retagging decision is made in Python.
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass
from functools import lru_cache

import gmeow_validate
from gmeow_rdf.compat.rdflib import Graph, Literal, URIRef

from gmeow_tools.config import NAMESPACE

GM = URIRef(NAMESPACE)


@lru_cache(maxsize=1)
def _annotation_predicates() -> frozenset[URIRef]:
    """Standard annotation predicates whose literals are subject to the policy.

    The registry is owned by the Rust validate crate; this reads it back through
    the native ``gmeow_validate.annotation_predicates`` surface so there is a
    single source of truth instead of a parallel Python constant.
    """
    return frozenset(URIRef(p) for p in gmeow_validate.annotation_predicates())


def is_internal_tag(lang: str | None) -> bool:
    """Return whether *lang* is a GMEOW internal private-use tag.

    Delegates to the Rust authority in ``gmeow_validate``.
    """
    if lang is None:
        return False
    return gmeow_validate.is_internal_tag(lang)


def _graph_nt(graph: Graph) -> bytes:
    """Serialise *graph* to N-Triples bytes for the native policy boundary."""
    return graph.serialize(format="ntriples").encode()


def load_tag_map(graph: Graph) -> dict[str, str]:
    """Build a mapping from internal language tag to BCP-47 tag.

    Scans ``gmeow:Language`` and its explicit subclasses
    (``gmeow:FormalLanguage``, ``gmeow:ProgrammingLanguage``) for
    ``gmeow:languageTag`` / ``gmeow:bcp47Tag`` pairs. The scan and validation are
    performed by the Rust authority ``gmeow_validate.load_tag_map`` via N-Triples
    serialization; Python only marshals the graph.

    Args:
        graph: An rdflib graph containing GMEOW language individuals.

    Returns:
        A dict mapping internal tag strings to BCP-47 tag strings.
    """
    return gmeow_validate.load_tag_map(_graph_nt(graph), "ntriples")


@lru_cache(maxsize=1)
def _default_tag_map() -> dict[str, str]:
    """Load the tag map from the merged ontology graph (cached)."""
    from gmeow_tools.graph import load_merged_graph

    return load_tag_map(load_merged_graph())


def rank_language(lang: str | None) -> tuple[int, str]:
    """The shared language-preference sort key (term-representation agnostic).

    Deterministic across multilingual labels: the artifact carrier language
    (``x-gmeow-english``) wins, then the remaining tags in lexicographic order —
    never graph order. One function, used by BOTH the rdflib path
    (:func:`public_literal`) and the GTS fold view (the narrow waist), so their
    selections agree by construction.

    Delegates to the Rust authority in ``gmeow_validate``.
    """
    return gmeow_validate.rank_language(lang or "")


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
    canonical GMEOW-authored values; everything else is a fallback. The ranking
    and internal-tag tests bottom out in the Rust authority
    (``rank_language`` / ``is_internal_tag``).

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

    if predicate not in _annotation_predicates():
        return None

    return (
        f"literal {obj!r} (on subject {subject}, predicate {predicate}) carries "
        f"external language tag '{obj.language}'; GMEOW-authored terms must use "
        f"the private-use 'x-gmeow-' prefix on standard annotation predicates."
    )


_PROBE_SUBJECT = "https://blackcatinformatics.ca/gmeow/_lang-probe"
_PROBE_PREDICATE = "https://blackcatinformatics.ca/gmeow/_lang-probe-of"


def _lang_remap(
    languages: Iterable[str | None],
    native_pass: object,
    *args: object,
) -> dict[str, str]:
    """Ask the Rust authority for the per-language retag of a literal-only pass.

    ``retag_graph`` / ``retag_graph_to_internal`` rewrite a literal's language tag
    as a function of that tag alone (subject/predicate/lexical are untouched). To
    keep the decision in Rust while mutating literals in place — leaving every
    blank node and structural triple untouched — the distinct languages are sent
    through a synthetic one-triple-per-language probe graph, the native pass is
    run, and the result is read back as ``{old_lang: new_lang}`` for the tags the
    authority actually changed.
    """
    distinct = sorted({lang for lang in languages if lang})
    if not distinct:
        return {}
    # One probe triple per distinct old language: the native pass rewrites each
    # one independently of the others (per-tag function), so reading the image of
    # the single probe literal back yields the exact ``old -> new`` retag the
    # authority chose. Only tags it actually changed are recorded.
    remap: dict[str, str] = {}
    probe_s = URIRef(_PROBE_SUBJECT)
    probe_p = URIRef(_PROBE_PREDICATE)
    for old in distinct:
        single = f'<{_PROBE_SUBJECT}> <{_PROBE_PREDICATE}> "probe"@{old} .\n'
        out = Graph()
        out.parse(
            data=native_pass(single.encode(), "ntriples", *args),  # type: ignore[operator]
            format="ntriples",
        )
        for obj in out.objects(probe_s, probe_p):
            if isinstance(obj, Literal) and obj.language and obj.language != old:
                remap[old] = obj.language
    return remap


def _apply_remap(graph: Graph, remap: dict[str, str]) -> Graph:
    """Swap every language-tagged literal in *graph* whose tag is in *remap*.

    Only literal objects are touched; subjects (including blank nodes), predicates,
    and lexical forms are left exactly as they are, so no blank node is relabeled.
    """
    if not remap:
        return graph
    swaps = []
    for s_, p_, o_ in graph:
        if isinstance(o_, Literal) and o_.language and o_.language in remap:
            new = Literal(str(o_), lang=remap[o_.language])
            if new != o_:
                swaps.append((s_, p_, o_, new))
    for s_, p_, old, new in swaps:
        graph.remove((s_, p_, old))
        graph.add((s_, p_, new))
    return graph


def retag_graph(graph: Graph, *, tag_map: dict[str, str] | None = None) -> Graph:
    """Retag every internal-tagged literal in *graph* to its public BCP-47 form.

    The projection-boundary pass: generated consumer artifacts carry standard
    tags; internal ``x-gmeow-*`` tags exist only in canonical sources (and the
    statements compilation, which is the canonical form). Each per-language retag
    decision is the Rust authority's (``gmeow_validate.retag_graph``); Python only
    applies the resulting ``{old_lang: new_lang}`` swap in place, touching literal
    objects only so blank nodes and structure are preserved. Mutates and returns
    *graph*.
    """
    if tag_map is None:
        tag_map = _default_tag_map()
    remap = _lang_remap(
        (o.language for _s, _p, o in graph if isinstance(o, Literal)),
        gmeow_validate.retag_graph,
        tag_map,
    )
    return _apply_remap(graph, remap)


def load_inverse_tag_map(graph: Graph) -> dict[str, str]:
    """Build the BCP-47 → internal mapping — the inverse of :func:`load_tag_map`.

    Built from **natural** ``gmeow:Language`` individuals only: a programming
    language's code carries an ``en`` BCP-47 tag too, so including them would make
    the ``en`` reverse ambiguous — but a consumer *prose* ``@en`` literal is
    natural English. A BCP-47 tag that several natural languages still share is
    dropped rather than guessed (the no-fabrication discipline). Keys are
    lowercased to match rdflib's normalized ``Literal.language``. Delegates to the
    Rust authority ``gmeow_validate.load_inverse_tag_map``.
    """
    return gmeow_validate.load_inverse_tag_map(_graph_nt(graph), "ntriples")


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
    (the invertible-FnO ``fnComposeBcp`` read backwards): a consumer source
    carries public tags (``en``/``fr``/``zh``), but the pure-GMEOW intermediate
    is canonical, so an ``@en`` literal becomes ``@x-gmeow-english``. A public tag
    with no internal counterpart (no GMEOW language individual) is left as-is. The
    retag decision is the Rust authority's; Python only marshals and reparses.
    Mutates and returns *graph*.
    """
    if tag_map is None:
        tag_map = _default_inverse_tag_map()
    remap = _lang_remap(
        (o.language for _s, _p, o in graph if isinstance(o, Literal)),
        gmeow_validate.retag_graph_to_internal,
        tag_map,
    )
    return _apply_remap(graph, remap)


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


def resolve_lang_input(
    raw: str | None,
    tag_map: dict[str, str],
    *,
    available: Iterable[str] | None = None,
) -> LangSelector:
    """Resolve CLI/env language input into a :class:`LangSelector`.

    * ``None``/empty → default ``(en,)``.
    * Internal tags (``x-gmeow-english``) are normalized to their BCP-47 form.
    * Public BCP-47 tags are lower-cased.
    * Comma-separated lists preserve order and are de-duplicated.
    * Unknown tags raise :class:`UnknownLanguageError` with the available list.

    The resolution is the Rust authority's (``gmeow_validate.resolve_lang_input``);
    Python only reconstructs the :class:`LangSelector` and raises on the reported
    unknown tag.

    Args:
        raw: The raw language request, e.g. ``"fr,en"`` or ``None``.
        tag_map: Mapping from internal ``x-gmeow-*`` tags to public BCP-47.
        available: Optional set of allowed public BCP-47 tags. When omitted,
            the values of *tag_map* are used (the full mapped catalog).
    """
    requested, avail, unknown = gmeow_validate.resolve_lang_input(
        raw, tag_map, list(available) if available is not None else None
    )
    if unknown is not None:
        raise UnknownLanguageError(unknown, avail)
    return LangSelector(requested=tuple(requested), available=frozenset(avail))


def select_literal(
    literals: Iterable[Literal],
    selector: LangSelector,
    *,
    tag_map: dict[str, str] | None = None,
) -> tuple[Literal | None, bool]:
    """Select the single best literal for *selector*.

    Returns ``(literal, is_fallback)``. The literal is retagged to its public
    BCP-47 form. Requested languages are tried in order; if none match, the
    English carrier language is returned as a fallback. The selection — index,
    retag tag, and fallback flag — is the Rust authority's
    (``gmeow_validate.select_literal``); Python only reconstructs the rdflib
    literal, preserving the original object (and its datatype) when no retag
    applies.
    """
    if tag_map is None:
        tag_map = _default_tag_map()

    candidates = list(literals)
    if not candidates:
        return None, False

    pairs = [(str(lit), lit.language) for lit in candidates]
    res = gmeow_validate.select_literal(pairs, list(selector.requested), tag_map)
    if res is None:
        return None, False
    index, retag, is_fallback = res
    lit = candidates[index]
    out = Literal(str(lit), lang=retag) if retag else lit
    return out, is_fallback


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
    tagged literal is returned as fallback. The selection is the Rust authority's
    (``gmeow_validate.filter_literals``); Python only reconstructs the rdflib
    literals, preserving the original object when no retag applies.
    """
    if tag_map is None:
        tag_map = _default_tag_map()

    candidates = list(literals)
    if not candidates:
        return []

    pairs = [(str(lit), lit.language) for lit in candidates]
    results: list[tuple[Literal, bool]] = []
    for index, retag, is_fallback in gmeow_validate.filter_literals(
        pairs, list(selector.requested), tag_map
    ):
        lit = candidates[index]
        out = Literal(str(lit), lang=retag) if retag else lit
        results.append((out, is_fallback))
    return results


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
    whose predicate is not in *predicates* are left untouched. Both IRI and
    blank-node subjects are in scope.

    Every keep/drop/retag decision is fully delegated to the Rust authority via
    ``gmeow_validate.filter_literals``. Python only marshals literal descriptors
    per ``(subject, predicate)`` group, applies the returned verdicts in place on
    the original graph, and never makes an independent policy decision. Mutates
    and returns *graph*.
    """
    if tag_map is None:
        tag_map = _default_tag_map()
    target_preds = (
        set(predicates) if predicates is not None else _annotation_predicates()
    )

    for p_ in target_preds:
        # Collect all subjects (IRI or blank node) that have language-tagged
        # literal objects on this predicate.
        subjects = {
            s_
            for s_, _p, o_ in graph.triples((None, p_, None))
            if isinstance(o_, Literal) and o_.language
        }
        for s_ in subjects:
            # Materialize the list BEFORE building pairs so indices are stable.
            current_list = [
                o_
                for o_ in graph.objects(s_, p_)
                if isinstance(o_, Literal) and o_.language
            ]
            if not current_list:
                continue
            pairs = [(str(lit), lit.language) for lit in current_list]
            chosen: set[Literal] = set()
            for index, retag, _is_fallback in gmeow_validate.filter_literals(
                pairs, list(selector.requested), tag_map
            ):
                lit = current_list[index]
                out = Literal(str(lit), lang=retag) if retag else lit
                chosen.add(out)
            current = set(current_list)
            if current == chosen:
                continue
            for old in current - chosen:
                graph.remove((s_, p_, old))
            for new in sorted(chosen - current, key=str):
                graph.add((s_, p_, new))
    return graph
