//! The validator's rule-identity registry — the single authority for *what*
//! finding codes the validator can emit, and the seam by which every finding
//! code resolves to a constraint-catalog entry (the "what GMEOW enforces"
//! surface).
//!
//! # The irreducible line
//!
//! A finding code exists *only because* a Rust check mints it (e.g.
//! `Finding::new(Severity::Error, "discipline/relator-mediation", …)`). The set
//! of codes, each code's default grade, and the *kind* of thing it enforces are
//! therefore intrinsic Rust facts and live here. Everything human-readable — the
//! per-term description and the category — is **generated** from the reasoned
//! graph by the constraint-catalog pipeline stage, never authored here, so the
//! catalog stays a projection of the axioms rather than a hand-maintained list.
//!
//! This module owns exactly four things:
//!
//! * [`slugify`] / [`help_uri_for`] — the *single* anchor transform shared by the
//!   validator (finding `helpUri`) and the docs renderer, so a finding code and
//!   its catalog page anchor can never disagree.
//! * [`Enforcement`] + [`STATIC_RULES`] + the family classifiers — the minimal
//!   `{code → default severity, enforcement kind}` seeds.
//! * [`rule_for`] / [`populate_rules`] — populate a report's `rules` so every
//!   emitted code carries a rule entry whose `helpUri` resolves to the catalog.
//! * [`all_rules`] — the enumeration the catalog generator projects from.

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
/// The coherence test scans the crate source and fails the build if any emitted
/// literal code is neither listed here nor covered by a family classifier — so
/// this table stays total by construction.
pub const STATIC_RULES: &[(&str, Severity, Enforcement)] = &[
    // ── Modelling disciplines (OntoUML / CONSTITUTION) — data- and vocab-facing ──
    (
        "discipline/stereotype",
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        "discipline/identity-overlap",
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        "discipline/anti-rigidity",
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        "discipline/relator-mediation",
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        "discipline/coequal-orthogonality",
        Severity::Error,
        Enforcement::Discipline,
    ),
    (
        "discipline/frame-completeness",
        Severity::Error,
        Enforcement::Discipline,
    ),
    // ── SHACL data-shape (the non-family static outcome) ──
    ("shacl.nonconforming", Severity::Error, Enforcement::Shacl),
    // ── Bundle trust / signature ──
    ("signature.verify", Severity::Error, Enforcement::Signature),
    ("signature.invalid", Severity::Error, Enforcement::Signature),
    ("signature.missing", Severity::Error, Enforcement::Signature),
    (
        "signature.unverified",
        Severity::Error,
        Enforcement::Signature,
    ),
    (
        "signature.untrusted",
        Severity::Error,
        Enforcement::Signature,
    ),
    ("signature.key", Severity::Info, Enforcement::Signature),
    // ── Deep-reason (`--deep`) semantic outcomes ──
    (
        "validate.deep.skipped",
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.permitted-conflict",
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.inconsistent",
        Severity::Error,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.unsatisfiable",
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.unsupported-construct",
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.projection-loss",
        Severity::Note,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.incomplete",
        Severity::Warning,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.consistent",
        Severity::Note,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.contract-invalid",
        Severity::Error,
        Enforcement::DeepReason,
    ),
    (
        "validate.deep.unavailable",
        Severity::Note,
        Enforcement::DeepReason,
    ),
    // ── Dev-governance / repo-structural (developer CLI) ──
    (
        "constitution.honor-system",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "constitution.orphaned-enforcement",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "slice-ownership.unowned",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "slice-ownership.conflict",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "slice-ownership.mismatch",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "slice-ownership.undeclared-dependency",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "slice-ownership.stale-dependency",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "slice-ownership.unparseable-query",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "crate-layering.violation",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "crate-layering.observation",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "repo-static.violation",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "repo-static.observation",
        Severity::Warning,
        Enforcement::Governance,
    ),
    (
        "coverage.gap-class",
        Severity::Info,
        Enforcement::Governance,
    ),
    (
        "coverage.gap-predicate",
        Severity::Info,
        Enforcement::Governance,
    ),
    (
        "box-roles.missing",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "box-roles.invalid",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "wikidata.qid-syntax",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "wikidata.namespace-misuse",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "statement.invariant",
        Severity::Error,
        Enforcement::Governance,
    ),
    (
        "statement-compile.lossless-round-trip",
        Severity::Error,
        Enforcement::Governance,
    ),
    // ── Input well-formedness ──
    ("example.parse", Severity::Error, Enforcement::Parse),
];

/// Dynamic code families keyed by a leading prefix (the `format!("{prefix}{…}")`
/// codes). Each covers arbitrarily many concrete codes minted at runtime.
pub const FAMILY_PREFIXES: &[(&str, Severity, Enforcement)] = &[
    ("shacl.", Severity::Error, Enforcement::Shacl),
    ("signature.", Severity::Error, Enforcement::Signature),
    ("gts.", Severity::Warning, Enforcement::Signature),
    ("validate.deep.", Severity::Warning, Enforcement::DeepReason),
    ("constitution.", Severity::Warning, Enforcement::Governance),
    ("slice-ownership.", Severity::Error, Enforcement::Governance),
    ("advice.", Severity::Note, Enforcement::Advisory),
];

