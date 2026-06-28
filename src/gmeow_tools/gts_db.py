# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Installed Python entry points for Rust-owned GTS relational exports."""

from __future__ import annotations

from pathlib import Path

import gmeow_rdf as ox


def to_sqlite(data: bytes, path: str | Path) -> Path:
    """Write GTS bytes to a SQLite database, returning its path."""
    return Path(ox.gts_to_sqlite(data, str(path)))


def to_duckdb(data: bytes, path: str | Path) -> Path:
    """Write GTS bytes to a DuckDB database, returning its path."""
    return Path(ox.gts_to_duckdb(data, str(path)))


def to_parquet(data: bytes, out_dir: str | Path) -> list[Path]:
    """Write one Parquet file per non-empty GTS relational table."""
    return [Path(path) for path in ox.gts_to_parquet(data, str(out_dir))]
