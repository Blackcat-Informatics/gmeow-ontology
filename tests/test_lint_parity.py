# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Durable parity guard for the Rust structural / naming / ownership lints (#579).

The goldens under ``tests/fixtures/lint-golden/`` were captured from the *original*
pure-Python ``structural_lint`` / ``term_naming_lint`` / ``slice_ownership_lint``
over the real merged graph BEFORE the Rust port. This test asserts the Rust path
reproduces each golden EXACTLY — so the behavior is pinned independently of the
Python lint bodies, and the guard survives Task 5's deletion of those bodies.

Two routes are checked per lint:

* the direct ``gmeow_validate`` extension API over the real source paths, and
* the ``gmeow_tools.validate`` wrappers over the merged rdflib graph,

so neither the FFI boundary nor the Python adapter can drift from the golden.
"""

from __future__ import annotations

import json
from pathlib import Path

import gmeow_validate
from gmeow_rdf.compat.rdflib import RDF, URIRef

from gmeow_tools.config import NAMESPACE, ONTOLOGY_IRI
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.slices import discover_slices, iter_slice_module_files
from gmeow_tools.validate import (
    _SELECTOR_TOKENS,
    slice_ownership_lint,
    structural_lint,
    term_naming_lint,
)

_GOLDEN_DIR = Path(__file__).parent / "fixtures" / "lint-golden"


def _golden(name: str) -> dict[str, list[str]]:
    payload = json.loads((_GOLDEN_DIR / f"{name}.json").read_text(encoding="utf-8"))
    assert isinstance(payload, dict)
    return payload


def _lint_config() -> gmeow_validate.LintConfig:
    slices = discover_slices()
    core = [s.iri for s in slices.values() if s.tier == "core"]
    # annotation_predicates omitted — LintConfig defaults to the canonical Rust
    # registry (the single source of truth since #630), exactly as production does.
    return gmeow_validate.LintConfig(
        str(NAMESPACE),
        str(ONTOLOGY_IRI),
        sorted(_SELECTOR_TOKENS),
        core,
    )


def _source_paths() -> list[str]:
    return [str(p) for p in iter_source_files()]


def _assert_matches_golden(name: str, errors: list[str], warnings: list[str]) -> None:
    golden = _golden(name)
    assert sorted(errors) == golden["errors"], f"{name}: errors drifted from golden"
    assert sorted(warnings) == golden["warnings"], (
        f"{name}: warnings drifted from golden"
    )


# --------------------------------------------------------------------------- #
# Direct gmeow_validate extension API over the real source paths.
# --------------------------------------------------------------------------- #


def test_structural_lint_rust_matches_golden() -> None:
    report = gmeow_validate.structural_lint(_source_paths(), _lint_config())
    _assert_matches_golden(
        "structural_lint", list(report["errors"]), list(report["warnings"])
    )


def test_term_naming_lint_rust_matches_golden() -> None:
    report = gmeow_validate.term_naming_lint(_source_paths(), _lint_config())
    _assert_matches_golden(
        "term_naming_lint", list(report["errors"]), list(report["warnings"])
    )


def test_slice_ownership_lint_rust_matches_golden() -> None:
    specs = [
        (str(module), f"{NAMESPACE}slices/{module.parent.name}")
        for module in iter_slice_module_files()
    ]
    report = gmeow_validate.slice_ownership_lint(specs, _lint_config())
    _assert_matches_golden(
        "slice_ownership_lint", list(report["errors"]), list(report["warnings"])
    )


def _declared_terms_from_rdflib() -> set[str]:
    """Independent live enumeration of declared GMEOW terms, via rdflib.

    Mirrors the Rust ``gmeow_validate.declared_terms`` definition
    (``crates/validate/src/lint.rs::collect_typed_terms``): every GMEOW-namespaced
    *named* IRI (``is_gmeow_term`` = ``startswith(namespace) or == ontology_iri``)
    that is the subject of at least one ``rdf:type`` triple; blank-node subjects
    are excluded. Computed over the same merged sources, so the two paths agree by
    construction rather than against a frozen snapshot.
    """
    ns, ont = str(NAMESPACE), str(ONTOLOGY_IRI)
    graph = load_merged_graph(include_imports=True)
    return {
        str(s)
        for s in graph.subjects(RDF.type, None)
        if isinstance(s, URIRef) and (str(s).startswith(ns) or str(s) == ont)
    }


def test_declared_terms_rust_matches_source() -> None:
    """``gmeow_validate.declared_terms`` agrees with an INDEPENDENT rdflib
    enumeration over the same sources — true parity that does not drift on every
    term addition (the prior frozen-golden form red-flagged on each new term).

    Also pins the structural contract the engine guarantees: the result is sorted,
    duplicate-free, non-empty, and every entry is a GMEOW-namespaced IRI.
    """
    terms = gmeow_validate.declared_terms(_source_paths(), _lint_config())
    ns, ont = str(NAMESPACE), str(ONTOLOGY_IRI)
    assert terms, "declared_terms must not be empty"
    assert terms == sorted(set(terms)), "declared_terms must be sorted and unique"
    assert all(t.startswith(ns) or t == ont for t in terms), (
        "every declared term must be a GMEOW-namespaced IRI"
    )
    assert set(terms) == _declared_terms_from_rdflib()


# --------------------------------------------------------------------------- #
# Python wrappers over the merged graph (the production validate_all path).
# These survive Task 5: the wrappers must keep reproducing the golden.
# --------------------------------------------------------------------------- #


def test_structural_lint_wrapper_matches_golden() -> None:
    # The wrapper now takes source paths (graph-free, #579) — the production
    # validate_all path. It must keep reproducing the golden.
    result = structural_lint(_source_paths())
    _assert_matches_golden("structural_lint", result.errors, result.warnings)


def test_term_naming_lint_wrapper_matches_golden() -> None:
    result = term_naming_lint(_source_paths())
    _assert_matches_golden("term_naming_lint", result.errors, result.warnings)


def test_slice_ownership_lint_wrapper_matches_golden() -> None:
    result = slice_ownership_lint()
    _assert_matches_golden("slice_ownership_lint", result.errors, result.warnings)
