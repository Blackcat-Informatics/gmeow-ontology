// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The enumeration authority for every finding code the validator emits.
//!
//! This module replaces a source-scanning coherence heuristic
//! (`rule_catalog::every_emitted_code_literal_is_catalogued`, now removed) with
//! totality *by construction*: every literal finding code and every dynamic-code
//! family base/suffix is declared here exactly once as a `pub const`, every emit
//! site references the const (never a bare string literal), and
//! [`crate::rule_catalog::STATIC_RULES`] / `FAMILY_PREFIXES` / `FAMILY_SUFFIXES` are
//! built FROM these consts. [`ALL_CODES`] enumerates every static const so a test
//! can assert each one is classified — so a newly added const that is not wired
//! into a `STATIC_RULES` row or a family fails the build, not a grep.
//!
//! String VALUES here are byte-identical to what was emitted before this module
//! existed — this is a pure refactor of *where* the literal lives, never a change
//! to *what* is emitted (changing a value would churn generated goldens).

// ── Modelling disciplines (OntoUML / CONSTITUTION) ──────────────────────────

/// `crates/validate/src/gufo.rs` — stereotype-assignment discipline.
pub const DISCIPLINE_STEREOTYPE: &str = "discipline/stereotype";
/// `crates/validate/src/gufo.rs` — identity-overlap discipline.
pub const DISCIPLINE_IDENTITY_OVERLAP: &str = "discipline/identity-overlap";
/// `crates/validate/src/gufo.rs` — anti-rigidity discipline.
pub const DISCIPLINE_ANTI_RIGIDITY: &str = "discipline/anti-rigidity";
/// `crates/validate/src/gufo.rs` — relator-mediation discipline.
pub const DISCIPLINE_RELATOR_MEDIATION: &str = "discipline/relator-mediation";
/// `crates/validate/src/gufo.rs` — coequal-facet orthogonality discipline.
pub const DISCIPLINE_COEQUAL_ORTHOGONALITY: &str = "discipline/coequal-orthogonality";
/// `crates/validate/src/gufo.rs` — frame-completeness discipline.
pub const DISCIPLINE_FRAME_COMPLETENESS: &str = "discipline/frame-completeness";

// ── SHACL data-shape ─────────────────────────────────────────────────────────

/// `crates/validate/src/validate_all.rs` — non-conforming-with-no-results guard.
pub const SHACL_NONCONFORMING: &str = "shacl.nonconforming";
/// `crates/pipeline/src/stages/validate.rs` — a conforming run's informational
/// success record (emitted so the stage's attach delta is never empty). Declared
/// here so `remediation_for` resolves it to an honest `None` instead of inheriting
/// the `shacl.` family's "repair the data" fix prose — a "validation passed" record
/// is not a constraint violation. Deliberately NOT a `STATIC_RULES` row: a success
/// sentinel is not an enforced constraint, so it never enters the constraint catalog.
pub const SHACL_CLEAN: &str = "shacl.clean";
/// Family base for `format!("shacl.{ConstraintComponentLocalName}")`
/// (`crates/validate/src/findings.rs`).
pub const SHACL_FAMILY: &str = "shacl.";

// ── Bundle trust / signature ─────────────────────────────────────────────────

/// `crates/validate/src/signature.rs` — top-level verify failure.
pub const SIGNATURE_VERIFY: &str = "signature.verify";
/// `crates/validate/src/signature.rs` — key load / transport-key failure.
pub const SIGNATURE_INVALID: &str = "signature.invalid";
/// `crates/validate/src/signature.rs` — no signed frames found.
pub const SIGNATURE_MISSING: &str = "signature.missing";
/// `crates/validate/src/signature.rs` — signature key unresolved.
pub const SIGNATURE_UNVERIFIED: &str = "signature.unverified";
/// `crates/validate/src/signature.rs` — no trusted signer.
pub const SIGNATURE_UNTRUSTED: &str = "signature.untrusted";
/// `crates/validate/src/signature.rs` — resolved key info note.
pub const SIGNATURE_KEY: &str = "signature.key";
/// Family base for `format!("signature.{profile_finding_code}")`
/// (`crates/validate/src/signature.rs`).
pub const SIGNATURE_FAMILY: &str = "signature.";
/// Family base for `format!("gts.{reader_diagnostic_code}")`
/// (`crates/validate/src/signature.rs`).
pub const GTS_FAMILY: &str = "gts.";

