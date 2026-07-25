//! The validator's rule-identity registry — the single authority for *what*
//! finding codes the validator can emit, and the seam by which every finding
//! code resolves to a constraint-catalog entry (the "what GMEOW enforces"
//! surface).
//!
//! # The irreducible line
//!
//! A finding code exists *only because* a Rust check mints it (e.g.
//! `Finding::new(Severity::Error, codes::DISCIPLINE_RELATOR_MEDIATION, …)`). The
//! set of codes, each code's default grade, and the *kind* of thing it enforces
//! are therefore intrinsic Rust facts and live here. Every literal code value is
//! declared exactly once, in [`crate::codes`] — emit sites reference the const,
//! never a bare string — so totality (every emitted code is catalogued) holds by
//! construction and is checked at build time by
//! `tests::every_declared_code_is_classified`, not by scanning source text.
//! Everything human-readable — the per-term description and the category — is
//! **generated** from the reasoned graph by the constraint-catalog pipeline
//! stage, never authored here, so the catalog stays a projection of the axioms
//! rather than a hand-maintained list.
//!
//! This module owns exactly four things:
//!
//! * [`slugify`] / [`help_uri_for`] — the *single* anchor transform shared by the
//!   validator (finding `helpUri`) and the docs renderer, so a finding code and
//!   its catalog page anchor can never disagree.
//! * [`catalog_anchor_uri`] — resolves a *concrete* finding code (which may be a
//!   dynamic family member with no catalog row of its own, e.g.
//!   `shacl.MinCountConstraintComponent`) to the `help_uri` of the catalog entry
//!   that actually documents it — its own row if static, otherwise the family
//!   representative's row.
//! * [`Enforcement`] + [`STATIC_RULES`] + the family classifiers — the minimal
//!   `{code → default severity, enforcement kind}` seeds.
//! * [`rule_for`] / [`populate_rules`] — populate a report's `rules` so every
//!   emitted code carries a rule entry whose `helpUri` resolves to the catalog.
//! * [`all_rules`] — the enumeration the catalog generator projects from.

use crate::codes;
use gmeow_errors::{Report, Rule, Severity};
use std::collections::BTreeSet;

/// The canonical documentation base the catalog page is served at; a code's
/// entry is the fragment anchored by its [`slugify`]-ed form.
pub const CATALOG_BASE_URI: &str = "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints";

/// The kind of thing an enforced check constrains — the coarse *seed* the
/// constraint-catalog generator refines (into a `logic:FindingCategory` and the
/// per-term prose) by resolving against the reasoned graph. Intentionally coarse:
/// the prose/principle pointer is NOT stored here (it is resolved from the graph
/// via `logic:formalizes`), only the enforcement kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    /// An OntoUML / CONSTITUTION modelling discipline (stereotype, identity,
    /// anti-rigidity, relator mediation, facet orthogonality, frame declaration).
    Discipline,
    /// A SHACL constraint-shape projection of a `logic:` axiom (P17).
    Shacl,
    /// Bundle trust / signature policy over the `gmeow.gts` a consumer loads.
    Signature,
    /// A deep-reason (`--deep`) semantic outcome over the reasoned bundle.
    DeepReason,
    /// A repo-structural / dev-governance check (developer CLI, not consumer data).
    Governance,
    /// Input well-formedness (parse / example) before any enforcement runs.
    Parse,
    /// A soft advisory (`advice.*`) — recommendation, not a violation.
    Advisory,
}

/// One registry seed: a code, the grade it defaults to, and what it enforces.
#[derive(Debug, Clone)]
pub struct RuleSeed {
    pub code: &'static str,
    pub default_severity: Severity,
    pub enforcement: Enforcement,
    /// Whether `code` is a dynamic *family* representative (e.g. `shacl.*`) rather
    /// than a single literal code. Family entries anchor one catalog entry for the
    /// whole family; the generator renders them as a pattern.
    pub family: bool,
    /// The registry-authored rule-level remediation prose ([`remediation_for`]), or `None`
    /// for a code on the honest-absence allowlist. The constraint-catalog generator
    /// projects it as `gmeow:ruleRemediation`.
    pub remediation: Option<&'static str>,
}

