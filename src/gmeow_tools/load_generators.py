# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Central import helper that registers every GMEOW artifact generator.

The generator modules use the ``@register`` decorator side-effect to populate
``gmeow_tools.generator._REGISTRY``.  Rather than listing every module by hand
in the CLI, the constitution gate, and the Makefile commit target, callers can
import ``load_all`` and invoke it once.
"""

from __future__ import annotations

MODULES = (
    "apache",
    "catalog_gen",
    "evals",
    "export",
    "frame_shapes_gen",
    "gts_full_gen",
    "gts_gen",
    "gts_vectors_gen",
    "lpg",
    "mapping_compile",
    "matrix",
    "metadata",
    "ontology_docs",
    "parquet_gen",
    "profiles_gen",
    "references",
    "research_objects",
    "schema_compile",
    "statement_compile",
)


def load_all() -> None:
    """Import every generator module to trigger ``@register`` side effects."""
    from gmeow_tools import generator  # noqa: F401  (ensure package is loaded)

    for name in MODULES:
        __import__(f"gmeow_tools.{name}")
