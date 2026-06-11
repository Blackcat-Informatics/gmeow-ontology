# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""TRANSIENT (narrow waist PR 6, commit A): fold LPG ≡ pyoxigraph LPG.

Value-normalized comparison per the plan: nodes by id with multiset-
normalized multi-valued properties; edges as a (source, target, type,
normalized-props) multiset — never by edge_id, which hashes JSON list order.
dateTime lexical forms are instant-normalized: the old pyoxigraph parser
Z-normalizes what the canonical rdf12 source writes as +00:00; the fold is
verbatim-faithful to the source. Deleted with the old path in commit B.
"""

from __future__ import annotations

import json
import re
from collections import Counter

from gmeow_tools.gts_views import load_fold
from gmeow_tools.lpg import _load_store, build_lpg, build_lpg_fold


def _norm(value: object) -> object:
    if isinstance(value, list):
        return sorted(json.dumps(_norm(x), sort_keys=True) for x in value)
    if isinstance(value, dict):
        return {k: _norm(v) for k, v in value.items()}
    if isinstance(value, str):
        return re.sub(r"\+00:00$", "Z", value)
    return value


def _props(properties: dict[str, object]) -> str:
    return json.dumps({k: _norm(v) for k, v in properties.items()}, sort_keys=True)


def test_fold_lpg_equals_pyoxigraph_lpg() -> None:
    old = build_lpg(_load_store())
    new = build_lpg_fold(load_fold())

    old_nodes = {n.id: (n.labels, _props(n.properties)) for n in old.nodes}
    new_nodes = {n.id: (n.labels, _props(n.properties)) for n in new.nodes}
    assert new_nodes == old_nodes

    old_edges = Counter(
        (e.source, e.target, e.type, _props(e.properties)) for e in old.edges
    )
    new_edges = Counter(
        (e.source, e.target, e.type, _props(e.properties)) for e in new.edges
    )
    assert new_edges == old_edges
