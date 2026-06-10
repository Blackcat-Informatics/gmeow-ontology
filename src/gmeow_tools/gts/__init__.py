# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""GTS — Graph Transport Substrate — reference implementation (baseline).

A small, dependency-light reader/writer for the GTS v0.2 wire format defined in
``docs/GTS-SPEC.md``. The baseline covers the CBOR append-only log, the four-table
RDF 1.2 fold, the ``identity``/``gzip``/``zstd`` codecs, opaque/damaged degradation,
torn-append detection, and the ``gts → nquads`` transform. COSE signing/encryption,
nested-GTS recursion, the index/MMR acceleration, and the database transforms are
deferred (see issue #267/#268).
"""

from __future__ import annotations

from gmeow_tools.gts.model import (
    Diagnostic,
    Graph,
    OpaqueNode,
    Term,
    TermKind,
)
from gmeow_tools.gts.nquads import to_nquads
from gmeow_tools.gts.reader import read
from gmeow_tools.gts.writer import Writer

__all__ = [
    "Diagnostic",
    "Graph",
    "OpaqueNode",
    "Term",
    "TermKind",
    "Writer",
    "read",
    "to_nquads",
]
