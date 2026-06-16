# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The ``gts → parquet`` export generator (#377, #12).

Projects the committed GTS snapshot into one Parquet file per table of the
relational integer-id schema (:mod:`gmeow_tools.gts_db`): ``terms``, ``quads``,
``reifiers``, ``annotations``, ``blobs`` — the columnar interchange form for
DataFrame/SQL consumers (DuckDB, pandas, polars, Spark) who should not need an
RDF parser (CONSTITUTION P13). LOSSLESS: the tables jointly carry every term,
quad, reifier binding, statement annotation, and inline blob of the fold.

A fold-only shim over the narrow waist (#267): the snapshot is the single
input; no rdflib, no pyoxigraph. Outputs live under ``dist/parquet/``
(git-ignored, published on release): Parquet bytes embed writer metadata and
are not guaranteed byte-deterministic across library versions, so drift is
compared SEMANTICALLY (row counts + content hash via DuckDB) rather than
byte-for-byte.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from gmeow_tools.config import DIST_DIR, GTS_SNAPSHOT_FILE, PROJECT_ROOT
from gmeow_tools.generator import Generator, _rel, register
from gmeow_tools.gts_db import to_parquet
from gmeow_tools.gts_views import load_fold

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

#: The generator's output directory (under the git-ignored dist/ tree).
PARQUET_DIR = DIST_DIR / "parquet"

_TABLES = ("terms", "quads", "reifiers", "annotations", "blobs")


def _table_fingerprint(path: Path) -> tuple[int, object]:
    """A semantic fingerprint of one Parquet file: (row count, content hash).

    Ordered content hash via DuckDB's parquet reader — two files with the same
    rows in the same order fingerprint equal regardless of writer metadata.
    """
    import duckdb

    quoted = str(path).replace("'", "''")
    row = duckdb.sql(
        f"SELECT count(*), md5(string_agg(t::VARCHAR, '|' ORDER BY t::VARCHAR))"
        f" FROM read_parquet('{quoted}') t"
    ).fetchone()
    return (0, None) if row is None else (row[0], row[1])


@register
class ParquetGenerator(Generator):
    """Generate the per-table Parquet projection of the GTS snapshot."""

    name: str = "parquet"

    @property
    def inputs(self) -> Sequence[Path]:
        """The GTS snapshot is the single input (the narrow waist)."""
        return [GTS_SNAPSHOT_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """One Parquet file per (non-empty) relational table."""
        return [PARQUET_DIR / f"{table}.parquet" for table in _TABLES]

    def render(self, staging: Path) -> None:
        """Render the Parquet tables from the GTS snapshot."""
        out_dir = staging / PARQUET_DIR.relative_to(PROJECT_ROOT)
        to_parquet(load_fold().graph, out_dir)

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Semantic drift: row counts + ordered content hash, never bytes.

        Parquet writer metadata varies across library versions; equal tables
        must not read as drift (P7 wants the CONTENT gated). Missing committed
        files are skipped — the outputs are git-ignored, like the exports.
        """
        if not committed.exists():
            return []
        if not fresh.exists():
            return [f"{_rel(committed)} (not produced in staging)"]
        if _table_fingerprint(fresh) != _table_fingerprint(committed):
            return [f"{_rel(committed)}"]
        return []
