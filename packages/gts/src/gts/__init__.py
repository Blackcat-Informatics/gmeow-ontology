# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""GTS — Graph Transport Substrate — the format engine (reference implementation).

A small, dependency-light reader/writer for the GTS wire format defined in
``docs/GTS-SPEC.md``: the CBOR append-only log, the four-table RDF 1.2 fold,
multi-segment ``cat``-append composition (§3.1), the ``identity``/``gzip``/
``zstd`` codecs, opaque/damaged degradation, torn-append detection, the
``gts → nquads`` transform, and COSE signing (§9.2).

Producers and database transforms (``RDF → GTS``, ``gts → {sqlite,duckdb}``)
live with their heavyweight dependencies in consuming packages — this package
owns the format, nothing else.
"""

from __future__ import annotations

from gts.crypto import InMemoryKeys, KeyProvider, Signer
from gts.model import (
    Diagnostic,
    Graph,
    OpaqueNode,
    Signature,
    Suppression,
    Term,
    TermKind,
)
from gts.nquads import to_nquads
from gts.reader import read, read_segments
from gts.writer import Writer

__all__ = [
    "Diagnostic",
    "Graph",
    "InMemoryKeys",
    "KeyProvider",
    "OpaqueNode",
    "Signature",
    "Signer",
    "Suppression",
    "Term",
    "TermKind",
    "Writer",
    "read",
    "read_segments",
    "to_nquads",
]