/// Every statically-known finding code the validator can emit, with its default
/// grade and enforcement kind. Dynamic codes (built with `format!`) are covered
/// by [`FAMILY_PREFIXES`] / [`FAMILY_SUFFIXES`] instead of one row each.
///
/// Each row's code is a [`codes`] const, never a raw literal — [`codes`] is the
/// single authority for every emitted code, and
/// `tests::every_declared_code_is_classified` fails the build if a const exists
/// there without a corresponding row (or family) here, so this table stays total
/// by construction rather than by a source-scanning heuristic.
pub const STATIC_RULES: &[(&str, Severity, Enforcement)] = &[
    // ── Modelling disciplines (OntoUML / CONSTITUTION) — data- and vocab-facing ──
    (
        codes::DISCIPLINE_STEREOTYPE,
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        codes::DISCIPLINE_IDENTITY_OVERLAP,
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        codes::DISCIPLINE_ANTI_RIGIDITY,
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        codes::DISCIPLINE_RELATOR_MEDIATION,
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        codes::DISCIPLINE_FRAME_COMPLETENESS,
        Severity::Error,
        Enforcement::Discipline,
    ),
    // ── SHACL data-shape (the non-family static outcome) ──
    (
        codes::SHACL_NONCONFORMING,
        Severity::Error,
        Enforcement::Shacl,
    ),
    // ── Bundle trust / signature ──
    (
        codes::SIGNATURE_VERIFY,
        Severity::Error,
        Enforcement::Signature,
    ),
    (
        codes::SIGNATURE_INVALID,
        Severity::Error,
        Enforcement::Signature,
    ),
    (
        codes::SIGNATURE_MISSING,
        Severity::Error,
        Enforcement::Signature,
    ),
    (
        codes::SIGNATURE_UNVERIFIED,
        Severity::Error,
        Enforcement::Signature,
    ),
    (
        codes::SIGNATURE_UNTRUSTED,
        Severity::Error,
        Enforcement::Signature,
    ),
    (codes::SIGNATURE_KEY, Severity::Info, Enforcement::Signature),
    // ── Deep-reason (`--deep`) semantic outcomes ──
    (
        codes::VALIDATE_DEEP_SKIPPED,
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_PERMITTED_CONFLICT,
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_INCONSISTENT,
        Severity::Error,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_UNSATISFIABLE,
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_UNSUPPORTED_CONSTRUCT,
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_PROJECTION_LOSS,
        Severity::Note,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_INCOMPLETE,
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_CONSISTENT,
        Severity::Note,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_CONTRACT_INVALID,
        Severity::Error,
        Enforcement::DeepReason,
    ),
    (
        codes::VALIDATE_DEEP_UNAVAILABLE,
        Severity::Note,
        Enforcement::DeepReason,
    ),
    // ── Dev-governance / repo-structural (developer CLI) ──
    (
        codes::CONSTITUTION_HONOR_SYSTEM,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::CONSTITUTION_ORPHANED_ENFORCEMENT,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_UNOWNED,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_CONFLICT,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_MISMATCH,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_STALE_DEPENDENCY,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_UNPARSEABLE_QUERY,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_PEERED_UNREGISTERED_SEAM,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY,
        Severity::Error,
        Enforcement::Governance,
    ),
    // ── Ontology-surface authoring gates ──
    (
        codes::AUTHORING_SHAPE_IRI_COLLISION,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_CATALOG_MISSING_MODULE,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_MODULE_IRI_MISMATCH,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_PROFILE_CLOSURE,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_GRAFT_LEAK,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_UNDECLARED_TERM,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::AUTHORING_SEAM_REGISTRY_DRIFT,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_DISCIPLINE_DUPLICATE_IRI,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_DISCIPLINE_MISSING_TIER,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_DISCIPLINE_NON_GROUNDING_PEERAGE,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_DISCIPLINE_GROUNDING_MARKER_DRIFT,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::CRATE_LAYERING_VIOLATION,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::CRATE_LAYERING_OBSERVATION,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::REPO_STATIC_VIOLATION,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::REPO_STATIC_OBSERVATION,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::COVERAGE_GAP_CLASS,
        Severity::Info,
        Enforcement::Governance,
    ),
    (
        codes::COVERAGE_GAP_PREDICATE,
        Severity::Info,
        Enforcement::Governance,
    ),
    (
        codes::BOX_ROLES_MISSING,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::BOX_ROLES_INVALID,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::WIKIDATA_QID_SYNTAX,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::WIKIDATA_NAMESPACE_MISUSE,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::STATEMENT_INVARIANT,
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        codes::STATEMENT_COMPILE_LOSSLESS_ROUND_TRIP,
        Severity::Error,
        Enforcement::Governance,
    ),
    // ── Bundle ontology completeness (`gmeow verify`) — governance-ish ──
    // Non-blocking Warnings: a bundle that passes `gmeow verify` today carries
    // zero missing labels/definitions, so these never change a clean bundle's
    // exit code (they were previously informational `println!` rows).
    (
        codes::ONTOLOGY_MISSING_LABEL,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::ONTOLOGY_MISSING_DEFINITION,
        Severity::Warning,
        Enforcement::Governance,
    ),
    // ── Input well-formedness ──
    (codes::EXAMPLE_PARSE, Severity::Error, Enforcement::Parse),
];

/// Dynamic code families keyed by a leading prefix (the `format!("{prefix}{…}")`
/// codes). Each covers arbitrarily many concrete codes minted at runtime.
pub const FAMILY_PREFIXES: &[(&str, Severity, Enforcement)] = &[
    (codes::SHACL_FAMILY, Severity::Error, Enforcement::Shacl),
    (
        codes::SIGNATURE_FAMILY,
        Severity::Error,
        Enforcement::Signature,
    ),
    (codes::GTS_FAMILY, Severity::Warning, Enforcement::Signature),
    (
        codes::VALIDATE_DEEP_FAMILY,
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        codes::CONSTITUTION_FAMILY,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_FAMILY,
        Severity::Error,
        Enforcement::Governance,
    ),
    (codes::ADVICE_FAMILY, Severity::Note, Enforcement::Advisory),
];

/// Dynamic code families keyed by a trailing suffix — the per-DSL SHACL failure
/// `format!("{label}-dsl.nonconforming")`.
pub const FAMILY_SUFFIXES: &[(&str, Severity, Enforcement)] = &[(
    codes::DSL_NONCONFORMING_SUFFIX,
    Severity::Error,
    Enforcement::Shacl,
)];