// ── Deep-reason (`--deep`) semantic outcomes ─────────────────────────────────

/// `crates/validate/src/validate_all.rs` — `--deep` requested without a bundle.
pub const VALIDATE_DEEP_SKIPPED: &str = "validate.deep.skipped";
/// `crates/validate/src/validate_all.rs` — permitted/disclosed within-world glut.
pub const VALIDATE_DEEP_PERMITTED_CONFLICT: &str = "validate.deep.permitted-conflict";
/// `crates/validate/src/validate_all.rs` — forbidden contradiction witness.
pub const VALIDATE_DEEP_INCONSISTENT: &str = "validate.deep.inconsistent";
/// `crates/validate/src/validate_all.rs` — provably-empty (unsatisfiable) class.
pub const VALIDATE_DEEP_UNSATISFIABLE: &str = "validate.deep.unsatisfiable";
/// `crates/validate/src/validate_all.rs` — undecided DL construct.
pub const VALIDATE_DEEP_UNSUPPORTED_CONSTRUCT: &str = "validate.deep.unsupported-construct";
/// `crates/validate/src/validate_all.rs` — projection-loss ledger disclosure.
pub const VALIDATE_DEEP_PROJECTION_LOSS: &str = "validate.deep.projection-loss";
/// `crates/validate/src/validate_all.rs` — budget-exhausted / incomplete verdict.
pub const VALIDATE_DEEP_INCOMPLETE: &str = "validate.deep.incomplete";
/// `crates/validate/src/validate_all.rs` — consistent, fully-covered verdict.
pub const VALIDATE_DEEP_CONSISTENT: &str = "validate.deep.consistent";
/// `crates/validate/src/data_validate.rs` — garbled declared contract policy.
pub const VALIDATE_DEEP_CONTRACT_INVALID: &str = "validate.deep.contract-invalid";
/// `crates/validate/src/validate_all.rs` — a reasoning verdict named a clash quad
/// whose explain-skeleton derivation could not be built or located. An internal
/// invariant violation that HARD-FAILS the deep pass (never a graceful advisory).
pub const VALIDATE_DEEP_DERIVATION_UNRESOLVED: &str = "validate.deep.derivation-unresolved";
/// `crates/validate/src/data_validate.rs` — Tier-2 pass unavailable (graceful
/// degradation note).
pub const VALIDATE_DEEP_UNAVAILABLE: &str = "validate.deep.unavailable";
/// Family base for `validate.deep.*` (all rows above are also static so this
/// only covers members not otherwise enumerated).
pub const VALIDATE_DEEP_FAMILY: &str = "validate.deep.";

// ── Dev-governance / repo-structural ─────────────────────────────────────────

/// `crates/validate/src/constitution.rs` — enforced only by review practice.
pub const CONSTITUTION_HONOR_SYSTEM: &str = "constitution.honor-system";
/// `crates/validate/src/constitution.rs` — enforcement maps to no principle.
pub const CONSTITUTION_ORPHANED_ENFORCEMENT: &str = "constitution.orphaned-enforcement";
/// Family base for `format!("constitution.{code}")`
/// (`crates/validate/src/constitution.rs`, covers `undeclared-enforcement` and
/// `principle-unenforced` alongside the two static rows above).
pub const CONSTITUTION_FAMILY: &str = "constitution.";

