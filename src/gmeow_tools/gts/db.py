# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The ``gts → {sqlite,duckdb}`` transforms (§14, issue #271).

Loads a folded :class:`~gmeow_tools.gts.model.Graph` into a relational store using
the **integer-id** model — five dictionary-encoded tables (``terms``, ``quads``,
``reifiers``, ``annotations``, ``blobs``) — so the target DB stays compact and the
load is a near-mechanical bulk insert; the engine does the join to resolve ids.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from gmeow_tools.gts.model import Graph

# DDL shared by both engines (both speak this SQL subset).
_SCHEMA = [
    "CREATE TABLE terms (id INTEGER PRIMARY KEY, kind INTEGER, lex TEXT,"
    " datatype INTEGER, lang TEXT, reifier INTEGER)",
    "CREATE TABLE quads (s INTEGER, p INTEGER, o INTEGER, g INTEGER)",
    "CREATE TABLE reifiers (reifier INTEGER, s INTEGER, p INTEGER, o INTEGER)",
    "CREATE TABLE annotations (reifier INTEGER, predicate INTEGER, value INTEGER)",
    "CREATE TABLE blobs (digest TEXT PRIMARY KEY, bytes BLOB)",
]
_INDEXES = [
    "CREATE INDEX quads_s ON quads (s)",
    "CREATE INDEX quads_p ON quads (p)",
    "CREATE INDEX quads_o ON quads (o)",
    "CREATE INDEX annot_reifier ON annotations (reifier)",
]

# (table, INSERT statement) in dependency-free order.
_INSERTS = [
    ("terms", "INSERT INTO terms VALUES (?,?,?,?,?,?)"),
    ("quads", "INSERT INTO quads VALUES (?,?,?,?)"),
    ("reifiers", "INSERT INTO reifiers VALUES (?,?,?,?)"),
    ("annotations", "INSERT INTO annotations VALUES (?,?,?)"),
    ("blobs", "INSERT INTO blobs VALUES (?,?)"),
]


class _Conn(Protocol):
    """The minimal cursor surface shared by sqlite3 and duckdb connections."""

    def execute(self, sql: str, /) -> object: ...
    def executemany(self, sql: str, params: list[tuple[object, ...]], /) -> object: ...


def _load(conn: _Conn, graph: Graph) -> None:
    """Create the schema, bulk-insert non-empty tables, then build indexes."""
    for ddl in _SCHEMA:
        conn.execute(ddl)
    rows = _rows(graph)
    for table, sql in _INSERTS:
        if rows[table]:  # duckdb rejects an empty executemany
            conn.executemany(sql, rows[table])
    for ddl in _INDEXES:
        conn.execute(ddl)


def _rows(graph: Graph) -> dict[str, list[tuple[object, ...]]]:
    """Project the folded graph into per-table id-valued rows."""
    return {
        "terms": [
            (i, int(t.kind), t.value, t.datatype, t.lang, t.reifier)
            for i, t in enumerate(graph.terms)
        ],
        "quads": [(s, p, o, g) for s, p, o, g in graph.quads],
        "reifiers": [(r, s, p, o) for r, (s, p, o) in graph.reifiers.items()],
        "annotations": list(graph.annotations),
        "blobs": list(graph.blobs.items()),
    }


def to_sqlite(graph: Graph, path: str | Path) -> Path:
    """Write a folded graph to a SQLite database, returning its path."""
    out = Path(path)
    out.unlink(missing_ok=True)
    try:
        conn = sqlite3.connect(out)
        try:
            _load(conn, graph)
            conn.commit()
        finally:
            conn.close()
    except Exception:
        out.unlink(missing_ok=True)  # don't leave a half-written DB on disk
        raise
    return out


def to_duckdb(graph: Graph, path: str | Path) -> Path:
    """Write a folded graph to a DuckDB database, returning its path."""
    import duckdb

    out = Path(path)
    out.unlink(missing_ok=True)
    try:
        conn = duckdb.connect(str(out))
        try:
            _load(conn, graph)
        finally:
            conn.close()
    except Exception:
        out.unlink(missing_ok=True)  # don't leave a half-written DB on disk
        raise
    return out
