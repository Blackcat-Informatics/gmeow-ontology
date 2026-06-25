# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Thin Rust-delegating surface for the YAML-LD-star / JSON-LD-star lane (#699).

The authoritative codec lives in Rust (`crates/pipeline/src/stages/yaml_ld.rs`):
the JSON-LD-star / YAML-LD-star serializers, the full RDF-1.2 reverse parser, the
anchor/alias hard-fail on YAML ingest, and the statement-metadata downcast. This
module exposes only the two up-projection entry points the CLI transpile lane
needs, each routing straight through the native ``gmeow_native.pipeline``
surface. There is no second Python parser, no pyoxigraph, and no YAML library
here — the Rust codec is the single authority.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph


def jsonld_star_to_graph(json_bytes: bytes) -> Graph:
    """Parse JSON-LD-star into a ``gmeow_rdf.compat.rdflib.Graph``.

    The Rust native parser downcasts RDF 1.2 quoted triples to native GMEOW
    statement-metadata triples before the data reaches the rdflib-compat
    ``Graph``, so statement-level annotations survive the up-projection lane
    losslessly (#699).
    """
    import gmeow_native.pipeline as _pipeline

    nquads = _pipeline.parse_jsonld_star_to_gmeow_statement_metadata_nquads(json_bytes)
    graph = Graph()
    graph.parse(data=nquads.decode("utf-8"), format="nquads")
    return graph


def yaml_ld_to_graph(yaml_bytes: bytes) -> Graph:
    """Parse YAML-LD-star into a ``gmeow_rdf.compat.rdflib.Graph``.

    Routes YAML-LD-star through the Rust native downcast, which converts YAML to
    JSON-LD-star (hard-failing on YAML anchors/aliases), then downcasts RDF 1.2
    quoted triples to native GMEOW statement-metadata triples so the
    rdflib-compat ``Graph`` facade carries statement-level annotations
    losslessly (#699).
    """
    import gmeow_native.pipeline as _pipeline

    nquads = _pipeline.parse_yaml_ld_star_to_gmeow_statement_metadata_nquads(yaml_bytes)
    graph = Graph()
    graph.parse(data=nquads.decode("utf-8"), format="nquads")
    return graph