/// `crates/validate/src/slice_ownership.rs` — a slice has no declared owner.
pub const SLICE_OWNERSHIP_UNOWNED: &str = "slice-ownership.unowned";
/// `crates/validate/src/slice_ownership.rs` — conflicting ownership claims.
pub const SLICE_OWNERSHIP_CONFLICT: &str = "slice-ownership.conflict";
/// `crates/validate/src/slice_ownership.rs` — ownership table mismatch.
pub const SLICE_OWNERSHIP_MISMATCH: &str = "slice-ownership.mismatch";
/// `crates/validate/src/slice_ownership.rs` — dependency used but undeclared.
pub const SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY: &str = "slice-ownership.undeclared-dependency";
/// `crates/validate/src/slice_ownership.rs` — declared dependency unreferenced.
pub const SLICE_OWNERSHIP_STALE_DEPENDENCY: &str = "slice-ownership.stale-dependency";
/// `crates/validate/src/slice_ownership.rs` — a slice query failed to parse.
pub const SLICE_OWNERSHIP_UNPARSEABLE_QUERY: &str = "slice-ownership.unparseable-query";
/// Family base for `slice-ownership.*`.
pub const SLICE_OWNERSHIP_FAMILY: &str = "slice-ownership.";

// ── Ontology-surface authoring gates (`crates/validate/src/authoring_integrity.rs`) ──
/// A `sh:NodeShape` IRI is declared in more than one shape file — merged into one
/// graph, the definitions fuse into a shape whose meaning depends on parse order.
pub const AUTHORING_SHAPE_IRI_COLLISION: &str = "authoring.shape-iri-collision";
/// A slice module's `owl:Ontology` IRI is absent from the generated XML catalog.
pub const AUTHORING_CATALOG_MISSING_MODULE: &str = "authoring.catalog-missing-module";
/// A slice module's `owl:Ontology` IRI does not match its location-derived IRI.
pub const AUTHORING_MODULE_IRI_MISMATCH: &str = "authoring.module-iri-mismatch";
/// A profile's `owl:imports` closure disagrees with the slice-tier partition
/// (full ≠ root ∪ extensions, claims ⊄ core, or a slice outside core/ext/profile).
pub const AUTHORING_PROFILE_CLOSURE: &str = "authoring.profile-closure";
/// The core `rights` module references a norms-extension IRI — the graft must live
/// on the extension side only, with zero core churn.
pub const AUTHORING_GRAFT_LEAK: &str = "authoring.graft-leak";
/// A fixture / example references a GMEOW vocabulary term that is not declared in
/// the ontology or any slice module (an undeclared predicate SHACL leaves inert).
pub const AUTHORING_UNDECLARED_TERM: &str = "authoring.undeclared-term";
/// A localizable-predicate literal in authored source carries no language tag — a
/// plain literal is a distinct RDF term from any tagged sibling, silently
/// untranslatable.
pub const AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL: &str = "authoring.untagged-localizable-literal";
/// Family base for `authoring.*`.
pub const AUTHORING_FAMILY: &str = "authoring.";

// ── Slice-discipline loader gates (`crates/validate/src/authoring_integrity.rs`) ──
/// Two slice manifests declare the same slice IRI — identity is manifest-only and
/// must be unique (the catalog loader would otherwise keep both silently).
pub const SLICE_DISCIPLINE_DUPLICATE_IRI: &str = "slice-discipline.duplicate-iri";
/// A `gmeow:Slice` manifest carries no `gmeow:sliceTier` — tier is mandatory
/// (the loader would otherwise accept a silent `None`).
pub const SLICE_DISCIPLINE_MISSING_TIER: &str = "slice-discipline.missing-tier";
/// Family base for `slice-discipline.*`.
pub const SLICE_DISCIPLINE_FAMILY: &str = "slice-discipline.";

/// `crates/validate/src/crate_layering.rs` — a first-party layering violation.
pub const CRATE_LAYERING_VIOLATION: &str = "crate-layering.violation";
/// `crates/validate/src/crate_layering.rs` — a non-failing layering observation.
pub const CRATE_LAYERING_OBSERVATION: &str = "crate-layering.observation";