/// The stable anchor transform: `/` and `.` become `-`, everything else is kept.
/// The *single* implementation shared by the validator (a finding's help URI) and
/// the docs renderer (a catalog entry's `#anchor`), so the two never diverge.
///
/// `discipline/relator-mediation` → `discipline-relator-mediation`;
/// `validate.deep.skipped` → `validate-deep-skipped`.
pub fn slugify(code: &str) -> String {
    code.chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
}

/// The full catalog help URI for a code: the catalog page anchored by the slug.
pub fn help_uri_for(code: &str) -> String {
    format!("{CATALOG_BASE_URI}#{}", slugify(code))
}

/// The registry-authored rule-level remediation prose — the standing "how to fix a
/// violation of this rule" guidance, keyed by a static [`codes`] const or a dynamic
/// family base. It is the SINGLE source of the fix guidance rendered on both the
/// rule registry (`gmeow:ruleRemediation`, via [`rule_for`]) and, through the
/// pipeline's annotate-by-fingerprint pass, on each finding's SARIF `fixes`.
///
/// A static code carries EXACTLY its own entry (or an honest absence — see
/// [`REMEDIATION_ABSENT`]); a dynamic family member (e.g.
/// `shacl.MinCountConstraintComponent`) inherits its family base's entry.
pub const REMEDIATIONS: &[(&str, &str)] = &[
    // ── Modelling disciplines ──
    (
        codes::DISCIPLINE_STEREOTYPE,
        "Assign the class exactly one OntoUML/gUFO stereotype consistent with its identity supplier; a sortal must specialize a single ultimate kind.",
    ),
    (
        codes::DISCIPLINE_IDENTITY_OVERLAP,
        "Give the entity identity from exactly one ultimate sortal (kind): remove the conflicting identity supplier, or split the class if it genuinely mixes two identities.",
    ),
    (
        codes::DISCIPLINE_ANTI_RIGIDITY,
        "Do not let a rigid type specialize an anti-rigid one; re-classify the phase/role as anti-rigid, or make the supertype non-rigid.",
    ),
    (
        codes::DISCIPLINE_RELATOR_MEDIATION,
        "Have the relator mediate every relatum through gmeow:mediates so each participating endpoint is reachable from the relator.",
    ),
    (
        codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
        "Keep co-equal facets on orthogonal axes; move the overlapping distinction onto a separate facet so the two partitions do not intersect.",
    ),
    (
        codes::DISCIPLINE_FRAME_COMPLETENESS,
        "Declare every role the frame requires; add the missing frame-participant declarations so the frame is complete.",
    ),
    // ── SHACL data-shape ──
    (
        codes::SHACL_NONCONFORMING,
        "SHACL reported non-conformance with no per-result detail; re-run validation to surface the offending focus nodes, or repair the shape that produced an empty result set.",
    ),
    (
        codes::SHACL_FAMILY,
        "Repair the data so it satisfies the violated SHACL constraint shape; the finding's focus node and result path name the offending value.",
    ),
    // ── Bundle trust / signature ──
    (
        codes::SIGNATURE_VERIFY,
        "Re-sign the bundle with a trusted key or supply the correct verification key; the top-level signature check failed.",
    ),
    (
        codes::SIGNATURE_INVALID,
        "Provide a well-formed signing/transport key; the supplied key could not be loaded.",
    ),
    (
        codes::SIGNATURE_MISSING,
        "Sign the bundle before shipping it; no signed frames were found.",
    ),
    (
        codes::SIGNATURE_UNVERIFIED,
        "Supply the signer's public key so the signature can be resolved and checked.",
    ),
    (
        codes::SIGNATURE_UNTRUSTED,
        "Add the signer to the trusted-key set, or re-sign the bundle with a trusted key.",
    ),
    (
        codes::SIGNATURE_FAMILY,
        "Address the bundle-signature profile failure: re-sign with a trusted key or supply the correct verification key.",
    ),
    (
        codes::GTS_FAMILY,
        "Resolve the GTS reader diagnostic named in the message; re-mint the bundle if it is malformed.",
    ),
    // ── Deep-reason (`--deep`) semantic outcomes ──
    (
        codes::VALIDATE_DEEP_SKIPPED,
        "Provide a reasoned bundle so the deep (--deep) pass can run; it was requested without one.",
    ),
    (
        codes::VALIDATE_DEEP_PERMITTED_CONFLICT,
        "Review the disclosed within-world conflict and confirm the glut-admitting reasoning contract is intended; tighten the contract if the conflict should be forbidden.",
    ),
    (
        codes::VALIDATE_DEEP_INCONSISTENT,
        "Resolve the forbidden contradiction: inspect the clash witness and remove or reconcile one of the conflicting assertions.",
    ),
    (
        codes::VALIDATE_DEEP_UNSATISFIABLE,
        "The class is provably empty; relax the over-constraining axioms, or remove the class if its emptiness is unintended.",
    ),
    (
        codes::VALIDATE_DEEP_UNSUPPORTED_CONSTRUCT,
        "The DL construct is outside the decided profile; simplify the axiom into a supported construct, or accept the undecided verdict.",
    ),
    (
        codes::VALIDATE_DEEP_PROJECTION_LOSS,
        "A lowering could not carry the construct exactly; consult the loss ledger and prefer a construct that projects losslessly if exactness is required.",
    ),
    (
        codes::VALIDATE_DEEP_INCOMPLETE,
        "The check ran out of budget; raise the reasoning budget or reduce the input so the verdict completes.",
    ),
    (
        codes::VALIDATE_DEEP_CONTRACT_INVALID,
        "Fix the declared reasoning-contract policy; it was garbled and could not be parsed.",
    ),
    (
        codes::VALIDATE_DEEP_FAMILY,
        "Review the deep-reasoning outcome and act on the specific verdict (inconsistency, unsatisfiability, or incompleteness) it reports.",
    ),
    // ── Dev-governance / repo-structural ──
    (
        codes::CONSTITUTION_HONOR_SYSTEM,
        "This principle is enforced by review practice, not a gate; confirm the change respects it during review.",
    ),
    (
        codes::CONSTITUTION_ORPHANED_ENFORCEMENT,
        "Link the enforcement to the Constitution principle it enforces via logic:formalizes, or remove the orphaned enforcement.",
    ),
    (
        codes::CONSTITUTION_FAMILY,
        "Reconcile the enforcement with the Constitution principle it maps to (declare it, or link it via logic:formalizes).",
    ),
    (
        codes::SLICE_OWNERSHIP_UNOWNED,
        "Declare an owner for the slice in the ownership table.",
    ),
    (
        codes::SLICE_OWNERSHIP_CONFLICT,
        "Resolve the conflicting ownership claims so exactly one owner is declared for the slice.",
    ),
    (
        codes::SLICE_OWNERSHIP_MISMATCH,
        "Reconcile the ownership table with the slice's declared owner so the two agree.",
    ),
    (
        codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY,
        "Declare the used dependency in the slice's dependency list.",
    ),
    (
        codes::SLICE_OWNERSHIP_STALE_DEPENDENCY,
        "Remove the declared dependency that is no longer referenced, or start using it.",
    ),
    (
        codes::SLICE_OWNERSHIP_UNPARSEABLE_QUERY,
        "Fix the malformed slice query so it parses.",
    ),
    (
        codes::SLICE_OWNERSHIP_PEERED_UNREGISTERED_SEAM,
        "Register the crossing term(s) on a gmeow:Seam covering this direction between the co-foundational peers, or replace the peerage-riding reference with an ordinary declared gmeow:sliceDependsOn edge.",
    ),
    (
        codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY,
        "Remove the tier-forbidden reference: a core slice must not depend on an extension, and an extension must not depend on another extension.",
    ),
    (
        codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY,
        "Reverse the direction: move the grounding concept into the grounding slice so it owns it, and have the non-grounding slice consume the grounding term — then drop the grounding slice's gmeow:sliceDependsOn declaration on it. A grounding slice is foundational by construction and must not depend on any consumer.",
    ),
    (
        codes::SLICE_OWNERSHIP_FAMILY,
        "Fix the slice-ownership table entry the finding names (owner, dependency, or query).",
    ),
    // ── Ontology-surface authoring gates ──
    (
        codes::AUTHORING_SHAPE_IRI_COLLISION,
        "Give the sh:NodeShape a single owning shape file; rename or remove the duplicate declaration so the merged shape graph has exactly one definition per IRI.",
    ),
    (
        codes::AUTHORING_CATALOG_MISSING_MODULE,
        "Regenerate the XML catalog (make sync) so every slice module owl:Ontology IRI is mapped.",
    ),
    (
        codes::AUTHORING_MODULE_IRI_MISMATCH,
        "Set the module's owl:Ontology IRI to the location-derived IRI (…/gmeow/slices/<slice-dir-name>).",
    ),
    (
        codes::AUTHORING_PROFILE_CLOSURE,
        "Reconcile the profile's owl:imports with the slice-tier partition: full imports the root plus every extension, claims is a strict subset of core, and every slice is exactly one of core/extension/profile.",
    ),
    (
        codes::AUTHORING_GRAFT_LEAK,
        "Move the norms-extension reference out of the core rights module; the graft is asserted on the extension side only.",
    ),
    (
        codes::AUTHORING_UNDECLARED_TERM,
        "Declare the GMEOW term in the ontology or a slice module, or fix the misspelled predicate/class the fixture or example references.",
    ),
    (
        codes::AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL,
        "Add a language tag to the localizable literal (@x-gmeow-english for authored source) so it is a distinct, translatable term.",
    ),
    (
        codes::AUTHORING_SEAM_REGISTRY_DRIFT,
        "Regenerate the docs projection (make sync SYNC_OUTPUTS=docs) so the seam-registry page reflects the current gmeow:Seam data, or fix the gmeow:Seam individual the finding names in the grounding slice's manifest.ttl.",
    ),
    (
        codes::AUTHORING_FAMILY,
        "Fix the ontology-surface authoring defect the finding names (shape ownership, profile/catalog closure, module IRI, graft isolation, term declaration, language tag, or seam-registry drift).",
    ),
    (
        codes::SLICE_DISCIPLINE_DUPLICATE_IRI,
        "Give each slice manifest a unique slice IRI; identity is manifest-only, so two manifests may not declare the same IRI.",
    ),
    (
        codes::SLICE_DISCIPLINE_MISSING_TIER,
        "Declare exactly one gmeow:sliceTier (tierCore / tierExtension / tierProfile) on the slice manifest.",
    ),
    (
        codes::SLICE_DISCIPLINE_NON_GROUNDING_PEERAGE,
        "Remove the gmeow:sliceCoFoundationalWith declaration from the non-grounding slice, or type the slice gmeow:GroundingSlice if it genuinely is one of the three co-foundational grounding layers.",
    ),
    (
        codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE,
        "Declare gmeow:sliceCoFoundationalWith back from the peer slice; the relation is symmetric and must be authored on both manifests.",
    ),
    (
        codes::SLICE_DISCIPLINE_GROUNDING_MARKER_DRIFT,
        "Reconcile the slice's gmeow:GroundingSlice typing with its location: move the slice under slices/grounding/ or drop the typing, whichever matches its real role.",
    ),
    (
        codes::SLICE_DISCIPLINE_FAMILY,
        "Fix the slice manifest the finding names (unique slice IRI and a mandatory gmeow:sliceTier).",
    ),
    (
        codes::CRATE_LAYERING_VIOLATION,
        "Remove the cross-layer dependency; route through the allowed layer boundary instead.",
    ),
    (
        codes::CRATE_LAYERING_OBSERVATION,
        "Review the layering observation and tighten the crate boundary if the dependency is unintended.",
    ),
    (
        codes::REPO_STATIC_VIOLATION,
        "Fix the repo-structural violation the static check flagged (the message names the offending path).",
    ),
    (
        codes::REPO_STATIC_OBSERVATION,
        "Review the structural observation and adjust the repo layout if it is unintended.",
    ),
    (
        codes::COVERAGE_GAP_CLASS,
        "Add an alignment (e.g. skos:exactMatch / skos:closeMatch) for the external class so it is not used unaligned.",
    ),
    (
        codes::COVERAGE_GAP_PREDICATE,
        "Add an alignment for the external predicate so it is not used unaligned.",
    ),
    (
        codes::BOX_ROLES_MISSING,
        "Add the missing box-role coverage the audit requires.",
    ),
    (
        codes::BOX_ROLES_INVALID,
        "Correct the invalid box-role assignment so it matches the audit's expectations.",
    ),
    (
        codes::WIKIDATA_QID_SYNTAX,
        "Use a well-formed Wikidata QID (Q followed by digits).",
    ),
    (
        codes::WIKIDATA_NAMESPACE_MISUSE,
        "Use the correct Wikidata namespace for the entity (wd: for items, wdt:/p: for properties).",
    ),
    (
        codes::STATEMENT_INVARIANT,
        "Repair the statement-metadata invariant: ground the base triple, use DL-safe datatypes, and keep annotation soundness / preferred-rank consistent.",
    ),
    (
        codes::STATEMENT_COMPILE_LOSSLESS_ROUND_TRIP,
        "Adjust the RDF-1.2 statement so the OWL round-trip is lossless, or record the loss in the projection-loss ledger.",
    ),
    // ── Bundle ontology completeness (`gmeow verify`) ──
    (
        codes::ONTOLOGY_MISSING_LABEL,
        "Add an rdfs:label to the term so it names itself in the documented vocabulary.",
    ),
    (
        codes::ONTOLOGY_MISSING_DEFINITION,
        "Add a skos:definition to the term so its meaning is documented in the vocabulary.",
    ),
    // ── Input well-formedness ──
    (
        codes::EXAMPLE_PARSE,
        "Fix the syntax of the example file so it parses.",
    ),
    // ── Dynamic per-DSL SHACL failure suffix ──
    (
        codes::DSL_NONCONFORMING_SUFFIX,
        "Repair the DSL input so it satisfies its SHACL shape; the per-DSL result names the offending node.",
    ),
];

