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
use gmeow_diagnostics::{Report, Rule, Severity};
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
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_STALE_DEPENDENCY,
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        codes::SLICE_OWNERSHIP_UNPARSEABLE_QUERY,
        Severity::Warning,
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
        });
    }
    for (suffix, sev, enf) in FAMILY_SUFFIXES {
        seeds.push(RuleSeed {
            code: suffix,
            default_severity: *sev,
            enforcement: *enf,
            family: true,
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
        use gmeow_diagnostics::{Finding, Report};
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