/// `crates/validate/src/repo_static.rs` — a repo-structural violation.
pub const REPO_STATIC_VIOLATION: &str = "repo-static.violation";
/// `crates/validate/src/repo_static.rs` — a non-failing structural observation.
pub const REPO_STATIC_OBSERVATION: &str = "repo-static.observation";

/// `crates/validate/src/coverage.rs` — an external class used but unaligned.
pub const COVERAGE_GAP_CLASS: &str = "coverage.gap-class";
/// `crates/validate/src/coverage.rs` — an external predicate used but unaligned.
pub const COVERAGE_GAP_PREDICATE: &str = "coverage.gap-predicate";

/// `crates/validate/src/box_roles.rs` — a box-role audit is missing coverage.
pub const BOX_ROLES_MISSING: &str = "box-roles.missing";
/// `crates/validate/src/box_roles.rs` — a box-role audit is invalid.
pub const BOX_ROLES_INVALID: &str = "box-roles.invalid";

/// `crates/validate/src/mapping_eval.rs` — a malformed Wikidata QID.
pub const WIKIDATA_QID_SYNTAX: &str = "wikidata.qid-syntax";
/// `crates/validate/src/mapping_eval.rs` — a Wikidata namespace misuse.
pub const WIKIDATA_NAMESPACE_MISUSE: &str = "wikidata.namespace-misuse";

/// `crates/validate/src/statement.rs` — a statement-metadata invariant
/// violation (base-triple groundedness / DL datatypes / annotation soundness /
/// preferred-rank).
pub const STATEMENT_INVARIANT: &str = "statement.invariant";
/// `crates/validate/src/statement.rs` — the RDF-1.2 ↔ OWL round-trip is lossy.
pub const STATEMENT_COMPILE_LOSSLESS_ROUND_TRIP: &str = "statement-compile.lossless-round-trip";

// ── Bundle ontology completeness (`gmeow verify`) ────────────────────────────

/// `crates/gmeow-cli/src/commands.rs` — a documented bundle term (class/property)
/// carries no `rdfs:label`. A completeness gap surfaced by `gmeow verify`; a
/// non-blocking Warning (a bundle that passes verify today carries zero missing
/// labels, so this never changes a clean bundle's exit code).
pub const ONTOLOGY_MISSING_LABEL: &str = "ontology.missing-label";
/// `crates/gmeow-cli/src/commands.rs` — a documented bundle term (class/property)
/// carries no `skos:definition`. A completeness gap surfaced by `gmeow verify`; a
/// non-blocking Warning, for the same reason as [`ONTOLOGY_MISSING_LABEL`].
pub const ONTOLOGY_MISSING_DEFINITION: &str = "ontology.missing-definition";

// ── Input well-formedness ────────────────────────────────────────────────────

/// `crates/validate/src/validate_all.rs` — an example file failed to parse.
pub const EXAMPLE_PARSE: &str = "example.parse";

// ── Soft advisory ─────────────────────────────────────────────────────────

/// Family base for `advice.*` (e.g. [`ADVICE_TIER_ACTIVE`], and every harvested
/// advisory rule).
pub const ADVICE_FAMILY: &str = "advice.";

/// `crates/validate/src/validate_all.rs` — the fixed demonstrator advisory
/// emitted on every normal-completion validate run.
pub const ADVICE_TIER_ACTIVE: &str = "advice.tier.active";

// ── Dynamic per-DSL SHACL failure suffix ─────────────────────────────────────

/// Family suffix for `format!("{label}-dsl.nonconforming")`
/// (`crates/validate/src/dsl_shacl.rs`).
pub const DSL_NONCONFORMING_SUFFIX: &str = "-dsl.nonconforming";

