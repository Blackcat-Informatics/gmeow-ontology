# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""GTS transport slice — the two invariants the slicetest harness can't reach.

Most of this slice's structural invariants now live as declarative
``gmeow:StructuralAssertion`` cells in ``tests/structural.ttl``, auto-discovered
and run by the native Rust harness (``crates/slicetest``, ``make slicetest``). See
``dsl/tests/MIGRATION-LEDGER.md`` for the per-test pytest→DSL mapping.

The two functions that remain here are not faithfully expressible as a boolean
SPARQL ASK:

* ``test_value_vocabulary_cardinality_floors`` — the numeric cardinality of the
  open value vocabularies (``>=7`` GTSProfile, ``>=7`` TransformCodec, ``==3``
  CodecClass). The NAMED individuals and the ``OpacityReason`` exact closed set
  are migrated to cells; only the counts (which an ASK cannot assert) remain.
* ``test_competency_queries_parse_and_run`` — a parse+execute SMOKE over the
  slice's ``queries/*.rq`` with NO pinned expected result. A
  ``gmeow:CompetencyQuestion`` cell requires an expected outcome, so authoring one
  would fabricate an assertion this smoke never made.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import URIRef
from gmeow_rdf.compat.rdflib.namespace import RDF

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SLICE_DIR = Path(__file__).resolve().parent.parent


def test_value_vocabulary_cardinality_floors() -> None:
    """Open value vocabularies (P9) — the numeric cardinality floors a boolean
    ASK cannot express. The named individuals and the OpacityReason exact set are
    migrated to tests/structural.ttl."""
    g = load_merged_graph(include_imports=True)

    profiles = set(g.subjects(RDF.type, URIRef(GMEOW + "GTSProfile")))
    assert len(profiles) >= 7

    codecs = set(g.subjects(RDF.type, URIRef(GMEOW + "TransformCodec")))
    assert len(codecs) >= 7

    classes = set(g.subjects(RDF.type, URIRef(GMEOW + "CodecClass")))
    assert len(classes) == 3


def test_competency_queries_parse_and_run() -> None:
    g = load_merged_graph(include_imports=True)
    for query_file in sorted((SLICE_DIR / "queries").glob("*.rq")):
        list(g.query(query_file.read_text(encoding="utf-8")))
