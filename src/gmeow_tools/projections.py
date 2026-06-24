"""Project GMEOW data to pure target-vocabulary profiles via SPARQL CONSTRUCT.

The complex (non-1:1) correspondences are specified declaratively as FnO function
descriptions + EDOAL complex alignments under ``projections/``; this module is
their *executor* — it runs the per-profile CONSTRUCT queries
(``queries/projections/*.rq``) in-process via rdflib over the asserted ontology +
input data, emitting a pure-profile graph. The projections are deliberately
**lossy and directional** (a consumable view, never the canonical model), which is
why they live outside the reasoned core.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

import gmeow_rdf
from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools import sparql
from gmeow_tools.config import DIST_DIR, FIXTURES_DIR, PREFIXES, PROJECTION_QUERY_DIR
from gmeow_tools.language_tags import filter_graph, retag_graph

if TYPE_CHECKING:
    from gmeow_tools.language_tags import LangSelector


@dataclass(frozen=True, slots=True)
class Profile:
    """A target projection profile (its CONSTRUCT query + output prefixes)."""

    name: str
    prefixes: tuple[str, ...] = field(default_factory=tuple)


#: Registry of target profiles. Each maps to queries/projections/<name>.rq.
PROFILES: dict[str, Profile] = {
    "schema-org": Profile("schema-org", ("schema", "rdfs")),
    "geosparql": Profile("geosparql", ("geo",)),
    "vcard": Profile("vcard", ("vcard",)),
    "foaf": Profile("foaf", ("foaf", "wgs84")),
    "ical": Profile("ical", ("ical",)),
    "owl-time": Profile("owl-time", ("time",)),
    "odrl": Profile("odrl", ("odrl",)),
    "cc": Profile("cc", ("cc",)),
    "dcterms": Profile("dcterms", ("dcterms",)),
    "oai_dc": Profile("oai_dc", ("dc",)),
    "spdx": Profile("spdx", ("spdx",)),
    "ontolex": Profile("ontolex", ("ontolex", "lime", "rdf")),
    "web-annotation": Profile("web-annotation", ("oa",)),
    "skos": Profile("skos", ("skos",)),
    "bot": Profile("bot", ("bot",)),
    "mailmap": Profile("mailmap", ("gmeow",)),
    "exif": Profile("exif", ("exif",)),
    "iiif": Profile("iiif", ("iiif", "oa", "rdf")),
    "dcat": Profile("dcat", ("dcat", "dcterms", "prov", "spdx")),
    # #34 phases 2-3: the coverage profiles — plus doap/codemeta, which were
    # compile-only in the mapping compiler profile set but absent here, so their
    # queries never ran in MAXIMAL.
    "org": Profile("org", ("org",)),
    "bibo": Profile("bibo", ("bibo",)),
    "bibframe": Profile("bibframe", ("bibframe", "rdf")),
    "gedcom": Profile("gedcom", ("gedcom",)),
    "sioc": Profile("sioc", ("sioc",)),
    "doap": Profile("doap", ("doap",)),
    "codemeta": Profile("codemeta", ("codemeta",)),
    # Build provenance — the PROV qualification pattern (#415, #34 verdict 8)
    "prov": Profile("prov", ("prov",)),
}

#: Worked-example inputs (locations + naming + languages + identity + contacts +
#: events + rights). The events slice drives the schema.org event-role + iCalendar
#: VEVENT projections; the rights slice drives the ODRL / CC REL / Dublin Core
#: rights projections. Contested-fact variants are excluded so the published
#: examples stay neutral.
_EXAMPLE_FIXTURES = (
    "places.ttl",
    "names.ttl",
    "languages.ttl",
    "identity.ttl",
    "contact-fields.ttl",
    "events.ttl",
    "rights.ttl",
    "tags.ttl",
    "identity-over-history.ttl",
    "images.ttl",
    # #34 phases 2-3: the coverage-profile sources (genealogy, publications,
    # email threading, neutral memberships, software projects).
    "genealogy.ttl",
    "publications.ttl",
    "email.ttl",
    "organizations.ttl",
    "software-project.ttl",
    # #34 phase 3-4 work-down: migration endpoints (#412) + the build
    # provenance chain that drives the PROV qualification profile (#415).
    "migrations.ttl",
    "builds.ttl",
    # external-identifier records → schema:PropertyValue (#409).
    "identifier-records.ttl",
    # web-presence + media surface (#410).
    "web-presence.ttl",
    # syndicated content: postings, quotations, feeds (#412).
    "syndication.ttl",
    # people/org/accounts/presence + long tail (#411, #417, #413).
    "people-org-presence.ttl",
)


def _load_projection_query(profile: str) -> str:
    """The compiled CONSTRUCT for *profile*, from the repo or, failing that, the bundle.

    The dev fast-path reads ``generated/queries/<profile>.rq`` directly; a
    wheel-only install (no source tree) reads the query folded into the bundle
    (#bundle — the CLI razor: ``gmeow`` needs no repo).
    """
    path = PROJECTION_QUERY_DIR / f"{profile}.rq"
    if path.exists():
        return path.read_text(encoding="utf-8")
    from gmeow_tools.bundle import bundled_queries

    data = bundled_queries().get(f"{profile}.rq")
    if data is None:
        raise FileNotFoundError(f"projection query not found: {profile}.rq")
    return data.decode("utf-8")


def project_graph(
    profile: str,
    source: Graph | gmeow_rdf.Store,
    *,
    selector: LangSelector | None = None,
) -> Graph:
    """Run a profile's CONSTRUCT over a source, returning the projection.

    The CONSTRUCT runs on gmeow_rdf (~12x faster than rdflib's engine); the
    :mod:`gmeow_tools.engine_crosscheck` gate proves the two engines agree, so the
    output is identical to the former rdflib path.

    Args:
        profile: A key of :data:`PROFILES`.
        source: The data to project (ontology + instance data), either a
            gmeow_rdf store (preferred — no copy) or an rdflib graph (loaded into
            a fresh store).
        selector: Optional language selector for emitted labels/definitions.

    Returns:
        A fresh graph of pure target-vocabulary triples, prefixes bound.
    """
    prof = PROFILES[profile]
    query = _load_projection_query(profile)
    store = (
        source
        if isinstance(source, gmeow_rdf.Store)
        else sparql.store_from_graph(source)
    )
    out = sparql.construct(store, query)
    # Projection-boundary BCP-47 retag: the in-query retag only fires for
    # literals reachable from a gmeow:nameLanguage link, so a bare @x-gmeow-*
    # value (e.g. a name with no language link) would leak the internal tag.
    # This catch-all pass retags every remaining x-gmeow-* literal to its public
    # BCP-47 tag, so consumer parsers read the projected text as the real
    # language. Idempotent for already-retagged literals.
    retag_graph(out)
    if selector is not None:
        filter_graph(out, selector)
    for prefix in prof.prefixes:
        out.bind(prefix, PREFIXES[prefix])
    return out


def _serialize(graph: Graph, path: Path) -> Path:
    """Write a projection to Turtle and verify it re-parses (round-trip)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    graph.serialize(destination=path, format="turtle")
    check = Graph()
    check.parse(path, format="turtle")
    if len(check) != len(graph):
        raise ValueError(f"projection round-trip changed triple count: {path}")
    return path


def _example_store() -> gmeow_rdf.Store:
    """The asserted ontology plus the worked-example fixtures, as a store."""
    paths = [FIXTURES_DIR / fixture for fixture in _EXAMPLE_FIXTURES]
    return sparql.store_with(*paths, include_imports=False)


def _write_oai_dc_xml(graph: Graph, dist_dir: Path, stem: str) -> list[Path]:
    """Serialize an OAI-DC projection graph as OAI-PMH Dublin Core XML.

    The XML follows the OAI-DC schema:
    ``http://www.openarchives.org/OAI/2.0/oai_dc/``.
    Each distinct subject gets its own ``<oai_dc:dc>`` document.
    """
    from collections import defaultdict
    from xml.etree import ElementTree as ET

    dc_ns = "http://purl.org/dc/elements/1.1/"
    oai_dc_ns = "http://www.openarchives.org/OAI/2.0/oai_dc/"
    xsi_ns = "http://www.w3.org/2001/XMLSchema-instance"

    ET.register_namespace("oai_dc", oai_dc_ns)
    ET.register_namespace("dc", dc_ns)
    ET.register_namespace("xsi", xsi_ns)

    dist_dir.mkdir(parents=True, exist_ok=True)

    # Group triples by subject to prevent mixing different resources.
    from typing import Any

    subjects: defaultdict[str, list[tuple[str, Any]]] = defaultdict(list)
    for s_node, p_node, o_node in graph:
        p_str = str(p_node)
        if p_str.startswith(dc_ns):
            subjects[str(s_node)].append((p_str, o_node))

    paths: list[Path] = []
    for idx, s in enumerate(sorted(subjects), start=1):
        root = ET.Element(f"{{{oai_dc_ns}}}dc")
        root.set(
            f"{{{xsi_ns}}}schemaLocation",
            f"{oai_dc_ns} http://www.openarchives.org/OAI/2.0/oai_dc.xsd",
        )

        for p_str, o_node in sorted(subjects[s], key=lambda t: (t[0], str(t[1]))):
            local = p_str.replace(dc_ns, "")
            elem = ET.SubElement(root, f"{{{dc_ns}}}{local}")
            if hasattr(o_node, "language") and o_node.language:
                elem.set("{http://www.w3.org/XML/1998/namespace}lang", o_node.language)
            elem.text = str(o_node)

        tree = ET.ElementTree(root)
        ET.indent(tree, space="  ")
        path = dist_dir / f"{stem}-{idx:03d}.xml"
        tree.write(path, encoding="utf-8", xml_declaration=True)
        paths.append(path)

    return paths


def project_examples(
    dist_dir: Path = DIST_DIR, *, selector: LangSelector | None = None
) -> list[Path]:
    """Project the worked-example fixtures to every profile into ``dist_dir``."""
    store = _example_store()
    paths: list[Path] = []
    for name in PROFILES:
        graph = project_graph(name, store, selector=selector)
        paths.append(_serialize(graph, dist_dir / f"gmeow-example-{name}.ttl"))
        if name == "oai_dc":
            paths.extend(_write_oai_dc_xml(graph, dist_dir, "gmeow-example-oai_dc"))
    return paths


def project_file(
    input_path: Path,
    profile: str,
    *,
    dist_dir: Path = DIST_DIR,
    selector: LangSelector | None = None,
) -> Path:
    """Project an input data file to one profile (ontology merged in for context)."""
    from gmeow_rdf.compat.rdflib.util import guess_format

    data = Graph().parse(input_path, format=guess_format(str(input_path)) or "turtle")
    store = sparql.store_with(include_imports=False, extra_triples=data)
    out = project_graph(profile, store, selector=selector)
    return _serialize(out, dist_dir / f"gmeow-{input_path.stem}-{profile}.ttl")


_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"

#: Single-vocab view selectors that are not projection profiles.
GTS_VIEW_ALL = ("all", "maximal")
GTS_VIEW_GMEOW = "gmeow"


def gts_base_graph(gts_path: Path) -> Graph:
    """Extract the asserted base triples from a transpiled ``.gts``.

    A ``.gts`` is the canonical RDF-1.2 product: base/derived triples *plus* their
    provenance reifiers. This returns just the plain asserted triples — the
    reifier rows (``rdf:reifies``) and the quoted triple-terms are dropped — by
    routing through gmeow_rdf (which parses RDF-1.2 N-Quads that rdflib cannot).
    """
    import io

    import gts

    nquads = gts.to_nquads(gts.read(gts_path.read_bytes()))
    parsed = gmeow_rdf.Store()
    parsed.bulk_load(nquads.encode(), format=gmeow_rdf.RdfFormat.N_QUADS)
    reifies = gmeow_rdf.NamedNode(_REIFIES)
    base = gmeow_rdf.Store()
    for quad in parsed:
        # drop reifier rows and any quad with a quoted triple-term endpoint
        # (RDF-1.2 allows them in subject OR object; plain N-Triples / rdflib
        # cannot represent either, so they are not asserted base triples)
        if (
            quad.predicate == reifies
            or isinstance(quad.subject, gmeow_rdf.Triple)
            or isinstance(quad.object, gmeow_rdf.Triple)
        ):
            continue
        base.add(gmeow_rdf.Quad(quad.subject, quad.predicate, quad.object))
    buf = io.BytesIO()
    base.dump(
        buf,
        format=gmeow_rdf.RdfFormat.N_TRIPLES,
        from_graph=gmeow_rdf.DefaultGraph(),
    )
    graph = Graph()
    graph.parse(data=buf.getvalue(), format="nt")
    return graph


def _view_namespaces(view: str) -> frozenset[str]:
    """The IRI namespaces a single-vocab view keeps (empty = keep everything)."""
    if view in GTS_VIEW_ALL:
        return frozenset()  # the whole maximal — GMEOW + every vocab
    if view == GTS_VIEW_GMEOW:
        return frozenset({PREFIXES["gmeow"]})
    return frozenset(PREFIXES[p] for p in PROFILES[view].prefixes if p in PREFIXES)


def project_gts_subset(
    gts_path: Path,
    view: str,
    *,
    dist_dir: Path = DIST_DIR,
    selector: LangSelector | None = None,
) -> Path:
    """Emit the single-vocabulary view of a transpiled ``.gts`` (a filter).

    A *filter*, not a re-projection. The ``.gts`` is already maximal (GMEOW +
    every vocab), so a vocab view is the subset of its triples in that vocab's
    namespaces — the complete, drift-free view. ``view`` is a projection profile
    name, ``"gmeow"`` (the pure base), or
    ``"all"``/``"maximal"`` (the whole product). A triple is kept when its
    predicate is in the view's namespaces, or when it types a subject into a
    class of those namespaces (``rdf:type`` to a kept class).
    """
    from gmeow_rdf.compat.rdflib import RDF, URIRef

    base = gts_base_graph(gts_path)
    namespaces = _view_namespaces(view)
    if not namespaces:
        out = base  # all / maximal — keep everything
    else:
        out = Graph()
        for s, p, o in base:
            keep = any(str(p).startswith(ns) for ns in namespaces) or (
                p == RDF.type
                and isinstance(o, URIRef)
                and any(str(o).startswith(ns) for ns in namespaces)
            )
            if keep:
                out.add((s, p, o))
    retag_graph(out)
    if selector is not None:
        filter_graph(out, selector)
    for prefix, iri in PREFIXES.items():
        out.bind(prefix, iri)
    return _serialize(out, dist_dir / f"gmeow-{gts_path.stem}-{view}.ttl")
