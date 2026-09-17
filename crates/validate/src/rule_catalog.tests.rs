// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let declared_prefixes: BTreeSet<&str> = codes::ALL_FAMILY_PREFIXES.iter().copied().collect();
    for (prefix, _, _) in FAMILY_PREFIXES {
        assert!(
            declared_prefixes.contains(prefix),
            "family prefix {prefix} is missing from codes::ALL_FAMILY_PREFIXES"
        );
    }
    let declared_suffixes: BTreeSet<&str> = codes::ALL_FAMILY_SUFFIXES.iter().copied().collect();
    for (suffix, _, _) in FAMILY_SUFFIXES {
        assert!(
            declared_suffixes.contains(suffix),
            "family suffix {suffix} is missing from codes::ALL_FAMILY_SUFFIXES"
        );
    }
}
