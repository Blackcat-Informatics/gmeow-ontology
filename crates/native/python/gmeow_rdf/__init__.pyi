# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Type stub for the gmeow_rdf PyO3 extension. The signatures are transcribed
# verbatim from crates/rdf/src/py.rs (the statement codec) and
# crates/rdf/src/py_store.rs (the native oxigraph Store / SPARQL / parse /
# canonicalize surface, #667) — keep them in lockstep with those files (they are
# the ABI source of truth). This stub describes the native `gmeow_rdf` term /
# result / store surface — the in-repo binding that replaced the external RDF
# package removed in #667.

from __future__ import annotations

import builtins
from typing import IO, overload

# ── Statement codec (crates/rdf/src/py.rs) ──────────────────────────────────────

def project_statements_rdf12(owl_ttl: str) -> str: ...
def normalize_rdf12_to_owl(rdf12_ttl: str) -> str: ...
def loss_matrix_json() -> str: ...
def canonicalize_turtle(
    turtle_bytes: bytes, extra_prefixes: list[tuple[str, str]] = ...
) -> bytes: ...

# ── Serialization / canonicalization enums ──────────────────────────────────────

class RdfFormat:
    TURTLE: RdfFormat
    N_TRIPLES: RdfFormat
    N_QUADS: RdfFormat
    TRIG: RdfFormat

class CanonicalizationAlgorithm:
    RDFC_1_0: CanonicalizationAlgorithm
    UNSTABLE: CanonicalizationAlgorithm

# ── Term model ──────────────────────────────────────────────────────────────────

class NamedNode:
    def __init__(self, value: str) -> None: ...
    @property
    def value(self) -> str: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class BlankNode:
    def __init__(self, value: str) -> None: ...
    @property
    def value(self) -> str: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class Literal:
    def __init__(
        self,
        value: str,
        *,
        datatype: NamedNode | None = ...,
        language: str | None = ...,
    ) -> None: ...
    @property
    def value(self) -> str: ...
    @property
    def language(self) -> str | None: ...
    @property
    def datatype(self) -> NamedNode: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class Triple:
    def __init__(
        self, subject: _Subject, predicate: NamedNode, object: _Term
    ) -> None: ...
    @property
    def subject(self) -> _Subject: ...
    @property
    def predicate(self) -> NamedNode: ...
    @property
    def object(self) -> _Term: ...
    def __hash__(self) -> int: ...
    # `object` (the property above) shadows the builtin in class scope, so the
    # annotation must qualify it — otherwise mypy reads it as `Triple.object`.
    def __eq__(self, other: builtins.object) -> bool: ...

class DefaultGraph:
    def __init__(self) -> None: ...

class Quad:
    def __init__(
        self,
        subject: _Subject,
        predicate: NamedNode,
        object: _Term,
        graph_name: NamedNode | BlankNode | DefaultGraph | None = ...,
    ) -> None: ...
    @property
    def subject(self) -> _Subject: ...
    @property
    def predicate(self) -> NamedNode: ...
    @property
    def object(self) -> _Term: ...
    @property
    def graph_name(self) -> NamedNode | BlankNode | DefaultGraph: ...
    def __hash__(self) -> int: ...
    # `object` (the property above) shadows the builtin in class scope, so the
    # annotation must qualify it — otherwise mypy reads it as `Quad.object`.
    def __eq__(self, other: builtins.object) -> bool: ...

class Variable:
    def __init__(self, value: str) -> None: ...
    @property
    def value(self) -> str: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

# RDF 1.2 (unlike the obsolete RDF-star) permits triple terms in the OBJECT
# position only: a subject is an IRI or blank node, never a quoted triple. This
# mirrors oxigraph's `NamedOrBlankNode` subject type — see `extract_subject` in
# crates/rdf/src/py_store.rs.
_Subject = NamedNode | BlankNode
_Term = NamedNode | BlankNode | Literal | Triple

# ── Query results ───────────────────────────────────────────────────────────────

class QuerySolution:
    def __getitem__(self, key: str | Variable | int) -> _Term | None: ...

class QuerySolutions:
    @property
    def variables(self) -> list[Variable]: ...
    def __iter__(self) -> QuerySolutions: ...
    def __next__(self) -> QuerySolution: ...
    def __len__(self) -> int: ...

class QueryTriples:
    def __iter__(self) -> QueryTriples: ...
    def __next__(self) -> Triple: ...
    def __len__(self) -> int: ...
    def serialize(self, format: RdfFormat) -> bytes: ...

class QueryBoolean:
    def __bool__(self) -> bool: ...

# ── Store / Dataset ─────────────────────────────────────────────────────────────

