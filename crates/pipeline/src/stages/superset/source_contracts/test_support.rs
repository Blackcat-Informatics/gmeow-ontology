// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers kept outside the selected production source closure.

use super::*;

#[cfg(test)]
impl OwnedRule {
    pub(super) fn hydrate(&self) -> FanoutRule {
        let form = match &self.form {
            OwnedForm::Turtle => GraphForm::Turtle,
            OwnedForm::NTriples => GraphForm::NTriples,
            OwnedForm::NQuads(graph) => {
                assert_eq!(
                    graph,
                    super::GRAPH_DIAGNOSTICS_IRI,
                    "source record names an unsupported native N-Quads dispatch graph"
                );
                GraphForm::NQuads(super::GRAPH_DIAGNOSTICS_IRI)
            }
            OwnedForm::NQuadsSelf => GraphForm::NQuadsSelf,
            OwnedForm::Blob => GraphForm::Blob,
            OwnedForm::HeaderDict => GraphForm::HeaderDict,
        };
        FanoutRule {
            path: self.path.clone(),
            match_prefix: self.match_prefix,
            suffix: self.suffix.clone(),
            family: self.family,
            form,
        }
    }
}

#[cfg(test)]
fn selected() -> &'static Observation {
    use std::sync::OnceLock;
    struct Selected {
        selector: String,
        observation: Observation,
    }
    static OBSERVED: OnceLock<Result<Selected, gmeow_errors::Diag>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("superset source contracts require the exact producer selector");
    let selected = OBSERVED
        .get_or_init(|| {
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &gmeow_conformance::paths::repo_root(),
                "stage-conformance",
                CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let observation = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                observation,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated pipeline source contracts: {error}"));
    assert_eq!(
        selected.selector, selector,
        "source observations cannot cross selector identities"
    );
    assert_eq!(selected.observation.source_path, SOURCE);
    assert_eq!(
        selected.observation.source_digest.len(),
        64,
        "exact original pipeline module identity"
    );
    &selected.observation
}

#[cfg(test)]
pub(crate) fn authored_fanout_rules() -> Vec<FanoutRule> {
    selected()
        .fanout_rules
        .iter()
        .map(OwnedRule::hydrate)
        .collect()
}

#[cfg(test)]
pub(crate) fn authored_fanout_classes() -> super::RdfFanoutClasses {
    super::RdfFanoutClasses::from_rules(authored_fanout_rules())
}

#[cfg(test)]
pub(in crate::stages::superset) fn authored_expected() -> BTreeSet<String> {
    selected().expected_outputs.clone()
}

#[cfg(test)]
pub(in crate::stages::superset) fn declared_value_set(local: &str) -> BTreeSet<String> {
    let declaration = selected()
        .definitions
        .get(local)
        .unwrap_or_else(|| panic!("missing authored definition for gmeow:{local}"));
    assert!(
        declaration.text.contains(VALUE_MARKER),
        "the original definition must explicitly enumerate its values"
    );
    declaration
        .values
        .as_ref()
        .unwrap_or_else(|error| panic!("{error}"))
        .clone()
}
