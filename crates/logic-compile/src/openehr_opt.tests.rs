// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn missing_node_id_hard_fails() {
    let err = read_magnitude_interval("<template></template>", "at9999").unwrap_err();
    assert!(err.to_string().contains("at9999"));
}

#[test]
fn code_phrase_without_terminology_hard_fails() {
    // A <code_list> with no <terminology_id> is malformed — hard-fail rather than mint a
    // bogus `unknown` terminology.
    let xml = "<c xsi:type=\"C_CODE_PHRASE\" \
                   xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
                   <code_list>at0004</code_list></c>";
    let naming = std::collections::BTreeMap::new();
    let err = read_all_opt_constraints(xml, "https://ex/", &naming).unwrap_err();
    assert!(err.to_string().contains("terminology_id"), "got: {err}");
}

#[test]
fn existence_on_a_single_attribute_yields_cardinality() {
    // A `<existence>` interval on a C_SINGLE_ATTRIBUTE node is read exactly like an
    // `<occurrences>` interval — a Cardinality on a distinct `existence/` path.
    let xml = "<c xsi:type=\"C_SINGLE_ATTRIBUTE\" \
                   xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
                   <existence>\
                   <lower_included>true</lower_included>\
                   <upper_included>true</upper_included>\
                   <lower_unbounded>false</lower_unbounded>\
                   <upper_unbounded>false</upper_unbounded>\
                   <lower>1</lower><upper>1</upper>\
                   </existence></c>";
    let naming = std::collections::BTreeMap::new();
    let all = read_all_opt_constraints(xml, "https://ex/", &naming).unwrap();
    let card = all
        .iter()
        .find_map(|c| match &c.kind {
            OptConstraintKind::Cardinality { path, min, max } if path.contains("existence/") => {
                Some((*min, *max))
            }
            _ => None,
        })
        .expect("an existence cardinality on an existence/ path");
    assert_eq!(card, (Some(1), Some(1)));
}

#[test]
fn bounded_interval_with_missing_bound_element_hard_fails() {
    // <lower_unbounded>false</lower_unbounded> asserts the lower end IS bounded; a missing
    // <lower> must hard-fail, never be silently widened to open (which would accept magnitudes
    // the constraint rejects).
    let xml = "<c xsi:type=\"C_DV_QUANTITY\" \
                   xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
                   <list><magnitude>\
                   <lower_included>true</lower_included>\
                   <upper_included>true</upper_included>\
                   <lower_unbounded>false</lower_unbounded>\
                   <upper_unbounded>true</upper_unbounded>\
                   </magnitude><units>mmHg</units></list></c>";
    let naming = std::collections::BTreeMap::new();
    let err = read_all_opt_constraints(xml, "https://ex/", &naming).unwrap_err();
    assert!(err.to_string().contains("<lower> is missing"), "got: {err}");
}

#[test]
fn term_bindings_with_codes_but_no_terminology_hard_fails() {
    // A <term_bindings> that binds codes but carries no `terminology` id is malformed —
    // hard-fail rather than mint a bogus `unknown` terminology.
    let xml = "<term_bindings>\
                   <items><value><code_string>at0004</code_string></value></items>\
                   </term_bindings>";
    let naming = std::collections::BTreeMap::new();
    let err = read_all_opt_constraints(xml, "https://ex/", &naming).unwrap_err();
    assert!(err.to_string().contains("terminology"), "got: {err}");
}
