// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

#[cfg(test)]
/// Whether the closure contains one target IRI-object axiom.
///
/// A leave-one-out probe asks exactly one membership question. Borrowing the existing
/// strings and stopping on the first match avoids constructing a complete
/// `BTreeSet<String>` for every probe (up to 64 full closure re-indexes per slice).
pub(super) fn closure_contains_iri(
    inferred: &[InferredAxiom],
    subject: &str,
    predicate: &str,
    object: &str,
) -> bool {
    inferred.iter().any(|axiom| {
        axiom.subject == subject
            && axiom.predicate == predicate
            && axiom.object.as_iri() == Some(object)
    })
}

#[cfg(test)]
/// Rebuild the dataset without the single IRI triple `(s, p, o)`, preserving every
/// OTHER quad of every kind — blank-node (`owl:Restriction`-encoded) and literal
/// quads included. Only the exact `(s, p, o)` triple under test is removed; because
/// OWL restrictions and equivalences are blank-node encoded, dropping them would
/// corrupt the reasoned closure and thus the redundancy / clash scores. Blank
/// identity is preserved by round-tripping through the dataset's own
/// scope-qualified owned model (`owned_quads`), so co-referring blanks stay
/// co-referring after the rebuild.
pub(super) fn edb_without_triple(
    ds: &RdfDataset,
    drop_s: &str,
    drop_p: &str,
    drop_o: &str,
) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in ds.owned_quads() {
        // The target axiom is always an IRI→IRI-predicate→IRI triple; drop exactly
        // that one and preserve everything else regardless of term kind.
        if quad.predicate == drop_p
            && let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&quad.subject, &quad.object)
            && s == drop_s
            && o == drop_o
        {
            continue; // the triple under test — leave it out
        }
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .unwrap_or_else(|_| Arc::new(RdfDataset::union(&[])))
}
