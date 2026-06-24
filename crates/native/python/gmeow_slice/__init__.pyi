# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Type stub for the gmeow_slice PyO3 extension (#820 S8).
#
# Signatures transcribed verbatim from crates/slice/src/py.rs — keep in lockstep
# with that file (the ABI source of truth). This is the authoritative native
# slice catalog + ownership/dependency analyzer surface.

from __future__ import annotations

from typing import TypedDict

# ── Classes ──────────────────────────────────────────────────────────────────

class ArtifactRecord:
    @property
    def role(self) -> str: ...
    @property
    def logical_path(self) -> str: ...
    @property
    def media_type(self) -> str: ...
    @property
    def raw_digest(self) -> str: ...
    @property
    def semantic_digest(self) -> str | None: ...
    @property
    def content(self) -> bytes: ...

class ManifestView:
    @property
    def slice_iri(self) -> str: ...
    @property
    def label(self) -> str | None: ...
    @property
    def title(self) -> str | None: ...
    @property
    def creators(self) -> list[str]: ...
    @property
    def identifier(self) -> str | None: ...
    @property
    def tier(self) -> str | None: ...
    @property
    def consumers(self) -> list[str]: ...

class SliceRecord:
    @property
    def manifest(self) -> ManifestView: ...
    @property
    def artifacts(self) -> list[ArtifactRecord]: ...
    @property
    def slice_dir(self) -> str: ...
    @property
    def manifest_path(self) -> str: ...

class ManifestPatch:
    @property
    def manifest_path(self) -> str: ...
    @property
    def original_text(self) -> str: ...
    @property
    def patched_text(self) -> str: ...

class SliceCatalog:
    @staticmethod
    def discover(root: str) -> SliceCatalog: ...
    def records(self) -> list[SliceRecord]: ...
    def core_slice_iris(self) -> list[str]: ...
    def fix_deps(self) -> list[ManifestPatch]: ...

class DependencyEdge:
    @property
    def from_slice(self) -> str: ...
    @property
    def to_slice(self) -> str: ...
    @property
    def reconciliation(self) -> str: ...
    @property
    def is_semantic(self) -> bool: ...

class OwnershipReport:
    @property
    def edges(self) -> list[DependencyEdge]: ...
    def ownership_errors(self) -> list[str]: ...
    def has_ownership_defect(self) -> bool: ...

class OwnershipAnalyzer:
    def __init__(self, catalog: SliceCatalog) -> None: ...
    def analyze(self) -> OwnershipReport: ...
    def analysis_graph_turtle(
        self,
        authored_input_text: str,
        compiler_version: str,
        reasoning_profile: str,
    ) -> str: ...

# ── Module-level functions ─────────────────────────────────────────────────────

def emit_sssom(root: str) -> dict[str, str]:
    """Emit every SSSOM TSV from the repo at ``root``.

    Returns ``{sssom_file: tsv_text}`` (bare file names), byte-identical to the
    historical Python emitter (#848). Sources every input natively from ``root``:
    slice mapping artifacts, the shared ``dsl/mappings/`` tree, the prefix map,
    and ``metadata/gmeow-self.ttl`` for the version + release date.
    """
    ...

def emit_fno(root: str) -> str:
    """Emit the FnO function catalog from the repo at ``root``.

    Returns the ``functions.fno.ttl`` graph as full-IRI N-Triples text,
    graph-isomorphic to the historical Python emitter (#848). Sources every input
    natively from ``root``: the projection functions + cells from the
    ``dsl/mappings/`` tree + slice mapping artifacts, and each input predicate's
    ``rdfs:range`` from ``ontology/gmeow.ttl`` + slice module artifacts.
    """
    ...

# One ``ProjectionDiagnostic`` dict per cross-layer projection-lint problem. The
# shape matches ``gmeow_rdf.SssomDiagnostic`` so the Python finding leg packs both
# the same way; ``check`` is the drift family (``fno-type`` / ``fno-ref`` /
# ``spec-drift``) the leg maps to the ``mapping-compile.<check>`` code.
class ProjectionDiagnostic(TypedDict):
    severity: str
    code: str
    message: str
    check: str
    instance: str | None
    subject_id: str | None
    predicate_id: str | None
    object_id: str | None

def lint_projection(
    root: str, allow_network: bool = False
) -> list[ProjectionDiagnostic]:
    """Run the native cross-layer projection lint over ``root``'s committed tree.

    Ports the three projection-lint invariants (#854): FnO type-mismatch
    (``fno-type``), EDOAL→FnO reference integrity (``fno-ref``), and
    CONSTRUCT↔EDOAL↔SSSOM spec-drift (``spec-drift``), plus the alignment-direction
    checks (#936): ``inverse-direction``, ``domain-range``, ``property-character``,
    ``equivalence-collapse``, ``dc-refinement``, and ``dc-hand-authored``. Reads the
    committed
    ``generated/projections/*.{fno.ttl,edoal.ttl}`` + ``generated/queries/*.rq``,
    the ontology ``rdfs:range``s, and the SSSOM alignment.

    ``allow_network`` permits live fetching of missing target-axiom snapshots for the
    alignment-direction checks (default ``False``).

    An empty list means the projection stack and SSSOM alignments are internally
    consistent.
    """
    ...
