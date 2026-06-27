# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Type stub for the gmeow_validate PyO3 extension (#819 Task 10).
#
# Signatures transcribed verbatim from crates/validate/src/py.rs — keep in
# lockstep with that file (the ABI source of truth).  The language-tag functions
# (is_internal_tag, rank_language, load_tag_map) are the new additions in
# Task 10; the rest mirrors the existing validated surface.

from __future__ import annotations

from typing import Any

# ── Classes ──────────────────────────────────────────────────────────────────

class LintConfig:
    def __init__(
        self,
        namespace: str,
        ontology_iri: str,
        selector_tokens: list[str],
        core_slice_iris: list[str],
        annotation_predicates: list[str] | None = None,
    ) -> None: ...

class SignatureConfig:
    def __init__(
        self,
        trusted_signers: list[str] | None = None,
        require_signatures: bool = False,
        require_trusted_signer: bool = False,
        trusted_key: str | None = None,
    ) -> None: ...

class ValidateOptions:
    def __init__(
        self,
        timings: bool = False,
        sameas_allowlist: list[tuple[str, str]] | None = None,
        slices_dir: str | None = None,
        mapping_shapes_ttl: str | None = None,
        statement_shapes_ttl: str | None = None,
        test_dsl_dir: str | None = None,
        test_dsl_shapes_ttl: str | None = None,
        project_root: str | None = None,
        gts_bytes: bytes | None = None,
        signature_config: SignatureConfig | None = None,
        deep: bool = False,
    ) -> None: ...

class ValidationStore:
    def __init__(self, source_paths: list[str]) -> None: ...
    @staticmethod
    def from_gts_bytes(gts_bytes: bytes) -> ValidationStore: ...
    def _store_capsule(self) -> Any: ...
    def validate(self, shapes: Any) -> Any: ...

# ── Annotation predicate registry ────────────────────────────────────────────

def annotation_predicates() -> list[str]: ...

# ── Language-tag policy core (#819 Task 10) ──────────────────────────────────

def is_internal_tag(lang: str) -> bool: ...
def rank_language(lang: str) -> tuple[int, str]: ...
def load_tag_map(rdf_bytes: bytes, format: str) -> dict[str, str]: ...

# ── Lints ─────────────────────────────────────────────────────────────────────

def structural_lint(
    source_paths: list[str],
    cfg: LintConfig,
) -> dict[str, list[str]]: ...
def term_naming_lint(
    source_paths: list[str],
    cfg: LintConfig,
) -> dict[str, list[str]]: ...
def typed_terms(
    source_paths: list[str],
    cfg: LintConfig,
) -> list[tuple[str, str]]: ...
def declared_terms(
    source_paths: list[str],
    cfg: LintConfig,
) -> list[str]: ...
def check_syntax(paths: list[str]) -> dict[str, list[str]]: ...
def validate_instance(
    instance_bytes: bytes,
    format: str,
    schema_bytes: bytes,
) -> dict[str, list[str]]: ...
def validate_data(
    data_bytes: bytes,
    data_format: str,
    gts_bytes: bytes,
    namespace: str,
    origin: str,
) -> Any: ...
def check_sameas_ban(
    paths: list[str],
    namespace: str,
    allowlist: list[tuple[str, str]],
) -> dict[str, list[str]]: ...

# ── Reasoning invariants ─────────────────────────────────────────────────────

def reasoning_invariants(
    source_paths: list[str],
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_invariants_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_exactly_one_stereotype_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_identity_overlap_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_anti_rigidity_discipline_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_relator_mediation_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_coequal_facet_orthogonality_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...
def reasoning_frame_declaration_completeness_nt(
    data_nt: str,
    namespace: str,
) -> dict[str, list[str]]: ...

# ── Coverage ─────────────────────────────────────────────────────────────────

def coverage_analyze(
    fixture_paths: list[str],
    aligned: list[str],
    namespace: str,
) -> dict[str, list[str]]: ...
def wikidata_check_syntax_iri(
    iri: str,
    in_object_position: bool,
) -> list[tuple[str, str, str]]: ...
def wikidata_mapping_syntax(mappings_dir: str) -> dict[str, Any]: ...
def wikidata_collect_ids(mappings_dir: str) -> list[str]: ...
def wikidata_diagnostics_report(mappings_dir: str) -> Any: ...
def crate_layering_check(crates_dir: str) -> dict[str, Any]: ...
def crate_layering_diagnostics_report(crates_dir: str) -> Any: ...
def wikidata_check_existence(
    identifiers: list[str],
    project_root: str,
    timeout: float = 30.0,
    chunk_size: int = 50,
    delay: float = 0.1,
) -> dict[str, str]: ...
def wikidata_coverage_report(
    root: str,
    mappings_dir: str,
    threshold: float = 0.5,
    json_mode: bool = False,
) -> str: ...
def dc_coverage_report(
    mappings_dir: str,
    threshold: float = 0.5,
    json_mode: bool = False,
) -> str: ...

# ── DSL utilities ────────────────────────────────────────────────────────────

def merge_to_ntriples(source_paths: list[str]) -> str: ...
def dsl_merge_with_provenance(
    dsl_paths: list[str],
) -> tuple[str, list[tuple[str, str]]]: ...
def validate_dsl_shacl(dsl_paths: list[str], shapes_ttl: str) -> list[str]: ...

# ── Validation orchestration ─────────────────────────────────────────────────

def validate_all_native(
    source_paths: list[str],
    shapes_ttl: str,
    mapping_dsl_dir: str,
    statement_dsl_dir: str,
    config: LintConfig,
    options: ValidateOptions,
) -> dict[str, Any]: ...
def check_statement_invariants(
    statement_owl_ttl: str,
    ontology_nt: str,
) -> Any: ...
def check_statement_lossless(
    authored_owl_ttl: str,
    normalized_owl_ttl: str,
) -> Any: ...
def slice_ownership_report(slices_root: str) -> Any: ...
def constitution_enforcement_report(manifest_ttl: str) -> Any: ...
def constitution_full_report(
    manifest_path: str,
    constitution_path: str,
    root: str,
) -> Any: ...

# ── CrossRef deposit-XML (Task 11, #819) ─────────────────────────────────────

def build_deposit_xml_native(
    self_description_json: str,
    timestamp: str,
    batch_id: str,
) -> str: ...
def lint_deposit_native(self_description_json: str) -> list[str]: ...
