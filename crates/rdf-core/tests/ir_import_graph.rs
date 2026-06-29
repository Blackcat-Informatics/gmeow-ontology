// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate 4 (#819 C2.b): the consuming `import_gts_graph` importer is
//! IR-equivalent to the authoritative event-sink importer on single-segment
//! input, and it MOVES owned term strings into the interner rather than cloning
//! them.
//!
//! The IR — not oxigraph — is the equality oracle: both paths are imported, frozen,
//! and their quads resolved to value tuples for a multiset comparison.
//!
//! The string-move proof is operational: a process-global allocator counts the
//! *bytes* requested on the measuring thread. A very long IRI string is allocated
//! when the GTS `Graph` is built (BEFORE the measured window); importing that graph
//! then allocates far fewer bytes than the IRI length, proving the IRI bytes were
//! MOVED into the interner, not copied.

#![cfg(feature = "gts")]

// Rich colored line-diffs on assert_eq! failure (#871); shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;

use gmeow_gts::model::{Graph, Term, TermKind};
use gmeow_gts::writer::Writer;
use gmeow_rdf_core::{
    datasets_isomorphic, import_gts_events, import_gts_graph, RdfDataset, TermId, TermRef,
};

// --- Bytes-counting thread-local allocator ---------------------------------
//
// Like `ir_zero_alloc.rs`, the counter is THREAD-LOCAL so concurrent sibling test
// threads sharing this `#[global_allocator]` cannot contaminate the measurement.
// Here we count BYTES (not events): a clone of an N-byte string is one allocation
// EVENT regardless of N, so only a byte count can distinguish move from copy.
thread_local! {
    static ALLOCATED_BYTES: Cell<usize> = const { Cell::new(0) };
}

struct ByteCountingAllocator;

fn add_bytes(n: usize) {
    let _ = ALLOCATED_BYTES.try_with(|c| c.set(c.get() + n));
}

// SAFETY: every method forwards to the system allocator with the same layout; the
// only added behavior is a thread-local byte counter increment on allocation paths.
unsafe impl GlobalAlloc for ByteCountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        add_bytes(layout.size());
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // A realloc that grows copies the old bytes; count the new size.
        add_bytes(new_size);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: ByteCountingAllocator = ByteCountingAllocator;

fn allocated_bytes() -> usize {
    ALLOCATED_BYTES.with(Cell::get)
}

// --- Structural multiset comparison ----------------------------------------

/// A value-resolved term, used as the comparison key. Blank nodes carry their LABEL
/// but NOT their scope, because the two import paths assign different scope numbers
/// (the event-sink uses per-segment scope; the consuming path flattens to 0). For a
/// blank-free dataset this is exact; for a blanks dataset it compares the structural
/// shape modulo scope.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum ValueTerm {
    Iri(String),
    BlankLabel(String),
    Literal {
        lexical: String,
        datatype: String,
        language: Option<String>,
    },
    Triple(Box<(ValueTerm, ValueTerm, ValueTerm)>),
}

fn resolve_value(dataset: &RdfDataset, id: TermId) -> ValueTerm {
    match dataset.resolve(id) {
        TermRef::Iri(iri) => ValueTerm::Iri(iri.to_owned()),
        TermRef::Blank { label, .. } => ValueTerm::BlankLabel(label.to_owned()),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => ValueTerm::Literal {
            lexical: lexical.to_owned(),
            datatype: match dataset.resolve(datatype) {
                TermRef::Iri(iri) => iri.to_owned(),
                other => panic!("datatype must be an IRI, got {other:?}"),
            },
            language: language.map(str::to_owned),
        },
        TermRef::Triple { s, p, o } => ValueTerm::Triple(Box::new((
            resolve_value(dataset, s),
            resolve_value(dataset, p),
            resolve_value(dataset, o),
        ))),
    }
}