/// The honest-absence allowlist: codes with genuinely NO rule-level fix — purely
/// informational or positive-verdict outcomes — for which [`remediation_for`]
/// returns `None` by design. The coverage gate
/// (`tests::every_rule_code_has_remediation_or_is_allowlisted`) permits exactly
/// these to lack a remediation, so authoring one example cannot satisfy it.
pub const REMEDIATION_ABSENT: &[&str] = &[
    // Info: reports the resolved signing key — nothing to fix.
    codes::SIGNATURE_KEY,
    // Note: a consistent, fully-covered verdict — nothing to fix.
    codes::VALIDATE_DEEP_CONSISTENT,
    // Note: the Tier-2 deep pass was unavailable (graceful degradation) — no
    // rule-level fix; the outcome is a disclosure, not a violation.
    codes::VALIDATE_DEEP_UNAVAILABLE,
    // The advice.* family carries its OWN per-occurrence guidance (the advisory
    // demonstrator supplies its help URI), so there is no rule-level remediation.
    codes::ADVICE_FAMILY,
];

/// The rule-level remediation prose for a code, if the catalogue authors one.
///
/// A statically-declared code (in [`codes::ALL_CODES`]) carries EXACTLY its own
/// [`REMEDIATIONS`] entry, or an honest `None` — it never inherits a family's
/// generic prose. A dynamic family member (a code not itself declared, e.g.
/// `shacl.MinCountConstraintComponent`) inherits its family base's entry, with the
/// same static-over-prefix-over-suffix precedence as [`classify`].
pub fn remediation_for(code: &str) -> Option<&'static str> {
    if let Some((_, prose)) = REMEDIATIONS.iter().find(|(c, _)| *c == code) {
        return Some(prose);
    }
    // A known static code has no family fallback: it is exactly its authored entry
    // or an honest absence.
    if codes::ALL_CODES.contains(&code) {
        return None;
    }
    // Resolve to the FIRST matching family, mirroring `classify` /
    // `catalog_anchor_uri` precedence exactly (prefix families win over suffix
    // families, first match wins). Once a family matches it is authoritative:
    // return that family's remediation, or `None` if the family has none (an
    // honest absence, e.g. the `advice.` family). We must NOT continue scanning to
    // a later family once the first has matched — doing so would let an
    // allowlisted advisory code like `advice.foo-dsl.nonconforming` skip its own
    // (remediation-less) `advice.` family and pick up unrelated SHACL fix text
    // from the `-dsl.nonconforming` suffix family.
    if let Some((prefix, _, _)) = FAMILY_PREFIXES.iter().find(|(p, _, _)| code.starts_with(p)) {
        return REMEDIATIONS
            .iter()
            .find(|(c, _)| c == prefix)
            .map(|(_, prose)| *prose);
    }
    if let Some((suffix, _, _)) = FAMILY_SUFFIXES.iter().find(|(s, _, _)| code.ends_with(s)) {
        return REMEDIATIONS
            .iter()
            .find(|(c, _)| c == suffix)
            .map(|(_, prose)| *prose);
    }
    None
}

