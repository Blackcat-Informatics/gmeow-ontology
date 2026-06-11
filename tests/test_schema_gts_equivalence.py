# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""TRANSIENT (narrow waist PR 5, commit A): fold emitter ≡ rdflib emitter.

Exact dict equality of the LinkML schema (downstream JSON-Schema/Pydantic/
TS/GraphQL/OpenAPI artifacts follow automatically — they consume the YAML).
Deleted with the rdflib path in commit B.
"""

from __future__ import annotations

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.gts_views import load_fold
from gmeow_tools.schema_compile import emit_linkml, emit_linkml_fold


def test_fold_schema_dict_equals_rdflib_schema_dict() -> None:
    old_schema, old_warnings = emit_linkml(load_merged_graph(include_imports=False))
    new_schema, new_warnings = emit_linkml_fold(load_fold())
    assert new_schema == old_schema
    assert sorted(new_warnings) == sorted(old_warnings)