/// Every statically-declared literal finding code, for the compile-time
/// totality gate: `every_declared_code_is_classified`
/// iterates this array and asserts each is recognised by
/// [`crate::rule_catalog::is_known`]. A code const added here without a matching
/// `STATIC_RULES` row (or family prefix/suffix membership) fails that test.
pub const ALL_CODES: &[&str] = &[
    DISCIPLINE_STEREOTYPE,
    DISCIPLINE_IDENTITY_OVERLAP,
    DISCIPLINE_ANTI_RIGIDITY,
    DISCIPLINE_RELATOR_MEDIATION,
    DISCIPLINE_COEQUAL_ORTHOGONALITY,
    DISCIPLINE_FRAME_COMPLETENESS,
    SHACL_NONCONFORMING,
    SHACL_CLEAN,
    SIGNATURE_VERIFY,
    SIGNATURE_INVALID,
    SIGNATURE_MISSING,
    SIGNATURE_UNVERIFIED,
    SIGNATURE_UNTRUSTED,
    SIGNATURE_KEY,
    VALIDATE_DEEP_SKIPPED,
    VALIDATE_DEEP_PERMITTED_CONFLICT,
    VALIDATE_DEEP_INCONSISTENT,
    VALIDATE_DEEP_UNSATISFIABLE,
    VALIDATE_DEEP_UNSUPPORTED_CONSTRUCT,
    VALIDATE_DEEP_PROJECTION_LOSS,
    VALIDATE_DEEP_INCOMPLETE,
    VALIDATE_DEEP_CONSISTENT,
    VALIDATE_DEEP_CONTRACT_INVALID,
    VALIDATE_DEEP_DERIVATION_UNRESOLVED,
    VALIDATE_DEEP_UNAVAILABLE,
    CONSTITUTION_HONOR_SYSTEM,
    CONSTITUTION_ORPHANED_ENFORCEMENT,
    SLICE_OWNERSHIP_UNOWNED,
    SLICE_OWNERSHIP_CONFLICT,
    SLICE_OWNERSHIP_MISMATCH,
    SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY,
    SLICE_OWNERSHIP_STALE_DEPENDENCY,
    SLICE_OWNERSHIP_UNPARSEABLE_QUERY,
    CRATE_LAYERING_VIOLATION,
    CRATE_LAYERING_OBSERVATION,
    REPO_STATIC_VIOLATION,
    REPO_STATIC_OBSERVATION,
    COVERAGE_GAP_CLASS,
    COVERAGE_GAP_PREDICATE,
    BOX_ROLES_MISSING,
    BOX_ROLES_INVALID,
    WIKIDATA_QID_SYNTAX,
    WIKIDATA_NAMESPACE_MISUSE,
    STATEMENT_INVARIANT,
    STATEMENT_COMPILE_LOSSLESS_ROUND_TRIP,
    ONTOLOGY_MISSING_LABEL,
    ONTOLOGY_MISSING_DEFINITION,
    EXAMPLE_PARSE,
    AUTHORING_SHAPE_IRI_COLLISION,
    AUTHORING_CATALOG_MISSING_MODULE,
    AUTHORING_MODULE_IRI_MISMATCH,
    AUTHORING_PROFILE_CLOSURE,
    AUTHORING_GRAFT_LEAK,
    AUTHORING_UNDECLARED_TERM,
    AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL,
    SLICE_DISCIPLINE_DUPLICATE_IRI,
    SLICE_DISCIPLINE_MISSING_TIER,
];

/// Every dynamic-code family base declared here, paired with the module that
/// classifies via it. Not iterated by a test directly (families match by
/// prefix/suffix, not equality) but kept alongside `ALL_CODES` as the single
/// place a new family base is declared.
pub const ALL_FAMILY_PREFIXES: &[&str] = &[
    SHACL_FAMILY,
    SIGNATURE_FAMILY,
    GTS_FAMILY,
    VALIDATE_DEEP_FAMILY,
    CONSTITUTION_FAMILY,
    SLICE_OWNERSHIP_FAMILY,
    ADVICE_FAMILY,
];

/// Every dynamic-code family suffix declared here.
pub const ALL_FAMILY_SUFFIXES: &[&str] = &[DSL_NONCONFORMING_SUFFIX];
