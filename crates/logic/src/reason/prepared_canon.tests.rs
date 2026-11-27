// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The selected payload keeps the denial owner and the disjunctive lowering
/// boundary, and a reader cannot admit a truncated or oversize publication.
#[test]
fn complete_native_publication_keeps_proof_attribution_and_refusals() {
    let source = purrdf::parse_dataset(
        br#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://example.org/> .
ex:ban a logic:ForbiddenPatternConstraint ;
    logic:onClass ex:Guard ; logic:forbiddenPredicate ex:edge ;
    logic:formalizes ex:Guard .
ex:alternatives a logic:Formula ; logic:or ex:first, ex:second .
ex:first a logic:Formula ; logic:relation ex:firstRelation ; logic:argument ex:a, ex:b .
ex:second a logic:Formula ; logic:relation ex:secondRelation ; logic:argument ex:a, ex:b .
ex:a a logic:TermCarrier ; logic:termIndex 0 ; logic:termIri ex:itemA .
ex:b a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:itemB .
"#,
        "text/turtle",
        None,
    )
    .expect("synthetic source parses");
    let theory = gmeow_logic_compile::frontend::PreparedLogicSource::new(&source)
        .expect("synthetic source prepares")
        .into_compiled(None)
        .expect("synthetic source compiles");
    let inventory = SourceInventory::new(&theory, SourceSelection::all_classical_roots(&theory));
    let bytes = PreparedCanonProof::encode(&inventory, 32_768).expect("native artifact encodes");
    let mut decoded =
        PreparedCanonProof::decode(&bytes, bytes.len()).expect("native artifact reads");

    assert_eq!(
        decoded.binding().selection.source_digest,
        inventory.source_digest()
    );
    assert_eq!(
        decoded.binding().selection.program_digest,
        inventory.program_digest()
    );
    assert_eq!(
        decoded.binding().compiler_trace,
        inventory.compiler_trace_digest()
    );
    let owner = "https://example.org/ban";
    assert!(
        decoded
            .record
            .lowering
            .rules
            .iter()
            .any(|rule| decoded.denial_owner(&rule.rule_iri) == Some(owner)),
        "the executable denial retains its original owner"
    );
    assert!(
        decoded.incomplete_formulas().iter().any(|index| decoded
            .record
            .authored_formulas
            .get(index)
            .is_some_and(|names| names.contains("https://example.org/alternatives"))),
        "an unsupported root keeps its exact source name and incomplete lowering"
    );
    assert!(PreparedCanonProof::decode(&bytes, bytes.len() - 1).is_err());
    assert!(PreparedCanonProof::encode(&inventory, bytes.len() - 1).is_err());
    assert!(PreparedCanonProof::decode(&bytes[..bytes.len() - 1], bytes.len()).is_err());
    decoded.record.binding.selection.program_digest = "ab".repeat(32);
    let mut mismatched = Vec::new();
    ciborium::ser::into_writer(&decoded.record, &mut mismatched)
        .expect("malformed fixture encodes");
    assert!(PreparedCanonProof::decode(&mismatched, mismatched.len()).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(PreparedCanonProof::decode(&trailing, trailing.len()).is_err());
}