/// The enforcement kind + default grade for a code, if the registry knows it.
/// Static rows win over families; families match by prefix then suffix.
pub fn classify(code: &str) -> Option<(Severity, Enforcement)> {
    if let Some((_, sev, enf)) = STATIC_RULES.iter().find(|(c, _, _)| *c == code) {
        return Some((*sev, *enf));
    }
    if let Some((_, sev, enf)) = FAMILY_PREFIXES.iter().find(|(p, _, _)| code.starts_with(p)) {
        return Some((*sev, *enf));
    }
    if let Some((_, sev, enf)) = FAMILY_SUFFIXES.iter().find(|(s, _, _)| code.ends_with(s)) {
        return Some((*sev, *enf));
    }
    None
}

/// Whether the registry recognises a code (statically or via a family).
pub fn is_known(code: &str) -> bool {
    classify(code).is_some()
}

/// The catalog entry `help_uri` that actually documents `code`.
///
/// A static code has its own catalog row and anchors to its own slug. A
/// dynamic (family) code — e.g. `shacl.MinCountConstraintComponent` — has no
/// row of its own: the catalog only enumerates one *representative* row per
/// family (`shacl.`, `gts.`, `advice.`, `-dsl.nonconforming`, …), so the
/// concrete member's help URI must resolve to that representative's anchor,
/// not to a slug of the full concrete code (which the catalog page has no
/// entry for — a broken deep link). Precedence mirrors [`classify`] exactly
/// (static wins over prefix families, which win over suffix families) so
/// classification and anchor resolution can never disagree.
pub fn catalog_anchor_uri(code: &str) -> String {
    if STATIC_RULES.iter().any(|(c, _, _)| *c == code) {
        return help_uri_for(code);
    }
    if let Some((prefix, _, _)) = FAMILY_PREFIXES.iter().find(|(p, _, _)| code.starts_with(p)) {
        return help_uri_for(prefix);
    }
    if let Some((suffix, _, _)) = FAMILY_SUFFIXES.iter().find(|(s, _, _)| code.ends_with(s)) {
        return help_uri_for(suffix);
    }
    // Unknown to the registry: cannot happen for an emitted code (GAP 1 makes
    // the code set total, checked by `every_declared_code_is_classified`).
    // Fall back to the code's own slug rather than panicking, since
    // `help_uri_for`/`rule_for` are infallible by design.
    help_uri_for(code)
}

