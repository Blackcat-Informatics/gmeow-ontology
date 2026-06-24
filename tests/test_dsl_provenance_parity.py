# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Parity guard for the native DSL focus→file provenance (#579, #937).

The DSL SHACL seam now runs entirely in Rust (``gmeow_validate.validate_dsl_shacl``):
it merges Turtle sources, builds the first-seen focus→file map, validates via
``gmeow_shacl``, and enriches violations with ``source=``. This module pins the
behavior two ways:

* **Real-tree golden** — over the actual mapping + statement DSL dirs, the
  validation is clean (0 violations) on a healthy tree; this asserts that, which
  is the same invariant ``make validate`` enforces. (A clean tree yields no
  ``source=`` lines, so the empty case is the golden.)

* **Provenance unit test** — because the empty case never exercises the
  focus→file map, a tiny two-file fixture drives the map directly: a subject
  appearing in two files maps to the FIRST (first-seen-wins), and a malformed
  fixture produces a real ``source=`` line through the full native path.

Parity basis
^^^^^^^^^^^^
This guard proves that the native formatter is byte-for-byte compatible with
the deleted Python implementation in ``src/gmeow_tools/dsl_validate.py``
(removed in ``736373fcca``). Its ``_format_violations`` helper emitted one
string per SHACL result in this exact shape:

```text
focus=<focusNode> | path=<resultPath> | msg=<message> | source=<file>
```

``path`` and ``msg`` were omitted when absent; ``source=`` was appended only
when the focus node was a named IRI present in the focus→file map. The native
``validate_dsl_shacl`` preserves that same component order, omission behavior,
and provenance enrichment, so consumers that previously parsed or displayed
Python-generated violations continue to work unchanged.
"""

from __future__ import annotations

import re
from pathlib import Path

import gmeow_validate
import pytest

from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    MAPPING_DSL_SHAPES_FILE,
    STATEMENT_DSL_DIR,
    STATEMENT_DSL_SHAPES_FILE,
)

# --------------------------------------------------------------------------- #
# Real-tree golden: the DSL validation is clean (the empty-violations golden).
# --------------------------------------------------------------------------- #


def test_mapping_dsl_real_tree_is_clean() -> None:
    paths = [str(p) for p in sorted(MAPPING_DSL_DIR.rglob("*.ttl"))]
    shapes_ttl = MAPPING_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    assert gmeow_validate.validate_dsl_shacl(paths, shapes_ttl) == []


def test_statement_dsl_real_tree_is_clean() -> None:
    paths = [str(p) for p in sorted(STATEMENT_DSL_DIR.rglob("*.ttl"))]
    shapes_ttl = STATEMENT_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    assert gmeow_validate.validate_dsl_shacl(paths, shapes_ttl) == []


# --------------------------------------------------------------------------- #
# Provenance unit test: the focus→file map directly (first-seen-wins).
# --------------------------------------------------------------------------- #


def test_focus_to_file_first_seen_wins(tmp_path: Path) -> None:
    a = tmp_path / "a.ttl"
    a.write_text(
        "@prefix ex: <https://example.org/> .\n"
        "ex:alice ex:p ex:b .\n"
        "ex:shared ex:p ex:x .\n",
        encoding="utf-8",
    )
    b = tmp_path / "b.ttl"
    b.write_text(
        "@prefix ex: <https://example.org/> .\n"
        "ex:bob ex:p ex:c .\n"
        "ex:shared ex:p ex:y .\n",
        encoding="utf-8",
    )
    data_nt, pairs = gmeow_validate.dsl_merge_with_provenance([str(a), str(b)])
    focus_to_file = dict(pairs)

    assert focus_to_file["https://example.org/alice"] == str(a)
    assert focus_to_file["https://example.org/bob"] == str(b)
    # First-seen-wins: ex:shared appears in a then b → mapped to a.
    assert focus_to_file["https://example.org/shared"] == str(a)
    # The merged N-Triples carries every triple from both files (4 distinct).
    assert data_nt.count("\n") == 4


def test_malformed_mapping_cell_carries_source(tmp_path: Path) -> None:
    """A real SHACL violation enriches with ``source=`` from the focus→file map."""
    cell = tmp_path / "bad.ttl"
    cell.write_text(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n"
        "# Missing alignSubject — a SHACL violation on a named focus node.\n"
        "gmeow:eqBad001 a gmeow:TermEquivalence ;\n"
        "    gmeow:alignPredicate owl:equivalentClass ;\n"
        "    gmeow:alignObject <https://schema.org/Person> ;\n"
        "    gmeow:confidence 1.0 ;\n"
        '    gmeow:sssomFile "gmeow-bad.sssom.tsv" .\n',
        encoding="utf-8",
    )
    shapes_ttl = MAPPING_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    violations = gmeow_validate.validate_dsl_shacl([str(cell)], shapes_ttl)
    assert violations, "the malformed cell must produce at least one violation"
    joined = "\n".join(violations)
    assert "focus=https://blackcatinformatics.ca/gmeow/eqBad001" in joined
    assert f"source={cell}" in joined


def test_malformed_mapping_cell_legacy_format_matches_sample(tmp_path: Path) -> None:
    """The native output matches the documented legacy Python formatter sample."""
    cell = tmp_path / "bad.ttl"
    cell.write_text(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n"
        "# Missing alignSubject — a SHACL violation on a named focus node.\n"
        "gmeow:eqBad001 a gmeow:TermEquivalence ;\n"
        "    gmeow:alignPredicate owl:equivalentClass ;\n"
        "    gmeow:alignObject <https://schema.org/Person> ;\n"
        "    gmeow:confidence 1.0 ;\n"
        '    gmeow:sssomFile "gmeow-bad.sssom.tsv" .\n',
        encoding="utf-8",
    )
    shapes_ttl = MAPPING_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    violations = gmeow_validate.validate_dsl_shacl([str(cell)], shapes_ttl)
    assert len(violations) == 1
    expected = (
        "focus=https://blackcatinformatics.ca/gmeow/eqBad001 | "
        "path=https://blackcatinformatics.ca/gmeow/alignSubject | "
        "msg=TermEquivalence must have exactly one gmeow:alignSubject (IRI) | "
        f"source={cell}"
    )
    assert violations[0] == expected


def test_dsl_parse_error_hard_fails(tmp_path: Path) -> None:
    bad = tmp_path / "broken.ttl"
    bad.write_text("this is not turtle @@@ <<<", encoding="utf-8")
    shapes_ttl = MAPPING_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    with pytest.raises(ValueError, match=re.escape(str(bad))):
        gmeow_validate.validate_dsl_shacl([str(bad)], shapes_ttl)
