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

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;

use gmeow_gts::model::{Graph, Term, TermKind};
use gmeow_gts::writer::Writer;
use gmeow_rdf::{import_gts_events, import_gts_graph, RdfDataset, TermId, TermRef};

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
        reifier: None,
    }); // 4 language-tagged literal
    g.terms
        .push(iri("http://www.w3.org/2001/XMLSchema#integer")); // 5 datatype IRI
    g.terms.push(Term {
        kind: TermKind::Literal,
        value: Some("42".to_owned()),
        datatype: Some(5),
        lang: None,
        reifier: None,
    }); // 6 typed literal

    // Quads: default graph, named graph, literal objects.
    g.quads.push((0, 1, 2, None));
    g.quads.push((0, 1, 4, Some(3)));
    g.quads.push((0, 1, 6, None));
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
}

/// Single-segment input WITH blanks: the two paths assign different blank SCOPES, so
/// full isomorphism is not asserted here (the public blank-aware structural
/// comparator lands in Task 6). We assert the weaker, exact invariants: equal quad
/// count and equal NON-blank term values.
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

/// `import_gts_graph` MOVES owned term strings into the interner. We allocate a very
/// long IRI when building the `Graph` (BEFORE the measured window), then import it
/// and bound the bytes allocated DURING import.
///
/// The interner deduplicates by keeping the term in BOTH a `Vec` and a `HashMap`
/// index, so it clones the interned term exactly ONCE internally — an unavoidable
/// ~1× IRI-length cost shared by every import path. The importer's own contribution
/// is the difference: a MOVE adds nothing length-proportional (total ≈ 1× the IRI),
/// whereas a CLONE-based importer would copy the IRI an extra time (total ≈ 2×). We
/// therefore assert the import stays well under 2× the IRI length — only achievable
/// if the importer MOVES the string out of the `Graph` rather than cloning it.
#[test]
fn import_moves_long_iri_string_not_clone() {
    // A long IRI whose backing `String` has capacity == len (so the interner's
    // `into_boxed_str` shrink is a no-op and cannot mask the measurement).
    let mut long: String = String::from("http://example.org/") + &"x".repeat(200_000) + "#term";
    // Force capacity == len so the interner's `into_boxed_str` shrink is a no-op and
    // cannot reallocate (copy) the string inside the measured window. This shrink
    // happens BEFORE the measured window regardless.
    long.shrink_to_fit();
    let long_len = long.len();
    assert!(long_len > 200_000);

    // Build the graph; the long IRI's bytes are allocated HERE, before measuring.
    let mut graph = Graph::default();
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some(long),
        datatype: None,
        lang: None,
        reifier: None,
    });
    graph.terms.push(iri("http://example.org/p"));
    graph.terms.push(iri("http://example.org/o"));
    graph.quads.push((0, 1, 2, None));

    let before = allocated_bytes();
    let bundle = import_gts_graph(graph).expect("import");
    let after = allocated_bytes();

    // Sanity: the long IRI did survive into the interned dataset.
    let kept = bundle
        .dataset
        .quad_refs()
        .next()
        .map(|q| matches!(q.s, TermRef::Iri(s) if s.len() == long_len))
        .unwrap_or(false);
    assert!(kept, "the long IRI must be interned intact");

    let measured = after - before;
    // A move → ~1× the IRI (the interner's single index clone) plus small bookkeeping.
    // A clone-based importer → ~2× the IRI. Anything under 1.5× is only reachable by
    // moving the string out of the `Graph`.
    let move_ceiling = long_len + long_len / 2;
    assert!(
        measured < move_ceiling,
        "import allocated {measured} bytes for a {long_len}-byte IRI (ceiling \
         {move_ceiling}); a MOVE keeps this near 1× the IRI, a clone would push it \
         to ~2×"
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
    let ledger = gmeow_rdf::gts_to_rdf_loss_ledger();
    let entry = ledger
        .entries()
        .iter()
        .find(|e| e.code == "bnode-scope-flatten")
        .expect("ledger must document bnode-scope-flatten");
    assert!(entry.intentional, "the flatten loss is intentional");
    assert_eq!(entry.from, "gts");
    assert_eq!(entry.to, "rdf-1.2-dataset");
}
