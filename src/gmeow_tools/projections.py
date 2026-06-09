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

import pyoxigraph
from rdflib import Graph

from gmeow_tools import sparql
from gmeow_tools.config import DIST_DIR, FIXTURES_DIR, PREFIXES, PROJECTION_QUERY_DIR


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
    "bot": Profile("bot", ("bot",)),
    "mailmap": Profile("mailmap", ("gmeow",)),
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
)


def project_graph(profile: str, source: Graph | pyoxigraph.Store) -> Graph:
    """Run a profile's CONSTRUCT over a source, returning the projection.

    The CONSTRUCT runs on pyoxigraph (~12x faster than rdflib's engine); the
    :mod:`gmeow_tools.engine_crosscheck` gate proves the two engines agree, so the
    output is identical to the former rdflib path.

    Args:
        profile: A key of :data:`PROFILES`.
        source: The data to project (ontology + instance data), either a
            pyoxigraph store (preferred — no copy) or an rdflib graph (loaded into
            a fresh store).

    Returns:
        A fresh graph of pure target-vocabulary triples, prefixes bound.
    """
    prof = PROFILES[profile]
    query = (PROJECTION_QUERY_DIR / f"{profile}.rq").read_text(encoding="utf-8")
    store = (
        source
        if isinstance(source, pyoxigraph.Store)
        else sparql.store_from_graph(source)
    )
    out = sparql.construct(store, query)
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


def _example_store() -> pyoxigraph.Store:
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


def project_examples(dist_dir: Path = DIST_DIR) -> list[Path]:
    """Project the worked-example fixtures to every profile into ``dist_dir``."""
    store = _example_store()
    paths: list[Path] = []
    for name in PROFILES:
        graph = project_graph(name, store)
        paths.append(_serialize(graph, dist_dir / f"gmeow-example-{name}.ttl"))
        if name == "oai_dc":
            paths.extend(_write_oai_dc_xml(graph, dist_dir, "gmeow-example-oai_dc"))
    return paths


def project_file(input_path: Path, profile: str, *, dist_dir: Path = DIST_DIR) -> Path:
    """Project an input data file to one profile (ontology merged in for context)."""
    from rdflib.util import guess_format

    data = Graph().parse(input_path, format=guess_format(str(input_path)) or "turtle")
    store = sparql.store_with(include_imports=False, extra_triples=data)
    out = project_graph(profile, store)
    return _serialize(out, dist_dir / f"gmeow-{input_path.stem}-{profile}.ttl")
