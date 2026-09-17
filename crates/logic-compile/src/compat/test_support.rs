// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Build the expanded facet contract for each of the six named presets, as the
/// front-end would after expanding `logic:expandsToFacet`.  Used by the tests to
/// assert every preset's contract is supported.  Mirrors the `expandsToFacet`
/// bundles in `slices/grounding/logic/module.ttl`.
pub(super) fn preset_contracts() -> Vec<ReasoningContract> {
    use super::ir::SemanticProfileId;

    // PositiveHorn: HornFragment, LeastModelSemantics, MonotonicRevision,
    // OpenWorldClosure (default), CertifiedFragmentResource.
    let mut positive_horn = ReasoningContract::from_preset(SemanticProfileId::PositiveHorn);
    positive_horn.formula_fragment = Some("HornFragment".to_owned());
    positive_horn.model_semantics = Some("LeastModelSemantics".to_owned());
    positive_horn.revision = Some("MonotonicRevision".to_owned());
    positive_horn.default_closure = Some("OpenWorldClosure".to_owned());
    positive_horn
        .resource_policies
        .insert("CertifiedFragmentResource".to_owned());

    // StratifiedNAF: DefaultNegation, StratifiedSemantics.
    let mut stratified = ReasoningContract::from_preset(SemanticProfileId::StratifiedNaf);
    stratified
        .negation_operators
        .insert("DefaultNegation".to_owned());
    stratified.model_semantics = Some("StratifiedSemantics".to_owned());

    // WellFounded: DefaultNegation, WellFoundedSemantics.
    let mut well_founded = ReasoningContract::from_preset(SemanticProfileId::WellFounded);
    well_founded
        .negation_operators
        .insert("DefaultNegation".to_owned());
    well_founded.model_semantics = Some("WellFoundedSemantics".to_owned());

    // StableModel: DefaultNegation, StableModelSemantics. (No probabilistic
    // measure ⇒ supported.)
    let mut stable = ReasoningContract::from_preset(SemanticProfileId::StableModel);
    stable
        .negation_operators
        .insert("DefaultNegation".to_owned());
    stable.model_semantics = Some("StableModelSemantics".to_owned());

    // ProceduralProlog: HornFragment, BudgetBoundedResource.
    let mut prolog = ReasoningContract::from_preset(SemanticProfileId::ProceduralProlog);
    prolog.formula_fragment = Some("HornFragment".to_owned());
    prolog
        .resource_policies
        .insert("BudgetBoundedResource".to_owned());

    // Probabilistic: ProbabilisticMeasure. (No StableModelSemantics ⇒ supported by
    // the table; the model-declaration requirement is graph-dependent.)
    let mut probabilistic = ReasoningContract::from_preset(SemanticProfileId::Probabilistic);
    probabilistic
        .uncertainty_measures
        .insert("ProbabilisticMeasure".to_owned());

    vec![
        positive_horn,
        stratified,
        well_founded,
        stable,
        prolog,
        probabilistic,
    ]
}

/// The set of ids of every table-driven rule (a subset of [`ALL_RULE_IDS`]; the
/// remaining id is the graph-dependent front-end rule).
pub(super) fn table_rule_ids() -> std::collections::BTreeSet<&'static str> {
    RULES.iter().map(|r| r.id).collect()
}
