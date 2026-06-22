# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Durable native-FnO parity test (#848) — no dependency on the ``sssom`` package.

The whole FnO emitter is now native (``gmeow_slice.emit_fno`` →
``crates/slice/src/fno_emit.rs``). This test pins the live native emission against
the committed ``generated/projections/functions.fno.ttl`` via RDFC-1.0 graph
isomorphism (the same native, RDF-1.2-safe comparator used by
``tests/test_normalize_parity.py``).

The generator drift-gate (``regenerate`` / ``check-generated``) already enforces
byte-equality of the committed artifact; this test makes the graph-identity
contract explicit and independent of the writer's serialization.
"""

from __future__ import annotations

from pathlib import Path

import gmeow_slice
from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.language_tags import retag_graph
from gmeow_tools.rdf_canonical import graphs_isomorphic

PROJECT_ROOT = Path(__file__).resolve().parent.parent
_FNO_FILE = PROJECT_ROOT / "generated/projections/functions.fno.ttl"


def test_native_fno_isomorphic_to_committed() -> None:
    # Native emission: full-IRI N-Triples, sourced from the repo root (the exact
    # source the Python emit_fno wrapper consumes). The native emitter carries the
    # internal `@x-gmeow-english` tags; the committed Turtle is the public
    # projection surface, so we apply the same `retag_graph` projection-boundary
    # transform the writer (`_write_tree`) applies before serializing — comparing
    # the native graph the writer would have written, not the pre-retag form.
    ntriples = gmeow_slice.emit_fno(str(PROJECT_ROOT))
    native = Graph()
    native.parse(data=ntriples, format="nt")
    retag_graph(native)  # projection boundary: public BCP-47 only (#287)

    committed = Graph()
    committed.parse(data=_FNO_FILE.read_bytes(), format="turtle")

    assert graphs_isomorphic(native, committed), (
        "native FnO emission is not isomorphic to the committed "
        "generated/projections/functions.fno.ttl"
    )


def test_committed_fno_nonempty() -> None:
    # Guard against the parity test silently comparing two empty graphs.
    committed = Graph()
    committed.parse(data=_FNO_FILE.read_bytes(), format="turtle")
    assert len(committed) > 0
