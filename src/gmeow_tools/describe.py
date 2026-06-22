# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""``gmeow describe <term>`` — useful prose, not an RDF blob (#325).

A renderer composing existing structure with the Tier-1 documentation
properties: definition, gUFO stereotype, domain/range, owning slice + tier,
SSSOM alignments, scope/avoid notes, worked example, the
flat-first/reify-on-demand pairing (``gmeow:pairsWith``), and the pointer to
the slice's Tier-2 guide. Works offline against any ``.gts`` file (the
documentation rides the package — Principle 14; ``describe`` is a
five-minute-gate surface — Principle 13).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

from gmeow_rdf.compat.rdflib import RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.language_tags import (
    UnknownLanguageError,
    filter_literals,
    load_tag_map,
    resolve_lang_input,
    select_literal,
)

if TYPE_CHECKING:
    from gmeow_tools.language_tags import LangSelector

GM = Namespace(NAMESPACE)
GUFO = Namespace("http://purl.org/nemo/gufo#")

_GUFO_STEREOTYPES = (
    "Kind",
    "SubKind",
    "Category",
    "Role",
    "Phase",
    "Mixin",
    "RoleMixin",
    "PhaseMixin",
    "AbstractIndividualType",
    "EventType",
    "SituationType",
)


@dataclass
class TermCard:
    """Everything ``describe`` knows about one term, render-ready."""

    iri: URIRef
    local: str
    label: str = ""
    label_fallback: bool = False
    kinds: list[str] = field(default_factory=list)
    stereotype: str = ""
    supers: list[str] = field(default_factory=list)
    slice_name: str = ""
    slice_tier: str = ""
    domain: str = ""
    range: str = ""
    definition: str = ""
    definition_fallback: bool = False
    scope_notes: list[str] = field(default_factory=list)
    examples: list[str] = field(default_factory=list)
    use_when: list[str] = field(default_factory=list)
    avoid_when: list[str] = field(default_factory=list)
    how_to_use: list[str] = field(default_factory=list)
    use_for_consumer: list[str] = field(default_factory=list)
    avoid_for_consumer: list[str] = field(default_factory=list)
    pairs_with: list[str] = field(default_factory=list)
    paired_from: list[str] = field(default_factory=list)
    box_roles: list[str] = field(default_factory=list)
    alignments: list[str] = field(default_factory=list)
    guide: str = ""


def load_graph_from_gts(
    path: Path, *, graph_names: set[str | None] | None = None
) -> Graph:
    """Offline mode: read a .gts package into an rdflib Graph.

    Uses the gts package's reader and defaults to the GTS default graph, which
    carries the authored import-free ontology. Pass ``graph_names`` to flatten
    a specific named graph such as bundled self-description metadata.
    """
    from gts import read
    from gts.nquads import term_token

    payload = read(path.read_bytes())
    scopes = {None} if graph_names is None else graph_names
    lines: list[str] = []
    for s, p, o, graph_id in payload.quads:
        scope = payload.terms[graph_id].value if graph_id is not None else None
        if scope not in scopes:
            continue
        triple = (
            f"{term_token(payload, s)} {term_token(payload, p)} "
            f"{term_token(payload, o)}"
        )
        # rdflib cannot parse RDF 1.2 quoted triple terms; describe/crossref do
        # not render them from this compatibility graph.
        if "<<(" in triple or ")>>" in triple:
            continue
        lines.append(f"{triple} .")
    graph = Graph()
    if lines:
        graph.parse(data="\n".join(lines), format="nt")
    return graph


def resolve_term(graph: Graph, query: str) -> tuple[URIRef | None, list[str]]:
    """Resolve user input to a GMEOW term IRI.

    Accepts ``gmeow:X``, a full IRI, a bare local name, or a
    case-insensitive prefix; returns ``(term, candidates)`` where candidates
    is non-empty on ambiguity or no-match-with-suggestions.
    """
    text = query.strip()
    if text.startswith("gmeow:"):
        text = text[len("gmeow:") :]
    if text.startswith(NAMESPACE):
        text = text[len(NAMESPACE) :]
    if not text:
        # An empty query would prefix-match everything (startswith("")).
        return None, []
    locals_in_graph = sorted(
        {
            str(s)[len(NAMESPACE) :]
            for s in graph.subjects(RDFS.isDefinedBy, None)
            if isinstance(s, URIRef)
            and str(s).startswith(NAMESPACE)
            and "/" not in str(s)[len(NAMESPACE) :]
        }
    )
    if text in locals_in_graph:
        return URIRef(NAMESPACE + text), []
    exact_ci = [name for name in locals_in_graph if name.lower() == text.lower()]
    if len(exact_ci) == 1:
        return URIRef(NAMESPACE + exact_ci[0]), []
    prefix = [name for name in locals_in_graph if name.lower().startswith(text.lower())]
    if len(prefix) == 1:
        return URIRef(NAMESPACE + prefix[0]), []
    return None, (exact_ci or prefix)[:10]


