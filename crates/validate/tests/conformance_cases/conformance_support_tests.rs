// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Single execution owner for the shared conformance-support self-tests.

use crate::conformance_support;

#[gmeow_test_batch_macros::batch_test]
fn object_as_subject_converts_named_and_blank_only() {
    conformance_support::object_as_subject_converts_named_and_blank_only();
}

#[gmeow_test_batch_macros::batch_test]
fn rdf_list_h_walks_union_of_in_order() {
    conformance_support::rdf_list_h_walks_union_of_in_order();
}

#[gmeow_test_batch_macros::batch_test]
fn rdf_list_h_preserves_order_with_a_blank_member() {
    conformance_support::rdf_list_h_preserves_order_with_a_blank_member();
}

#[gmeow_test_batch_macros::batch_test]
fn subjects_of_type_h_and_restriction_matches_a_blank_restriction() {
    conformance_support::subjects_of_type_h_and_restriction_matches_a_blank_restriction();
}

#[gmeow_test_batch_macros::batch_test]
fn axiom_annotations_found_by_annotated_source() {
    conformance_support::axiom_annotations_found_by_annotated_source();
}

#[gmeow_test_batch_macros::batch_test]
fn sharded_validation_matches_serial_byte_for_byte() {
    conformance_support::sharded_validation_matches_serial_byte_for_byte();
}
