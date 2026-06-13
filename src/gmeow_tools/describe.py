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

from rdflib import RDF, RDFS, Graph, Namespace, URIRef
from rdflib.namespace import OWL, SKOS

from gmeow_tools.config import NAMESPACE

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
    kinds: list[str] = field(default_factory=list)
    stereotype: str = ""
    supers: list[str] = field(default_factory=list)
    slice_name: str = ""
    slice_tier: str = ""
    domain: str = ""
    range: str = ""
    definition: str = ""
    scope_notes: list[str] = field(default_factory=list)
    examples: list[str] = field(default_factory=list)
    pairs_with: list[str] = field(default_factory=list)
    paired_from: list[str] = field(default_factory=list)
    alignments: list[str] = field(default_factory=list)
    guide: str = ""


def load_graph_from_gts(path: Path) -> Graph:
    """Offline mode: read a .gts package into an rdflib Graph.

    Uses the gts package's reader + N-Quads view; the default graph carries
    the ontology, which is all ``describe`` needs.
    """
    from gts import read, to_nquads

    payload = read(path.read_bytes())
    # describe needs the plain ontology union; the RDF 1.2 statement layer's
    # quoted-triple terms (<<( ... )>>) are not rdflib-nquads-parseable and
    # carry nothing describe renders — filter those rows out.
    lines = [
        line
        for line in to_nquads(payload).splitlines()
        if "<<(" not in line and ")>>" not in line
    ]
    from rdflib import Dataset

    # Dataset (not the deprecated ConjunctiveGraph); flatten every quad's triple
    # into a plain Graph via quads(). NB: rdflib's own nquads parser internally
    # touches the deprecated Dataset.default_context — an upstream self-
    # deprecation we cannot avoid here; it is filtered in pyproject.
    ds = Dataset()
    ds.parse(data="\n".join(lines), format="nquads")
    graph = Graph()
    for s, p, o, _ctx in ds.quads((None, None, None, None)):
        graph.add((s, p, o))
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


def build_card(graph: Graph, term: URIRef) -> TermCard:
    """Compose the term card from the graph + SSSOM mappings."""
    local = str(term)[len(NAMESPACE) :]
    card = TermCard(iri=term, local=local)
    card.label = str(graph.value(term, RDFS.label) or local)
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
    card.definition = str(graph.value(term, SKOS.definition) or "")
    card.scope_notes = [str(o) for o in graph.objects(term, SKOS.scopeNote)]
    card.examples = [str(o) for o in graph.objects(term, SKOS.example)]
    card.pairs_with = sorted(_short(o) for o in graph.objects(term, GM.pairsWith))
    card.paired_from = sorted(_short(s) for s in graph.subjects(GM.pairsWith, term))

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
    lines = [f"[bold]gmeow:{card.local}[/bold]    {head_right}".rstrip()]
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
        lines.append(f"  {card.definition}")
    for note in card.scope_notes:
        lines.append(f"  [yellow]Scope[/yellow]  {note}")
    for example in card.examples:
        lines.append(f"  [cyan]Example[/cyan]  {example}")
    for target in card.pairs_with:
        lines.append(
            f"  [magenta]Pairs with[/magenta]  {target} — promote when "
            f"period, role, confidence, or standpoint must be first-class"
        )
    for source in card.paired_from:
        lines.append(f"  [magenta]Flat form[/magenta]  {source} — the 80% shortcut")
    if card.alignments:
        lines.append("  [green]Aligned[/green]  " + " · ".join(card.alignments[:8]))
    if card.guide:
        lines.append(f"  [dim]Guide: {card.guide}[/dim]")
    return "\n".join(lines)


def describe(query: str, gts_path: Path | None = None) -> tuple[str, int]:
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
    return (render_card(build_card(graph, term)), 0)