class QuadIter:
    def __iter__(self) -> QuadIter: ...
    def __next__(self) -> Quad: ...

class Store:
    def __init__(self) -> None: ...
    def __iter__(self) -> QuadIter: ...
    def load(
        self,
        input: bytes | str | None = ...,
        format: RdfFormat | None = ...,
        *,
        path: str | None = ...,
    ) -> None: ...
    def bulk_load(
        self,
        input: bytes | str | None = ...,
        format: RdfFormat | None = ...,
        *,
        path: str | None = ...,
    ) -> None: ...
    def add(self, quad: Quad) -> None: ...
    def remove(self, quad: Quad) -> None: ...
    def query(
        self,
        query: str,
        *,
        substitutions: dict[Variable, _Term] | None = ...,
    ) -> QuerySolutions | QueryTriples | QueryBoolean: ...
    @overload
    def dump(
        self,
        output: IO[bytes],
        format: RdfFormat,
        *,
        from_graph: NamedNode | BlankNode | DefaultGraph | None = ...,
    ) -> None: ...
    @overload
    def dump(
        self,
        output: None = ...,
        *,
        format: RdfFormat,
        from_graph: NamedNode | BlankNode | DefaultGraph | None = ...,
    ) -> bytes: ...
    def __len__(self) -> int: ...

class Dataset:
    def __init__(self, quads: object | None = ...) -> None: ...
    def add(self, quad: Quad) -> None: ...
    def canonicalize(self, algorithm: CanonicalizationAlgorithm) -> None: ...
    def __iter__(self) -> QuadIter: ...
    def __len__(self) -> int: ...

# ── Module functions ────────────────────────────────────────────────────────────

def parse(input: bytes | str, format: RdfFormat) -> list[Quad]: ...
@overload
def serialize(input: QueryTriples, output: IO[bytes], format: RdfFormat) -> None: ...
@overload
def serialize(
    input: QueryTriples, output: None = ..., *, format: RdfFormat
) -> bytes: ...

# ── RDF → GTS producer (crates/rdf/src/py_gts.rs, #819 Task 8) ───────────────────

#: A `(data, media_type, rep)` content-addressed blob row.
_BlobRow = tuple[bytes, str, str]
#: A `(data, format, graph_name, scope)` named-graph ingest row.
_NamedGraphRow = tuple[bytes, RdfFormat, str | None, str | None]

def gts_from_quads(
    data: bytes,
    *,
    format: RdfFormat,
    profile: str = ...,
    transform: list[str] | None = ...,
) -> bytes: ...
def gts_from_rdf12_bytes(
    data: bytes,
    *,
    format: RdfFormat,
    profile: str = ...,
    transform: list[str] | None = ...,
) -> bytes: ...
def compile_gts_native(
    base_data: bytes,
    base_format: RdfFormat,
    *,
    base_scope: str | None = ...,
    rdf12_data: bytes | None = ...,
    rdf12_format: RdfFormat | None = ...,
    rdf12_graph_name: str | None = ...,
    rdf12_scope: str | None = ...,
    named_graphs: list[_NamedGraphRow] | None = ...,
    transform: list[str] | None = ...,
    doc_blobs: list[_BlobRow] | None = ...,
    report_blobs: list[_BlobRow] | None = ...,
    signer_secret: bytes | None = ...,
    signer_kid: str | None = ...,
    public_key_armor: str | None = ...,
    rsyncable_threshold: int = ...,
) -> bytes: ...
def snapshot_content_id_native(data: bytes, *, format: RdfFormat) -> str: ...

# ── Text-format codecs via gmeow-gts (JSON-LD-star + RDF/XML, #834) ──────────────
# RDF bytes ↔ JSON-LD-star / RDF/XML through the gmeow-gts codec set. The compat
# `Graph.serialize`/`parse` route these formats here; serialize takes RDF bytes in
# `format` and returns the text form, parse takes the text and returns N-Quads bytes.
def to_json_ld(data: bytes, *, format: RdfFormat) -> str: ...
def from_json_ld(text: str) -> bytes: ...
def to_rdf_xml(data: bytes, *, format: RdfFormat) -> str: ...
def from_rdf_xml(text: str) -> bytes: ...
def feedback_bundle_native(
    data: bytes,
    *,
    format: RdfFormat,
    report_blobs: list[_BlobRow] | None = ...,
) -> bytes: ...

# A Python handle to a frozen, immutable RDF 1.2 dataset (#819 C7 foundation).
class RdfDataset:
    def __init__(self, data: bytes | str, format: RdfFormat) -> None: ...
    def quad_count(self) -> int: ...
    def term_count(self) -> int: ...
    def __len__(self) -> int: ...
    def to_gts(self, profile: str = ...) -> bytes: ...
