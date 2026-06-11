# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""TRANSIENT (narrow waist PR 3, commit A): fold path ≡ rdflib path.

Proves the GTS-fold implementation of the export views emits identical terms
and byte-identical artifacts before the rdflib path is deleted in commit B —
at which point this file is deleted with it. The permanent behavior tests
live in tests/test_export.py.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.export import (
    _collect_terms_rdflib,
    collect_terms,
    fold_meta,
    write_csvs,
    write_csvw,
    write_jsonl,
    write_llms_txt,
    write_markdown,
)
from gmeow_tools.gts_views import load_fold


def test_fold_terms_equal_rdflib_terms() -> None:
    assert collect_terms() == _collect_terms_rdflib()


def test_all_seven_artifacts_are_byte_identical(tmp_path: Path) -> None:
    old_dir, new_dir = tmp_path / "old", tmp_path / "new"
    old_dir.mkdir()
    new_dir.mkdir()

    old_terms = _collect_terms_rdflib()
    write_csvs(old_terms, old_dir)
    write_csvw(old_dir)  # meta defaults: the self-description strings
    write_jsonl(old_terms, old_dir)
    write_markdown(old_terms, old_dir)
    write_llms_txt(old_terms, old_dir)

    view = load_fold()
    title, version = fold_meta(view)
    new_terms = collect_terms(view)
    write_csvs(new_terms, new_dir)
    write_csvw(new_dir, title=title)
    write_jsonl(new_terms, new_dir)
    write_markdown(new_terms, new_dir, title=title, version=version)
    write_llms_txt(new_terms, new_dir, title=title, version=version)

    old_files = sorted(p.name for p in old_dir.iterdir())
    new_files = sorted(p.name for p in new_dir.iterdir())
    assert old_files == new_files and len(old_files) == 7
    for name in old_files:
        assert (old_dir / name).read_bytes() == (new_dir / name).read_bytes(), name