/// Build the [`Rule`] for a finding code: its id, the grade the emitted finding
/// carries, and the shared catalog `help_uri`. The rich `title`/`description` are
/// left `None` here — they are enriched from the generated catalog graph and,
/// authoritatively, rendered on the catalog page the `help_uri` points at.
pub fn rule_for(code: &str, default_severity: Severity) -> Rule {
    let mut rule = Rule::new(code, default_severity);
    rule.help_uri = Some(catalog_anchor_uri(code));
    // Thread the registry-authored rule-level remediation onto the built Rule so the
    // renderers surface `gmeow:ruleRemediation`. Honest absence for allowlisted codes.
    if let Some(remediation) = remediation_for(code) {
        rule = rule.with_remediation(remediation);
    }
    rule
}

/// Populate `report.rules` so every distinct finding code carries a rule entry
/// whose `helpUri` resolves to its constraint-catalog page anchor (the AC:
/// "validator finding codes resolve to catalog entries"). Idempotent: codes that
/// already carry a rule (e.g. the advisory demonstrator, which supplies its own
/// help URI) are left untouched, and each code is added at most once.
pub fn populate_rules(report: &mut Report) {
    let existing: BTreeSet<String> = report.rules.iter().map(|r| r.id.clone()).collect();
    let mut added: BTreeSet<String> = BTreeSet::new();
    // Deterministic order: findings are already in a stable order, and we add the
    // first-seen severity for each code.
    let mut to_add: Vec<(String, Severity)> = Vec::new();
    for finding in &report.findings {
        if existing.contains(&finding.code) || added.contains(&finding.code) {
            continue;
        }
        added.insert(finding.code.clone());
        to_add.push((finding.code.clone(), finding.severity));
    }
    for (code, severity) in to_add {
        report.add_rule(rule_for(&code, severity));
    }
}

