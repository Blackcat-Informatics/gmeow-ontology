# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The gts → parquet export generator (#377, #12).

Marked ``ci_only`` like the other secondary export surfaces: CI and
``make test`` run it; the fast ``make check`` gate does not.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools import parquet_gen  # noqa: F401  (@register side effect)
from gmeow_tools.generator import Generator, registry
from gmeow_tools.gts_db import to_parquet
from gmeow_tools.gts_views import load_fold

pytestmark = pytest.mark.ci_only


def _generator() -> Generator:
    return registry()["parquet"]


def test_parquet_tables_match_the_fold(tmp_path: Path) -> None:
    """Every table round-trips through DuckDB with the fold's cardinalities."""
    import duckdb

    view = load_fold()
    written = {p.stem: p for p in to_parquet(view.graph, tmp_path)}
    expected = {
        "terms": len(view.graph.terms),
        "quads": len(view.graph.quads),
        "reifiers": len(view.reifiers()),
        "annotations": len(view.annotations()),
        "blobs": len(view.graph.blobs),
    }
    for table, count in expected.items():
        if count == 0:
            assert table not in written  # empty tables are skipped, not emitted
            continue
        quoted = str(written[table]).replace("'", "''")
        row = duckdb.sql(f"SELECT count(*) FROM read_parquet('{quoted}')").fetchone()
        assert row is not None and row[0] == count, table
    # The dictionary encoding resolves: every quad subject joins to a term row.
    q = str(written["quads"]).replace("'", "''")
    t = str(written["terms"]).replace("'", "''")
    joined = duckdb.sql(
        f"SELECT count(*) FROM read_parquet('{q}') q "
        f"JOIN read_parquet('{t}') t ON q.s = t.id"
    ).fetchone()
    assert joined is not None and joined[0] == len(view.graph.quads)


def test_semantic_compare_tolerates_writer_metadata(tmp_path: Path) -> None:
    """Two independent writes of the same fold are NOT drift (P7 gates content).

    Parquet bytes may embed writer metadata; the generator's ``compare``
    fingerprints rows, not bytes — equal tables compare clean even when the
    files differ byte-for-byte.
    """
    view = load_fold()
    first = {p.name: p for p in to_parquet(view.graph, tmp_path / "a")}
    second = {p.name: p for p in to_parquet(view.graph, tmp_path / "b")}
    gen = _generator()
    for name, fresh in first.items():
        assert gen.compare(fresh, second[name]) == [], name


def test_compare_detects_real_content_drift(tmp_path: Path) -> None:
    """A genuinely different table IS drift — the comparator must not be vacuous."""
    import duckdb

    view = load_fold()
    fresh = {p.name: p for p in to_parquet(view.graph, tmp_path / "a")}["quads.parquet"]
    truncated = tmp_path / "b" / "quads.parquet"
    truncated.parent.mkdir()
    src = str(fresh).replace("'", "''")
    dst = str(truncated).replace("'", "''")
    duckdb.sql(
        f"COPY (SELECT * FROM read_parquet('{src}') LIMIT 10) TO '{dst}'"
        " (FORMAT parquet)"
    )
    assert _generator().compare(fresh, truncated) != []