/// The quad multiset of a dataset as resolved value tuples (graph included).
fn quad_value_multiset(
    dataset: &RdfDataset,
) -> BTreeMap<(ValueTerm, ValueTerm, ValueTerm, Option<ValueTerm>), usize> {
    let mut multiset = BTreeMap::new();
    for q in dataset.quads() {
        let key = (
            resolve_value(dataset, q.s),
            resolve_value(dataset, q.p),
            resolve_value(dataset, q.o),
            q.g.map(|g| resolve_value(dataset, g)),
        );
        *multiset.entry(key).or_insert(0) += 1;
    }
    multiset
}

// --- Fixture builders ------------------------------------------------------

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_owned()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    }
}

/// A single-segment blank-FREE graph: IRIs, a language-tagged literal, a typed
/// literal, and a named-graph quad. All terms here have stable value identity, so
/// the two import paths must be exactly isomorphic. (Quoted triple terms are
/// exercised separately in the `import_graph` unit tests; the *event-sink* path's
/// bytes-round-trip ordering of reifier-bound triple terms is its own contract.)
fn blank_free_segment() -> Graph {
    let mut g = Graph::default();
    g.terms.push(iri("http://example.org/s")); // 0
    g.terms.push(iri("http://example.org/p")); // 1
    g.terms.push(iri("http://example.org/o")); // 2
    g.terms.push(iri("http://example.org/graph")); // 3
    g.terms.push(Term {
        kind: TermKind::Literal,
        value: Some("Bonjour".to_owned()),
        datatype: None,
        lang: Some("fr".to_owned()),
        direction: None,
        reifier: None,
    }); // 4 language-tagged literal
    g.terms
        .push(iri("http://www.w3.org/2001/XMLSchema#integer")); // 5 datatype IRI
    g.terms.push(Term {
        kind: TermKind::Literal,
        value: Some("42".to_owned()),
        datatype: Some(5),
        lang: None,
        direction: None,
        reifier: None,
    }); // 6 typed literal

    // Quads: default graph, named graph, literal objects.
    g.quads.push((0, 1, 2, None));
    g.quads.push((0, 1, 4, Some(3)));
    g.quads.push((0, 1, 6, None));
    g
}

/// A single-segment, blank-FREE graph that DOES carry quoted-triple terms — both a
/// flat `<<ex:s ex:p ex:o>>` (in object position, also reified) and a NESTED
/// `<< <<ex:s ex:p ex:o>> ex:says ex:o2 >>`. The reifier bindings are emitted in a
/// separate `reifies` frame AFTER the `terms` frame, so the event path must be
/// order-independent (the two-phase fix) to import this at all. Blank-free, so the
/// two paths must be exactly isomorphic INCLUDING the quoted triples.
fn blank_free_triples_segment() -> Graph {
    let mut g = Graph::default();
    g.terms.push(iri("http://example.org/s")); // 0
    g.terms.push(iri("http://example.org/p")); // 1
    g.terms.push(iri("http://example.org/o")); // 2
    g.terms.push(iri("http://example.org/r0")); // 3 inner reifier resource
    g.reifiers.push((3, (0, 1, 2), None));
    g.terms.push(Term {
        kind: TermKind::Triple,
        value: None,
        datatype: None,
        lang: None,
        direction: None,
        reifier: Some(3),
    }); // 4 inner <<ex:s ex:p ex:o>>
    g.terms.push(iri("http://example.org/asserts")); // 5
                                                     // Quad with the flat quoted triple in OBJECT position: (ex:s ex:asserts <<...>>).
    g.quads.push((0, 5, 4, None));

    g.terms.push(iri("http://example.org/says")); // 6
    g.terms.push(iri("http://example.org/o2")); // 7
    g.terms.push(iri("http://example.org/r1")); // 8 outer reifier resource
                                                // Outer triple << <<ex:s ex:p ex:o>> ex:says ex:o2 >> — inner triple (4) is the
                                                // SUBJECT.
    g.reifiers.push((8, (4, 6, 7), None));
    g.terms.push(Term {
        kind: TermKind::Triple,
        value: None,
        datatype: None,
        lang: None,
        direction: None,
        reifier: Some(8),
    }); // 9 outer << <<...>> ex:says ex:o2 >>
    g.terms.push(iri("http://example.org/states")); // 10
                                                    // Quad with the NESTED quoted triple in object position.
    g.quads.push((0, 10, 9, None));
    g
}