/// Every rule the catalog can enumerate — the static rows plus one representative
/// per dynamic family (marked `family: true`). The constraint-catalog generator
/// projects one `gmeow:ValidationRule` per seed, resolving its description,
/// category, and governed terms from the reasoned graph.
pub fn all_rules() -> Vec<RuleSeed> {
    let mut seeds: Vec<RuleSeed> = STATIC_RULES
        .iter()
        .map(|(code, sev, enf)| RuleSeed {
            code,
            default_severity: *sev,
            enforcement: *enf,
            family: false,
            remediation: remediation_for(code),
        })
        .collect();
    for (prefix, sev, enf) in FAMILY_PREFIXES {
        // `validate.deep.` overlaps the static `validate.deep.*` rows and
        // `signature.`/`slice-ownership.` overlap their static rows: those static
        // rows already enumerate the known members, so skip a redundant family
        // representative when every emitted member is expected to be static.
        seeds.push(RuleSeed {
            code: prefix,
            default_severity: *sev,
            enforcement: *enf,
            family: true,
            remediation: remediation_for(prefix),
        });
    }
    for (suffix, sev, enf) in FAMILY_SUFFIXES {
        seeds.push(RuleSeed {
            code: suffix,
            default_severity: *sev,
            enforcement: *enf,
            family: true,
            remediation: remediation_for(suffix),
        });
    }
    seeds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_replaces_path_and_dot_separators() {
        assert_eq!(
            slugify("discipline/relator-mediation"),
            "discipline-relator-mediation"
        );
        assert_eq!(slugify("validate.deep.skipped"), "validate-deep-skipped");
        assert_eq!(slugify("shacl.nonconforming"), "shacl-nonconforming");
        assert_eq!(
            slugify("statement-compile.lossless-round-trip"),
            "statement-compile-lossless-round-trip"
        );
    }

    #[test]
    fn help_uri_is_the_catalog_anchor() {
        assert_eq!(
            help_uri_for("discipline/relator-mediation"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#discipline-relator-mediation"
        );
    }

    #[test]
    fn populate_rules_resolves_every_code_and_is_idempotent() {
        use gmeow_errors::{Finding, Report};
        let mut report = Report::new("validate");
        report.add_finding(Finding::new(
            Severity::Error,
            "discipline/relator-mediation",
            "m",
        ));
        report.add_finding(Finding::new(
            Severity::Warning,
            "shacl.MinCountConstraintComponent",
            "m",
        ));
        // A code already carrying a rule (advisory-style) must not be duplicated.
        let mut advisory = Rule::new("advice.sample", Severity::Note);
        advisory.help_uri = Some("https://blackcatinformatics.ca/gmeow/advice#sample".to_owned());
        report.add_rule(advisory);
        report.add_finding(Finding::new(Severity::Note, "advice.sample", "m"));

        populate_rules(&mut report);
        let first_len = report.rules.len();
        populate_rules(&mut report); // idempotent
        assert_eq!(
            report.rules.len(),
            first_len,
            "populate_rules must be idempotent"
        );

        // Every emitted code now resolves to exactly one rule with a catalog helpUri.
        for code in [
            "discipline/relator-mediation",
            "shacl.MinCountConstraintComponent",
            "advice.sample",
        ] {
            let matches: Vec<_> = report.rules.iter().filter(|r| r.id == code).collect();
            assert_eq!(matches.len(), 1, "exactly one rule per code {code}");
        }
        let mediation = report
            .rules
            .iter()
            .find(|r| r.id == "discipline/relator-mediation")
            .unwrap();
        assert_eq!(
            mediation.help_uri.as_deref(),
            Some(
                "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#discipline-relator-mediation"
            )
        );
        // A dynamic family member's helpUri must point at the FAMILY entry's
        // anchor (the catalog page has no row for the full concrete code), not
        // a slug of the concrete code itself.
        let shacl_member = report
            .rules
            .iter()
            .find(|r| r.id == "shacl.MinCountConstraintComponent")
            .unwrap();
        assert_eq!(
            shacl_member.help_uri.as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#shacl-")
        );
        // The pre-existing advisory rule's own help URI is preserved, not clobbered.
        let advice = report
            .rules
            .iter()
            .find(|r| r.id == "advice.sample")
            .unwrap();
        assert_eq!(
            advice.help_uri.as_deref(),
            Some("https://blackcatinformatics.ca/gmeow/advice#sample")
        );
    }

    #[test]
    fn catalog_anchor_uri_resolves_dynamic_family_members_to_the_family_entry() {
        assert_eq!(
            catalog_anchor_uri("shacl.MinCountConstraintComponent"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#shacl-"
        );
        assert_eq!(
            catalog_anchor_uri("gts.something"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#gts-"
        );
        assert_eq!(
            catalog_anchor_uri("advice.foo"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#advice-"
        );
        assert_eq!(
            catalog_anchor_uri("mylabel-dsl.nonconforming"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#-dsl-nonconforming"
        );
    }

    #[test]
    fn catalog_anchor_uri_resolves_static_codes_to_their_own_anchor() {
        assert_eq!(
            catalog_anchor_uri("discipline/relator-mediation"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#discipline-relator-mediation"
        );
        // `signature.verify` matches both the static row and the `signature.`
        // family prefix; the static row must win, same precedence as `classify`.
        assert_eq!(
            catalog_anchor_uri("signature.verify"),
            "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#signature-verify"
        );
    }

    /// D2a coverage gate: EVERY enumerated rule code (static rows + family reps)
    /// must carry EITHER an authored rule-level remediation OR appear on the honest
    /// absence allowlist — never both, never neither. Because it enumerates the whole
    /// `all_rules()` set (not one example), authoring a single remediation cannot
    /// satisfy it: a code with neither fails right here.
    #[test]
    fn every_rule_code_has_remediation_or_is_allowlisted() {
        for seed in all_rules() {
            let has = remediation_for(seed.code).is_some();
            let absent = REMEDIATION_ABSENT.contains(&seed.code);
            assert!(
                has != absent,
                "code `{}` must have EITHER an authored rule-level remediation OR appear on \
                 the honest-absence allowlist (REMEDIATION_ABSENT), never both nor neither",
                seed.code
            );
            // The seed's projected field mirrors the lookup exactly.
            assert_eq!(seed.remediation, remediation_for(seed.code));
        }
        // The allowlist must be honest: every entry genuinely resolves to no remediation.
        for code in REMEDIATION_ABSENT {
            assert!(
                remediation_for(code).is_none(),
                "allowlisted code `{code}` must genuinely have NO remediation"
            );
        }
    }

    /// A dynamic family member (never a declared static code) inherits its family
    /// base's remediation, so a real `shacl.*` / `-dsl.nonconforming` finding gets fix
    /// guidance through the same lookup the annotate pass uses.
    #[test]
    fn dynamic_family_members_inherit_the_family_remediation() {
        assert_eq!(
            remediation_for("shacl.MinCountConstraintComponent"),
            remediation_for(codes::SHACL_FAMILY),
        );
        assert_eq!(
            remediation_for("mylabel-dsl.nonconforming"),
            remediation_for(codes::DSL_NONCONFORMING_SUFFIX),
        );
        // A rule built for a dynamic member carries the inherited remediation.
        let rule = rule_for("shacl.MinCountConstraintComponent", Severity::Error);
        assert_eq!(
            rule.remediation.as_deref(),
            remediation_for(codes::SHACL_FAMILY)
        );
    }

    #[test]
    fn static_rules_are_unique_and_slug_distinct() {
        let mut seen = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        for (code, _, _) in STATIC_RULES {
            assert!(seen.insert(*code), "duplicate static code {code}");
            assert!(slugs.insert(slugify(code)), "slug collision for {code}");
            assert!(is_known(code), "static code {code} not classified");
        }
    }

    /// The compile-time totality gate: every code const declared in
    /// [`codes::ALL_CODES`] must be classified by [`is_known`], must appear as a
    /// `STATIC_RULES` row, and must be unique. This replaces the previous
    /// source-scanning heuristic — totality now holds *by construction*: a new
    /// emit site can only reference a `codes::` const (there is no other way to
    /// mint a code, since every wrapper/helper in this crate takes the code as an
    /// argument sourced from `codes`), and a const added to [`codes::ALL_CODES`]
    /// without a matching `STATIC_RULES` row fails right here, at build time, not
    /// via a grep over the source.
    #[test]
    fn every_declared_code_is_classified() {
        let mut seen = BTreeSet::new();
        for &code in codes::ALL_CODES {
            assert!(
                seen.insert(code),
                "duplicate entry in codes::ALL_CODES: {code}"
            );
            assert!(
                is_known(code),
                "codes::ALL_CODES entry {code} is not classified by STATIC_RULES or a family — \
                 add a STATIC_RULES row (or confirm it is meant to be family-only and drop it \
                 from ALL_CODES)"
            );
        }
    }

    /// `STATIC_RULES` is a subset of `codes::ALL_CODES`: every static row's code
    /// must be a declared const in the enumeration authority, so the registry and
    /// the enumeration can never silently diverge.
    #[test]
    fn static_rules_are_a_subset_of_all_codes() {
        let all: BTreeSet<&str> = codes::ALL_CODES.iter().copied().collect();
        for (code, _, _) in STATIC_RULES {
            assert!(
                all.contains(code),
                "STATIC_RULES code {code} is missing from codes::ALL_CODES"
            );
        }
    }

    /// Every family prefix/suffix used by `FAMILY_PREFIXES` / `FAMILY_SUFFIXES`
    /// must be declared in [`codes::ALL_FAMILY_PREFIXES`] /
    /// [`codes::ALL_FAMILY_SUFFIXES`], so a family base can only ever originate
    /// from the `codes` authority.
    #[test]
    fn family_prefixes_and_suffixes_are_declared_in_codes() {
        let declared_prefixes: BTreeSet<&str> =
            codes::ALL_FAMILY_PREFIXES.iter().copied().collect();
        for (prefix, _, _) in FAMILY_PREFIXES {
            assert!(
                declared_prefixes.contains(prefix),
                "family prefix {prefix} is missing from codes::ALL_FAMILY_PREFIXES"
            );
        }
        let declared_suffixes: BTreeSet<&str> =
            codes::ALL_FAMILY_SUFFIXES.iter().copied().collect();
        for (suffix, _, _) in FAMILY_SUFFIXES {
            assert!(
                declared_suffixes.contains(suffix),
                "family suffix {suffix} is missing from codes::ALL_FAMILY_SUFFIXES"
            );
        }
    }
}