def _short(node: object) -> str:
    if isinstance(node, URIRef):
        text = str(node)
        if text.startswith(NAMESPACE):
            return "gmeow:" + text[len(NAMESPACE) :]
        if text.startswith(str(GUFO)):
            return "gufo:" + text[len(str(GUFO)) :]
        return text
    return str(node)


def _selected_texts(
    graph: Graph,
    term: URIRef,
    predicate: URIRef,
    selector: LangSelector,
    tag_map: dict[str, str],
) -> list[str]:
    """Language-selected string values for a multi-valued annotation predicate."""
    literals = [o for o in graph.objects(term, predicate) if isinstance(o, Literal)]
    if not literals:
        return []
    return [
        str(lit)
        for lit, _fallback in filter_literals(literals, selector, tag_map=tag_map)
    ]


def _selected_single(
    graph: Graph,
    term: URIRef,
    predicate: URIRef,
    selector: LangSelector,
    tag_map: dict[str, str],
) -> tuple[str, bool]:
    """Language-selected single value plus fallback signal."""
    literals = [o for o in graph.objects(term, predicate) if isinstance(o, Literal)]
    lit, fallback = select_literal(literals, selector, tag_map=tag_map)
    return (str(lit) if lit is not None else ""), fallback


def build_card(
    graph: Graph,
    term: URIRef,
    *,
    selector: LangSelector,
    tag_map: dict[str, str],
) -> TermCard:
    """Compose the term card from the graph + SSSOM mappings."""
    local = str(term)[len(NAMESPACE) :]
    card = TermCard(iri=term, local=local)
    card.label, card.label_fallback = _selected_single(
        graph, term, RDFS.label, selector, tag_map
    )
    if not card.label:
        card.label = local
        card.label_fallback = False
    types = set(graph.objects(term, RDF.type))
    if OWL.Class in types:
        card.kinds.append("class")
    for t, kind in (
        (OWL.ObjectProperty, "object property"),
        (OWL.DatatypeProperty, "datatype property"),
        (OWL.AnnotationProperty, "annotation property"),
        (RDFS.Datatype, "datatype"),
    ):
        if t in types:
            card.kinds.append(kind)
    if not card.kinds:
        card.kinds.append("individual")
    for node in types:
        if isinstance(node, URIRef) and str(node).startswith(str(GUFO)):
            name = str(node)[len(str(GUFO)) :]
            if name in _GUFO_STEREOTYPES:
                card.stereotype = "gufo:" + name
    for pred in (RDFS.subClassOf, RDFS.subPropertyOf):
        for sup in graph.objects(term, pred):
            if isinstance(sup, URIRef):
                card.supers.append(_short(sup))
    card.supers = sorted(set(card.supers))
    card.domain = ", ".join(
        sorted(
            _short(d) for d in graph.objects(term, RDFS.domain) if isinstance(d, URIRef)
        )
    )
    card.range = ", ".join(
        sorted(
            _short(r) for r in graph.objects(term, RDFS.range) if isinstance(r, URIRef)
        )
    )
    card.definition, card.definition_fallback = _selected_single(
        graph, term, SKOS.definition, selector, tag_map
    )
    card.scope_notes = sorted(
        _selected_texts(graph, term, SKOS.scopeNote, selector, tag_map)
    )
    card.examples = sorted(
        _selected_texts(graph, term, SKOS.example, selector, tag_map)
    )
    card.use_when = sorted(_selected_texts(graph, term, GM.useWhen, selector, tag_map))
    card.avoid_when = sorted(
        _selected_texts(graph, term, GM.avoidWhen, selector, tag_map)
    )
    card.how_to_use = sorted(
        _selected_texts(graph, term, GM.howToUse, selector, tag_map)
    )
    card.use_for_consumer = sorted(
        _short(o) for o in graph.objects(term, GM.useForConsumer)
    )
    card.avoid_for_consumer = sorted(
        _short(o) for o in graph.objects(term, GM.avoidForConsumer)
    )
    card.pairs_with = sorted(_short(o) for o in graph.objects(term, GM.pairsWith))
    card.paired_from = sorted(_short(s) for s in graph.subjects(GM.pairsWith, term))
    card.box_roles = sorted({_short(o) for o in graph.objects(term, GM.graphBoxRole)})

    defined_by = graph.value(term, RDFS.isDefinedBy)
    if defined_by is not None:
        slice_iri = str(defined_by)
        card.slice_name = slice_iri.rstrip("/").rsplit("/", 1)[-1]
        try:
            from gmeow_tools.slices import discover_slices

            registry = discover_slices()
            entry = registry.get(slice_iri)
            if entry is not None:
                card.slice_tier = entry.tier
                # The discovered checkout path is authoritative — directory
                # grouping is organizational, never derived from tier
                # (the slices.py contract; PR #392 review).
                guide_path = entry.path / "docs.md"
                try:
                    from gmeow_tools.config import PROJECT_ROOT

                    card.guide = str(guide_path.relative_to(PROJECT_ROOT))
                except ValueError:
                    card.guide = str(guide_path)
        except Exception:
            card.guide = ""
    try:
        from gmeow_tools.mappings import load_mappings

        curie = f"gmeow:{local}"
        for mapping in load_mappings():
            if mapping.subject_id == curie:
                conf = f" ({mapping.confidence})" if mapping.confidence else ""
                card.alignments.append(
                    f"{mapping.predicate_id} {mapping.object_id}{conf}"
                )
    except Exception:
        pass
    card.alignments = sorted(set(card.alignments))
    return card