/// A single-segment graph WITH blanks (for the count + non-blank equality fixture).
fn blanks_segment() -> Graph {
    let mut g = Graph::default();
    g.terms.push(iri("http://example.org/s")); // 0
    g.terms.push(iri("http://example.org/p")); // 1
    g.terms.push(Term {
        kind: TermKind::Bnode,
        value: Some("b1".to_owned()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    }); // 2 blank
    g.quads.push((0, 1, 2, None));
    g
}

fn to_bytes(graph: &Graph) -> Vec<u8> {
    Writer::deterministic(graph, "gmeow-rdf-test")
        .expect("deterministic writer")
        .to_bytes()
}

// --- Gate 4: equivalence ---------------------------------------------------

/// Blank-FREE single-segment input: `import_gts_graph(read(bytes))` and
/// `import_gts_events(bytes)` produce structurally ISOMORPHIC datasets (exact quad
/// multiset and term-count equality). The IR is the equality oracle.
#[test]
fn graph_and_event_paths_are_isomorphic_blank_free() {
    let bytes = to_bytes(&blank_free_segment());

    let folded = gmeow_gts::reader::read(&bytes, true, None);
    let via_graph = import_gts_graph(folded).expect("graph-path import");
    let via_events = import_gts_events(&bytes).expect("event-path import");

    let graph_ds = &via_graph.dataset;
    let events_ds = &via_events.dataset;

    assert_eq!(
        graph_ds.quad_count(),
        events_ds.quad_count(),
        "both paths import the same number of quads"
    );
    assert_eq!(
        graph_ds.term_count(),
        events_ds.term_count(),
        "blank-free terms have stable value identity → same term count"
    );
    assert_eq!(
        quad_value_multiset(graph_ds),
        quad_value_multiset(events_ds),
        "blank-free datasets are structurally isomorphic across import paths"
    );
    // Task 6: the blank-aware IR-direct comparator is now the equality oracle.
    assert!(
        datasets_isomorphic(graph_ds, events_ds),
        "datasets_isomorphic agrees the blank-free paths are isomorphic"
    );
}

/// Blank-FREE single-segment input WITH (nested) quoted-triple terms:
/// `import_gts_graph(read(bytes))` and `import_gts_events(bytes)` produce
/// structurally ISOMORPHIC datasets, INCLUDING the quoted triples. This is the
/// regression the two-phase event-sink importer fixes — `Writer::deterministic`
/// emits the `reifies` frame (the triple bindings) AFTER the `terms` frame, so the
/// old single-pass event path failed on Writer-serialized quoted triples and this
/// fixture had to be triple-free. The IR is the equality oracle.
#[test]
fn graph_and_event_paths_are_isomorphic_blank_free_with_triples() {
    let bytes = to_bytes(&blank_free_triples_segment());

    let folded = gmeow_gts::reader::read(&bytes, true, None);
    let via_graph = import_gts_graph(folded).expect("graph-path import");
    let via_events = import_gts_events(&bytes).expect("event-path import (two-phase)");

    let graph_ds = &via_graph.dataset;
    let events_ds = &via_events.dataset;

    assert_eq!(
        graph_ds.quad_count(),
        events_ds.quad_count(),
        "both paths import the same number of quads (incl. quoted-triple objects)"
    );
    assert_eq!(
        graph_ds.term_count(),
        events_ds.term_count(),
        "blank-free terms (IRIs + structural quoted triples) → same term count"
    );
    assert_eq!(
        quad_value_multiset(graph_ds),
        quad_value_multiset(events_ds),
        "blank-free datasets are isomorphic across paths, INCLUDING nested quoted triples"
    );
    // Task 6: the IR-direct comparator confirms isomorphism through the quoted-triple
    // structure too (it canonicalizes triple terms recursively).
    assert!(
        datasets_isomorphic(graph_ds, events_ds),
        "datasets_isomorphic agrees, including nested quoted triples"
    );

    // Sanity: the event path really did materialize quoted triples (not skip them).
    assert!(
        events_ds
            .quad_refs()
            .any(|q| matches!(q.o, TermRef::Triple { .. })),
        "the event path must carry quoted-triple objects"
    );
    assert!(
        events_ds.capabilities().quoted_triples,
        "the frozen event-path dataset reports quoted-triple capability"
    );
}

/// Single-segment input WITH blanks: the two paths assign different blank SCOPES (the
/// graph path flattens to 0, the event path uses per-segment scope). Task 6's
/// blank-aware structural comparator now resolves the bijection, so we assert FULL
/// isomorphism via `datasets_isomorphic` — and keep the exact non-blank invariants as
/// a finer-grained guard.
#[test]
fn graph_and_event_paths_agree_on_non_blank_terms() {
    let bytes = to_bytes(&blanks_segment());

    let folded = gmeow_gts::reader::read(&bytes, true, None);
    let via_graph = import_gts_graph(folded).expect("graph-path import");
    let via_events = import_gts_events(&bytes).expect("event-path import");

    assert_eq!(
        via_graph.dataset.quad_count(),
        via_events.dataset.quad_count(),
        "equal quad count with blanks"
    );

    // Task 6: full blank-aware isomorphism — the comparator resolves the differing
    // blank scopes via bijection, something the multiset oracle could not.
    assert!(
        datasets_isomorphic(&via_graph.dataset, &via_events.dataset),
        "blank-aware comparator proves the two import paths are isomorphic"
    );

    // The non-blank object/subject/predicate values match; only the blank's SCOPE
    // differs (graph path flattens to 0, event path uses per-segment scope).
    let non_blank = |ds: &RdfDataset| -> Vec<ValueTerm> {
        let mut out = Vec::new();
        for q in ds.quads() {
            for id in [q.s, q.p, q.o] {
                if !matches!(ds.resolve(id), TermRef::Blank { .. }) {
                    out.push(resolve_value(ds, id));
                }
            }
        }
        out.sort();
        out
    };
    assert_eq!(
        non_blank(&via_graph.dataset),
        non_blank(&via_events.dataset),
        "non-blank terms agree across paths"
    );

    // Both still carry exactly one blank object, by label.
    for ds in [&via_graph.dataset, &via_events.dataset] {
        let blank_labels: Vec<String> = ds
            .quad_refs()
            .filter_map(|q| match q.o {
                TermRef::Blank { label, .. } => Some(label.to_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(blank_labels, vec!["b1".to_owned()]);
    }
}

// --- Gate 4: string-move proof ---------------------------------------------

/// P3b (#879) stores all interned strings in ONE contiguous byte arena instead of a
/// `Box<str>` per term. An arena copies each string's bytes once into the shared
/// buffer (and copies the whole buffer once more when it is frozen at materialize) —
/// so the old "move the `String`'s heap buffer, never copy a byte" invariant no longer
/// applies, and measuring a single import's absolute byte count is no longer a
/// meaningful clone-detector (the fixed copy+freeze cost dwarfs it).
///
/// The invariant that DOES survive — and that P3b + store-once (#880) must uphold — is
/// that each DISTINCT string value lands in the arena EXACTLY ONCE. We assert it with a
/// delta: importing the same long IRI as four separate `Graph` terms must allocate
/// barely more than importing it once. Equal terms dedup by value before any arena
/// push, so the three extra copies add only constant bookkeeping (term-table / quad
/// rows), NOT three more arena copies of the IRI. A per-term arena copy (a dedup
/// regression) would add ≈3× the IRI length, blowing the ceiling. Taking the delta
/// cancels the fixed copy+freeze overhead that the absolute single-import number can't.
#[test]
fn import_dedups_equal_long_iris_in_arena() {
    let long: String = String::from("http://example.org/") + &"x".repeat(200_000) + "#term";
    let long_len = long.len();
    assert!(long_len > 200_000);

    // Import the long IRI as `copies` distinct subject terms (all equal) and return the
    // bytes allocated DURING import. The graph's own term strings are allocated before
    // the measured window, so the delta between two `copies` values isolates the arena.
    let import_with_copies = |copies: usize| -> usize {
        let mut graph = Graph::default();
        for _ in 0..copies {
            graph.terms.push(Term {
                kind: TermKind::Iri,
                value: Some(long.clone()),
                datatype: None,
                lang: None,
                direction: None,
                reifier: None,
            });
        }
        let p = copies; // predicate term index, right after the subject copies
        let o = copies + 1; // object term index
        graph.terms.push(iri("http://example.org/p"));
        graph.terms.push(iri("http://example.org/o"));
        for s in 0..copies {
            graph.quads.push((s, p, o, None));
        }

        let before = allocated_bytes();
        let bundle = import_gts_graph(graph).expect("import");
        let measured = allocated_bytes() - before;

        // Sanity: the long IRI survived into the interned dataset intact, and the equal
        // subjects collapsed to a single quad (one interned IRI shared by all rows).
        let kept = bundle
            .dataset
            .quad_refs()
            .next()
            .map(|q| matches!(q.s, TermRef::Iri(s) if s.len() == long_len))
            .unwrap_or(false);
        assert!(kept, "the long IRI must be interned intact");
        assert_eq!(
            bundle.dataset.quad_count(),
            1,
            "equal subjects over the same (p, o) dedup to ONE quad"
        );
        measured
    };

    let one = import_with_copies(1);
    let four = import_with_copies(4);

    // Store-once: the three EXTRA equal IRIs add no arena bytes (deduped before any
    // push) — only constant bookkeeping. A per-term arena copy would add ≈3× the IRI.
    let dedup_ceiling = one + long_len;
    assert!(
        four < dedup_ceiling,
        "four equal long IRIs allocated {four} bytes vs {one} for one (ceiling \
         {dedup_ceiling}); store-once keeps the three extras near zero arena bytes, a \
         per-term copy would add ≈3× the {long_len}-byte IRI"
    );
}

/// `import_gts_graph` takes the `Graph` BY VALUE (compile-time consumption proof):
/// the graph is unusable after the call. This is a structural guard that the API
/// owns its input so it is free to move out of it.
#[test]
fn import_consumes_graph_by_value() {
    let mut graph = Graph::default();
    graph.terms.push(iri("http://example.org/s"));
    graph.terms.push(iri("http://example.org/p"));
    graph.terms.push(iri("http://example.org/o"));
    graph.quads.push((0, 1, 2, None));
    let bundle = import_gts_graph(graph).expect("import");
    // `graph` has been moved; referencing it here would not compile. The successful
    // by-value call is the proof.
    assert_eq!(bundle.dataset.quad_count(), 1);
}

/// The loss ledger documents the `bnode-scope-flatten` loss this path incurs.
#[test]
fn loss_ledger_documents_bnode_scope_flatten() {
    let ledger = gmeow_rdf_core::gts_to_rdf_loss_ledger();
    let entry = ledger
        .entries()
        .iter()
        .find(|e| e.code == "bnode-scope-flatten")
        .expect("ledger must document bnode-scope-flatten");
    assert!(entry.intentional, "the flatten loss is intentional");
    assert_eq!(entry.from, "gts");
    assert_eq!(entry.to, "rdf-1.2-dataset");
}
