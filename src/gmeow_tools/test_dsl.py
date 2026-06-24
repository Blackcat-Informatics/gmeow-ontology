"""Load and SHACL-validate the GMEOW test DSL — slice-resident declarative tests.

The test DSL (vocabulary in ``dsl/tests/vocabulary.ttl``) is the canonical
authoring vocabulary for slice-resident declarative test fixtures: competency
questions, structural assertions, and example-conformance fixtures, authored AS
ontology data in each slice's ``tests/`` directory (``slices/*/*/tests/*.ttl``).

This module is the load-then-validate seam: it gathers the vocabulary plus every
slice-resident fixture file and validates the merged graph against the test-DSL
SHACL shapes (``shapes/test-dsl-shapes.ttl``) via
:func:`gmeow_tools.dsl_validate.validate_test_dsl`, raising
:exc:`~gmeow_tools.dsl_validate.DslValidationError` on any violation. It is a
spec layer — grounded in the ``gmeow:`` namespace, never owl:imports-ed into
the reasoned OWL 2 DL core.

Execution of the test specs themselves (running the SPARQL, checking outcomes)
is a later concern; this module only makes the specs SHACL-validated and
well-formed. The split mirrors the mapping and statement DSL loaders.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import DSL_TESTS_DIR
from gmeow_tools.dsl_validate import DslValidationError, validate_test_dsl
from gmeow_tools.slices import iter_slice_test_files


def _test_dsl_sources(vocab_dir: Path = DSL_TESTS_DIR) -> list[Path]:
    """Gather the test-DSL vocabulary plus every slice-resident fixture file.

    The vocabulary (``dsl/tests/vocabulary.ttl``) is merged with each
    ``slices/*/*/tests/*.ttl`` fixture so SHACL targets that reference vocabulary
    terms resolve against the same graph. Returns a deterministically-ordered
    list (vocabulary first, then the sorted slice fixtures).
    """
    sources: list[Path] = sorted(vocab_dir.glob("*.ttl"))
    sources += iter_slice_test_files()
    return sources


def load_test_dsl(vocab_dir: Path = DSL_TESTS_DIR) -> tuple[Path, ...]:
    """Load and SHACL-validate the whole test DSL.

    Merges the test-DSL vocabulary with every slice-resident ``tests/*.ttl``
    fixture and validates the result against the test-DSL SHACL shapes. The
    actual execution of the test specs is a later concern; this entry point only
    enforces that the specs are well-formed.

    Returns:
        The validated source paths, in the order they were merged.

    Raises:
        DslValidationError: When any fixture (or the vocabulary) violates the
            test-DSL SHACL shapes.
    """
    sources = _test_dsl_sources(vocab_dir)
    violations = validate_test_dsl([str(path) for path in sources])
    if violations:
        raise DslValidationError(
            "test DSL SHACL violations:\n  " + "\n  ".join(violations)
        )
    return tuple(sources)