def render_card(card: TermCard) -> str:
    """Terminal markdown for one term — the issue's target shape."""
    head_right = " · ".join(
        part
        for part in (
            f"{card.slice_tier}/{card.slice_name}" if card.slice_name else "",
            card.stereotype,
        )
        if part
    )
    label_fallback = " [dim](fallback: en)[/dim]" if card.label_fallback else ""
    lines = [
        f"[bold]gmeow:{card.local}[/bold]{label_fallback}    {head_right}".rstrip()
    ]
    meta = " · ".join(card.kinds)
    if card.supers:
        meta += "  ⊑ " + ", ".join(card.supers)
    lines.append(f"  [dim]{meta}[/dim]")
    if card.domain or card.range:
        dr = []
        if card.domain:
            dr.append(f"domain {card.domain}")
        if card.range:
            dr.append(f"range {card.range}")
        lines.append(f"  [dim]{' · '.join(dr)}[/dim]")
    if card.definition:
        fallback_note = " [dim](fallback: en)[/dim]" if card.definition_fallback else ""
        lines.append(f"  {card.definition}{fallback_note}")
    for note in card.scope_notes:
        lines.append(f"  [yellow]Scope[/yellow]  {note}")
    for example in card.examples:
        lines.append(f"  [cyan]Example[/cyan]  {example}")
    for note in card.use_when:
        lines.append(f"  [yellow]Use when[/yellow]  {note}")
    for note in card.avoid_when:
        lines.append(f"  [yellow]Avoid when[/yellow]  {note}")
    for note in card.how_to_use:
        lines.append(f"  [cyan]How[/cyan]  {note}")
    if card.use_for_consumer:
        lines.append("  [green]Use for[/green]  " + " · ".join(card.use_for_consumer))
    if card.avoid_for_consumer:
        lines.append(
            "  [green]Avoid for[/green]  " + " · ".join(card.avoid_for_consumer)
        )
    for target in card.pairs_with:
        lines.append(
            f"  [magenta]Pairs with[/magenta]  {target} — promote when "
            f"period, role, confidence, or standpoint must be first-class"
        )
    for source in card.paired_from:
        lines.append(f"  [magenta]Flat form[/magenta]  {source} — the 80% shortcut")
    if card.box_roles:
        lines.append("  [blue]Box roles[/blue]  " + " · ".join(card.box_roles))
    if card.alignments:
        lines.append("  [green]Aligned[/green]  " + " · ".join(card.alignments[:8]))
    if card.guide:
        lines.append(f"  [dim]Guide: {card.guide}[/dim]")
    return "\n".join(lines)


def describe(
    query: str,
    gts_path: Path | None = None,
    *,
    selector: LangSelector | None = None,
) -> tuple[str, int]:
    """Resolve + render; returns (text, exit_code)."""
    if gts_path is not None:
        if not gts_path.exists():
            return (f"GTS package file not found: {gts_path}", 1)
        graph = load_graph_from_gts(gts_path)
    else:
        from gmeow_tools.graph import load_merged_graph

        graph = load_merged_graph(include_imports=False)
    term, candidates = resolve_term(graph, query)
    if term is None:
        if candidates:
            options = "\n  ".join(f"gmeow:{c}" for c in candidates)
            return (
                f"ambiguous or unknown term '{query}' — candidates:\n  {options}",
                1,
            )
        return (f"no GMEOW term matches '{query}'", 1)

    tag_map = load_tag_map(graph)
    if selector is None:
        selector = resolve_lang_input(None, tag_map)
    try:
        return (
            render_card(build_card(graph, term, selector=selector, tag_map=tag_map)),
            0,
        )
    except UnknownLanguageError as exc:
        return (str(exc), 1)
