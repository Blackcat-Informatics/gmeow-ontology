# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Temporary parity harness for the DSL SHACL cutover (#937).

This module compares the legacy Python ``gmeow_tools.dsl_validate`` path with
the new native ``gmeow_validate.validate_dsl_shacl`` entry point on the real
DSL source trees and on a synthetic malformed fixture. It is intentionally
prefixed with ``_`` so it is discovered and run by pytest; it will be removed
in Task 5 once parity is proven.
"""

from __future__ import annotations

from pathlib import Path

import gmeow_validate

from gmeow_tools.config import (
    DSL_TESTS_DIR,
    MAPPING_DSL_DIR,
    MAPPING_DSL_SHAPES_FILE,
    STATEMENT_DSL_DIR,
    STATEMENT_DSL_SHAPES_FILE,
    TEST_DSL_SHAPES_FILE,
)
from gmeow_tools.dsl_validate import (
    validate_mapping_dsl,
    validate_statement_dsl,
    validate_test_dsl,
)
from gmeow_tools.slices import iter_slice_mapping_files, iter_slice_test_files


def _mapping_sources() -> list[str]:
    sources = sorted(MAPPING_DSL_DIR.rglob("*.ttl"))
    sources += iter_slice_mapping_files()
    return [str(p) for p in sources]


def _statement_sources() -> list[str]:
    return [str(p) for p in sorted(STATEMENT_DSL_DIR.rglob("*.ttl"))]


def _test_sources() -> list[str]:
    sources = sorted(DSL_TESTS_DIR.glob("*.ttl"))
    sources += iter_slice_test_files()
    return [str(p) for p in sources]


_MALFORMED_MAPPING_TTL = """
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

# Missing alignSubject — should trigger a SHACL violation.
gmeow:eqBad001 a gmeow:TermEquivalence ;
    gmeow:alignPredicate owl:equivalentClass ;
    gmeow:alignObject <https://schema.org/Person> ;
    gmeow:confidence 1.0 ;
    gmeow:sssomFile "gmeow-bad.sssom.tsv" .
"""


def _sorted_violations(violations: list[str]) -> list[str]:
    """Normalize for comparison; the order of SHACL results is deterministic."""
    return sorted(violations)


def test_mapping_dsl_parity() -> None:
    paths = _mapping_sources()
    shapes_ttl = MAPPING_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    legacy = _sorted_violations(validate_mapping_dsl(paths))
    native = _sorted_violations(gmeow_validate.validate_dsl_shacl(paths, shapes_ttl))
    assert legacy == native, (
        f"mapping DSL parity mismatch:\nlegacy={legacy}\nnative={native}"
    )


def test_statement_dsl_parity() -> None:
    paths = _statement_sources()
    shapes_ttl = STATEMENT_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    legacy = _sorted_violations(validate_statement_dsl(paths))
    native = _sorted_violations(gmeow_validate.validate_dsl_shacl(paths, shapes_ttl))
    assert legacy == native, (
        f"statement DSL parity mismatch:\nlegacy={legacy}\nnative={native}"
    )


def test_test_dsl_parity() -> None:
    paths = _test_sources()
    shapes_ttl = TEST_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    legacy = _sorted_violations(validate_test_dsl(paths))
    native = _sorted_violations(gmeow_validate.validate_dsl_shacl(paths, shapes_ttl))
    assert legacy == native, (
        f"test DSL parity mismatch:\nlegacy={legacy}\nnative={native}"
    )


def test_malformed_mapping_cell_parity(tmp_path: Path) -> None:
    cell = tmp_path / "bad.ttl"
    cell.write_text(_MALFORMED_MAPPING_TTL, encoding="utf-8")
    paths = [str(cell)]
    shapes_ttl = MAPPING_DSL_SHAPES_FILE.read_text(encoding="utf-8")
    legacy = _sorted_violations(validate_mapping_dsl(paths))
    native = _sorted_violations(gmeow_validate.validate_dsl_shacl(paths, shapes_ttl))
    assert legacy == native, (
        f"malformed mapping cell parity mismatch:\nlegacy={legacy}\nnative={native}"
    )
    assert legacy, "malformed fixture must produce violations"