/// Dynamic code families keyed by a trailing suffix — the per-DSL SHACL failure
/// `format!("{label}-dsl.nonconforming")`.
pub const FAMILY_SUFFIXES: &[(&str, Severity, Enforcement)] =
    &[("-dsl.nonconforming", Severity::Error, Enforcement::Shacl)];

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

/// Build the [`Rule`] for a finding code: its id, the grade the emitted finding
/// carries, and the shared catalog `help_uri`. The rich `title`/`description` are
/// left `None` here — they are enriched from the generated catalog graph and,
/// authoritatively, rendered on the catalog page the `help_uri` points at.
pub fn rule_for(code: &str, default_severity: Severity) -> Rule {
    let mut rule = Rule::new(code, default_severity);
    rule.help_uri = Some(help_uri_for(code));
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
    use std::path::Path;

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
            Some("https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#discipline-relator-mediation")
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
    fn static_rules_are_unique_and_slug_distinct() {
        let mut codes = BTreeSet::new();
        let mut slugs = BTreeSet::new();
        for (code, _, _) in STATIC_RULES {
            assert!(codes.insert(*code), "duplicate static code {code}");
            assert!(slugs.insert(slugify(code)), "slug collision for {code}");
            assert!(is_known(code), "static code {code} not classified");
        }
    }

    /// Decide whether a string literal is a finding-code candidate. Real codes
    /// carry a `.` separator or the `discipline/` prefix; file paths, MIME types,
    /// and tool names either end in an extension or use `/` without `discipline/`.
    fn looks_like_code(lit: &str) -> bool {
        if lit.is_empty() || lit.contains(char::is_whitespace) {
            return false;
        }
        // discipline codes are the only `/`-bearing codes.
        if lit.contains('/') {
            return lit.starts_with("discipline/") && !lit.contains(".ttl");
        }
        // otherwise a code must be dot-segmented and not a filename.
        if !lit.contains('.') {
            return false;
        }
        const EXTENSIONS: &[&str] = &[
            ".ttl", ".rs", ".py", ".sh", ".json", ".md", ".nt", ".nq", ".gts", ".toml", ".cff",
            ".po", ".rq", ".yaml", ".yml", ".html", ".css", ".b",
        ];
        if EXTENSIONS.iter().any(|e| lit.ends_with(e)) {
            return false;
        }
        // codes are lowercase alnum with `.`/`-` separators only.
        lit.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    }

    /// Scan the crate's production source (each file truncated at its `mod tests`)
    /// and assert every finding-code literal is recognised by the registry, so a
    /// newly-added `Finding::new(…, "new/code", …)` fails the build until it is
    /// catalogued. Dynamic (`format!`) codes are literal-free here and covered by
    /// the family classifiers instead. This is the coherence gate: no emitted code
    /// escapes the catalog.
    #[test]
    fn every_emitted_code_literal_is_catalogued() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut unknown: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&src).expect("read validate/src") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source file");
            // Drop the trailing `#[cfg(test)]` / `mod tests` region so test-only
            // codes (e.g. "validate.x", "tier1.fixture") do not count.
            let production = match text.find("mod tests") {
                Some(idx) => &text[..idx],
                None => &text[..],
            };
            // Only consider literals that are arguments to `Finding::new(` or a
            // lone code argument on its own line (the multi-line call shape). Both
            // reduce to: string literals appearing in a `Finding::new( … )` span.
            for literal in code_literals_near_finding_new(production) {
                if looks_like_code(&literal) && !is_known(&literal) {
                    unknown.push(format!("{}: {literal}", path.display()));
                }
            }
        }
        assert!(
            unknown.is_empty(),
            "finding codes emitted but absent from the rule catalog (add to STATIC_RULES or a family):\n{}",
            unknown.join("\n")
        );
    }

    /// Extract string literals that appear inside a `Finding::new( … )` call span.
    /// The span runs from `Finding::new(` to its balanced closing paren, capturing
    /// every `"…"` literal within (the code is among them; messages are filtered
    /// out later by [`looks_like_code`]). Also captures lone `"code",` argument
    /// lines that feed a code variable passed to `Finding::new`.
    fn code_literals_near_finding_new(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let bytes = text.as_bytes();
        let needle = "Finding::new(";
        let mut search_from = 0usize;
        while let Some(rel) = text[search_from..].find(needle) {
            let start = search_from + rel + needle.len();
            // Walk to the balanced close paren (or a cap) collecting literals.
            let mut depth = 1i32;
            let mut i = start;
            while i < bytes.len() && depth > 0 && i - start < 400 {
                match bytes[i] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    b'"' => {
                        // read literal
                        let mut j = i + 1;
                        while j < bytes.len() && bytes[j] != b'"' {
                            if bytes[j] == b'\\' {
                                j += 1;
                            }
                            j += 1;
                        }
                        if j < bytes.len() {
                            out.push(text[i + 1..j].to_string());
                            i = j;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            search_from = start;
        }
        // Lone `"code",` / `code = "…"` argument lines (variable-fed codes whose
        // literal is defined near the call, e.g. box-roles / coverage helpers).
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    let lit = &rest[..end];
                    let tail = rest[end + 1..].trim();
                    if tail == "," || tail.is_empty() {
                        out.push(lit.to_string());
                    }
                }
            }
        }
        out
    }
}
