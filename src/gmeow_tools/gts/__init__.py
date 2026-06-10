# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""GTS — Graph Transport Substrate — reference implementation (baseline).

A small, dependency-light reader/writer for the GTS v0.2 wire format defined in
``docs/GTS-SPEC.md``. Covers the CBOR append-only log, the four-table RDF 1.2 fold,
the ``identity``/``gzip``/``zstd`` codecs, opaque/damaged degradation, torn-append
detection, the ``gts → nquads`` transform, the ``RDF → GTS`` producer, the
``gts → {sqlite,duckdb}`` transforms, and COSE signing (§9.2). COSE encryption,
nested-GTS recursion, and the index/MMR acceleration are deferred (see #267/#272).
"""

from __future__ import annotations

from gmeow_tools.gts.crypto import InMemoryKeys, KeyProvider, Signer
from gmeow_tools.gts.db import to_duckdb, to_sqlite
from gmeow_tools.gts.model import (
    Diagnostic,
    Graph,
    OpaqueNode,
    Signature,
    Term,
    TermKind,
)
from gmeow_tools.gts.nquads import to_nquads
from gmeow_tools.gts.producer import compile_gts, gts_from_graph, gts_from_rdf12
from gmeow_tools.gts.reader import read
from gmeow_tools.gts.writer import Writer

__all__ = [
    "Diagnostic",
    "Graph",
    "InMemoryKeys",
    "KeyProvider",
    "OpaqueNode",
    "Signature",
    "Signer",
    "Term",
    "TermKind",
    "Writer",
    "compile_gts",
    "gts_from_graph",
    "gts_from_rdf12",
    "read",
    "to_duckdb",
    "to_nquads",
    "to_sqlite",
]
