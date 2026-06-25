# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Rust-backed context-aware up-projection surface (#942)."""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache
from types import ModuleType
from typing import cast

from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.up_projection import (
    LiftMap,
    UnsupportedLiftMapError,
    UpProjection,
    _native_project,
    build_lift_map,
)
from gmeow_tools.up_projection_audit import _ontology_nt, _projection_ttls, _sssom_texts


@dataclass(frozen=True)
class _Candidate:
    """Native context-resolution candidate adapted for test/debug imports."""

    gmeow: str
    context_type: str | None
    relation: str
    confidence: str


@dataclass(frozen=True)
class _Context:
    """Serialized native context inputs; resolution itself stays in Rust."""

    sssom_texts: tuple[str, ...]
    projection_ttls: tuple[str, ...]
    ontology_nt: str


def _pipeline() -> ModuleType:
    from gmeow_native import pipeline

    return cast(ModuleType, pipeline)


@lru_cache(maxsize=1)
def build_context() -> _Context:
    """Return the cached native context inputs for compatibility callers."""
    return _Context(_sssom_texts(), _projection_ttls(), _ontology_nt())


def _resolve(
    predicate: str, subject_types: set[str] | frozenset[str], ctx: _Context
) -> _Candidate | None:
    """Resolve one predicate in one subject-type context through Rust."""
    raw = _pipeline().up_projection_resolve_context(
        predicate,
        sorted(subject_types),
        list(ctx.sssom_texts),
        list(ctx.projection_ttls),
        ctx.ontology_nt,
    )
    if raw is None:
        return None
    return _Candidate(
        gmeow=raw["gmeow"],
        context_type=raw["context_type"],
        relation=raw["relation"],
        confidence=raw["confidence"],
    )


def up_project_descend(source: Graph, lift: LiftMap | None = None) -> UpProjection:
    """Lift a consumer graph using the native graph-descent resolver."""
    if lift is not None and lift != build_lift_map():
        raise UnsupportedLiftMapError
    return _native_project(source, descend=True)
