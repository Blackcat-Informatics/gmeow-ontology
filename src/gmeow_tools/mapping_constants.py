# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Shared constants for the mapping / saturation / alignment pipeline.

These frozensets used to live in :mod:`gmeow_tools.alignment_lint`; they are
authored here so the saturator can import them without depending on the
retiring Python alignment linter (#936).
"""

from __future__ import annotations

#: Predicate CURIEs whose alignment asserts (near-)equivalence for properties.
#: PUBLIC: the saturator may materialize cross-vocabulary triples only for these.
STRONG_PROPERTY_PREDICATES: frozenset[str] = frozenset(
    {"owl:equivalentProperty", "skos:exactMatch"}
)

#: Class-level strong equivalence (the collapse gate's edge set, also the
#: saturator's class-edge authorization).
STRONG_CLASS_PREDICATES: frozenset[str] = frozenset(
    {"owl:equivalentClass", "skos:exactMatch"}
)
