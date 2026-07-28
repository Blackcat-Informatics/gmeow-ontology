// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Bounded means–end search over authored `logic:DecompositionMethod`s (RQ2/RQ3).
//!
//! # Why this is bounded, and why it says so
//!
//! Hierarchical task-network decomposition is undecidable in general. A search that
//! reports "these are the candidates" without naming the fragment it ran within is
//! making a claim it cannot support, and the failure is silent: an operator reads a
//! roster, assumes it is the roster, and never learns that the search stopped early or
//! that the method set was outside anything decidable.
//!
//! So this module keeps THREE outcomes apart that a naive search collapses into one:
//!
//! * [`SearchStatus::CompleteForFragment`] — the search ran to exhaustion inside the
//!   declared fragment. The candidate set is closed.
//! * [`SearchStatus::IncompleteByBudget`] — the search was correct but ran out of
//!   expansions. The candidates found are real; the roster is NOT closed.
//! * [`SearchStatus::UnsupportedFragment`] — the method set is outside the declared
//!   fragment (a decomposition cycle under `logic:FragmentAcyclicMethod`). This is not
//!   a budget problem and must not be reported as one: no budget would fix it.
//!
//! The distinction between the last two is the point. "Incomplete because I ran out of
//! steps" invites a retry with a bigger budget; "incomplete because your method set has
//! a cycle" invites an authoring fix. Reporting the second as the first sends an
//! operator to buy compute for a problem compute cannot solve.
//!
//! # Determinism
//!
//! Methods are enumerated in sorted IRI order and expansion is breadth-first, so the
//! candidate list and the point at which a budget cut lands are both reproducible. The
//! budget counts METHOD APPLICATIONS, not wall time, for the same reason.

use std::collections::{BTreeMap, VecDeque};

/// The `logic:` namespace this search reads.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The declared fragment forbidding decomposition cycles.
const FRAGMENT_ACYCLIC: &str = "https://blackcatinformatics.ca/logic/FragmentAcyclicMethod";

/// One authored decomposition method: a task it decomposes, and the subtasks it yields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Method {
    /// The method's own IRI.
    pub iri: String,
    /// The task this method decomposes (`logic:methodDecomposes`).
    pub decomposes: String,
    /// The subtasks it yields, in authored order (`logic:methodYields`).
    pub yields: Vec<String>,
}

/// How a search terminated. Three outcomes, deliberately not two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchStatus {
    /// Ran to exhaustion within the declared fragment: the candidate set is CLOSED.
    CompleteForFragment,
    /// Correct but cut short. The candidates are real; the roster is not closed.
    IncompleteByBudget {
        /// The expansion budget that was exhausted.
        budget: u32,
    },
    /// The method set is outside the declared fragment. No budget would fix this.
    UnsupportedFragment {
        /// What put it out of fragment, named concretely enough to act on.
        condition: String,
    },
}

/// The result of a bounded means–end search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchResult {
    /// Every fully-decomposed candidate: the ordered primitive tasks it reduces to.
    pub candidates: Vec<Vec<String>>,
    /// How the search terminated.
    pub status: SearchStatus,
    /// Method applications consumed.
    pub expansions: u32,
}

/// Index methods by the task they decompose, in deterministic order.
fn index(methods: &[Method]) -> BTreeMap<String, Vec<&Method>> {
    let mut by_task: BTreeMap<String, Vec<&Method>> = BTreeMap::new();
    for m in methods {
        by_task.entry(m.decomposes.clone()).or_default().push(m);
    }
    for v in by_task.values_mut() {
        v.sort_by(|a, b| a.iri.cmp(&b.iri));
    }
    by_task
}

/// Detect a decomposition cycle: a task reachable from itself through methods.
///
/// Returned as a named condition rather than a bool because "your method set has a
/// cycle" is only actionable if it says WHICH task closes the loop.
fn cycle_condition(by_task: &BTreeMap<String, Vec<&Method>>) -> Option<String> {
    // Depth-first with an explicit colour map; iterative, so a deep method set cannot
    // blow the stack while diagnosing a method set that is already pathological.
    #[derive(Clone, Copy, PartialEq)]
    enum Colour {
        White,
        Grey,
        Black,
    }
    let mut colour: BTreeMap<&str, Colour> = BTreeMap::new();
    for t in by_task.keys() {
        colour.insert(t.as_str(), Colour::White);
    }
    let roots: Vec<&str> = by_task.keys().map(String::as_str).collect();
    for root in roots {
        if colour.get(root) != Some(&Colour::White) {
            continue;
        }
        let mut stack: Vec<(&str, usize)> = vec![(root, 0)];
        colour.insert(root, Colour::Grey);
        while let Some((task, idx)) = stack.pop() {
            let children: Vec<&str> = by_task
                .get(task)
                .map(|ms| {
                    ms.iter()
                        .flat_map(|m| m.yields.iter().map(String::as_str))
                        .collect()
                })
                .unwrap_or_default();
            if idx < children.len() {
                stack.push((task, idx + 1));
                let child = children[idx];
                match colour.get(child) {
                    Some(Colour::Grey) => {
                        return Some(format!(
                            "decomposition cycle: <{child}> is reachable from itself through the \
                             authored method set, so no expansion terminates"
                        ));
                    }
                    Some(Colour::White) | None => {
                        colour.insert(child, Colour::Grey);
                        stack.push((child, 0));
                    }
                    Some(Colour::Black) => {}
                }
            } else {
                colour.insert(task, Colour::Black);
            }
        }
    }
    None
}

/// Run a bounded means–end search from `root` over `methods`.
///
/// `budget` counts method applications. `fragment` is the declared
/// `logic:SearchFragment`; under [`FRAGMENT_ACYCLIC`] a decomposition cycle is an
/// out-of-fragment refusal rather than an infinite expansion.
///
/// A task with no method decomposing it is PRIMITIVE — it is a leaf of the candidate,
/// not a failure. That is what lets a partially-methodised domain still produce useful
/// candidates instead of nothing.
pub(crate) fn search(root: &str, methods: &[Method], fragment: &str, budget: u32) -> SearchResult {
    let by_task = index(methods);

    if fragment == FRAGMENT_ACYCLIC
        && let Some(condition) = cycle_condition(&by_task)
    {
        // Refused BEFORE any expansion: reporting a cycle as a budget cut would send
        // someone to raise a limit that cannot help.
        return SearchResult {
            candidates: Vec::new(),
            status: SearchStatus::UnsupportedFragment { condition },
            expansions: 0,
        };
    }

    let mut expansions: u32 = 0;
    let mut candidates: Vec<Vec<String>> = Vec::new();
    // Breadth-first over partial decompositions, so a budget cut lands at a
    // reproducible frontier rather than wherever a depth-first dive happened to be.
    let mut queue: VecDeque<Vec<String>> = VecDeque::new();
    queue.push_back(vec![root.to_owned()]);

    while let Some(state) = queue.pop_front() {
        // The first non-primitive task, left to right: totally-ordered expansion.
        let open = state.iter().position(|t| by_task.contains_key(t));
        let Some(pos) = open else {
            // Fully primitive: a candidate.
            if !candidates.contains(&state) {
                candidates.push(state);
            }
            continue;
        };
        if expansions >= budget {
            // Correct-but-cut. Everything already in `candidates` is real; the roster
            // is not closed, and the status says so rather than the caller guessing.
            return SearchResult {
                candidates,
                status: SearchStatus::IncompleteByBudget { budget },
                expansions,
            };
        }
        let task = state[pos].clone();
        for m in by_task.get(&task).into_iter().flatten() {
            expansions += 1;
            let mut next = state.clone();
            next.splice(pos..=pos, m.yields.iter().cloned());
            queue.push_back(next);
        }
    }

    SearchResult {
        candidates,
        status: SearchStatus::CompleteForFragment,
        expansions,
    }
}

/// `rdf:first` — the head cell of an RDF list.
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
/// `rdf:rest` — the tail cell of an RDF list.
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// `rdf:nil` — the empty list, and the only legal terminator of a well-formed chain.
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// Walk the `rdf:List` rooted at `head` into its members, in list order.
///
/// Returns `None` for a MALFORMED chain — a cell missing `rdf:first` or `rdf:rest`, or one
/// that loops back on itself — rather than the prefix it managed to read. A truncated
/// prefix of a plan is a different, shorter plan, and returning one would let a broken list
/// silently delete the steps after the break; the caller drops the method instead, which is
/// visible as a missing candidate rather than invisible as a wrong one.
///
/// `rdf:nil` reads as the empty list, which [`methods_from_triples`] rejects for the same
/// reason it rejects a method with no list at all.
fn list_members(
    head: &str,
    first: &BTreeMap<&str, &str>,
    rest: &BTreeMap<&str, &str>,
) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut cell = head;
    while cell != RDF_NIL {
        // A cell revisited is a cyclic chain: it has no last element, so it denotes no
        // sequence at all and no amount of further walking would terminate.
        if !seen.insert(cell) {
            return None;
        }
        out.push((*first.get(cell)?).to_owned());
        cell = rest.get(cell)?;
    }
    Some(out)
}

/// Extract authored methods from a set of `(subject, predicate, object)` triples.
///
/// Reads `logic:methodDecomposes` and the ORDERED `logic:methodYields` list. A method
/// declaring no `methodYields`, or one whose list is empty or malformed, is skipped rather
/// than treated as decomposing to nothing: "this method reduces the task to the empty plan"
/// is a claim an author should have to make explicitly, and silently inventing it would let
/// a typo delete work from a plan.
///
/// # Order is read, not guessed
///
/// `logic:methodYields` carries ONE `rdf:List`, and this reader walks it, so
/// `inspect → prepare → extract → verify → store` comes back in that sequence. That matters
/// because the sequence IS the plan: verifying before extracting verifies nothing. The
/// carrier is a list precisely because repeated triples would be an unordered set, leaving
/// a reader to impose an order — and any order it imposed (insertion, alphabetical) would
/// be a plan nobody authored.
///
/// A method carrying two `logic:methodYields` lists is skipped: the property is
/// single-valued, and picking one of two candidate sequences is exactly the arbitrary
/// choice the ordered carrier exists to abolish.
pub(crate) fn methods_from_triples(rows: &[(String, String, String)]) -> Vec<Method> {
    let decomposes_p = format!("{LOGIC_NS}methodDecomposes");
    let yields_p = format!("{LOGIC_NS}methodYields");

    let mut decomposes: BTreeMap<&str, &str> = BTreeMap::new();
    // `None` marks a method that named more than one yields list — recorded rather than
    // dropped on the floor so the ambiguity is what removes the method, not the last write.
    let mut yields: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    let mut first: BTreeMap<&str, &str> = BTreeMap::new();
    let mut rest: BTreeMap<&str, &str> = BTreeMap::new();
    for (s, p, o) in rows {
        if *p == decomposes_p {
            decomposes.insert(s.as_str(), o.as_str());
        } else if *p == yields_p {
            yields
                .entry(s.as_str())
                .and_modify(|slot| {
                    if *slot != Some(o.as_str()) {
                        *slot = None;
                    }
                })
                .or_insert(Some(o.as_str()));
        } else if *p == RDF_FIRST {
            first.insert(s.as_str(), o.as_str());
        } else if *p == RDF_REST {
            rest.insert(s.as_str(), o.as_str());
        }
    }

    let mut out: Vec<Method> = Vec::new();
    for (iri, task) in decomposes {
        let Some(Some(head)) = yields.get(iri) else {
            continue;
        };
        let Some(ys) = list_members(head, &first, &rest) else {
            continue;
        };
        if ys.is_empty() {
            continue;
        }
        out.push(Method {
            iri: iri.to_owned(),
            decomposes: task.to_owned(),
            yields: ys,
        });
    }
    out.sort_by(|a, b| a.iri.cmp(&b.iri));
    out
}

/// Every task the method set never decomposes — the primitives a candidate bottoms out in.
#[cfg(test)]
fn primitives(methods: &[Method]) -> std::collections::BTreeSet<String> {
    let decomposed: std::collections::BTreeSet<&str> =
        methods.iter().map(|m| m.decomposes.as_str()).collect();
    methods
        .iter()
        .flat_map(|m| m.yields.iter())
        .filter(|y| !decomposed.contains(y.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(iri: &str, task: &str, ys: &[&str]) -> Method {
        Method {
            iri: iri.to_owned(),
            decomposes: task.to_owned(),
            yields: ys.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// The POSITIVE path first: a search that only ever declines proves the capability is
    /// absent, not present.
    #[test]
    fn a_methodised_task_decomposes_to_its_primitive_steps() {
        let methods = vec![m(
            "ex:ocrMethod",
            "ex:ocr",
            &[
                "ex:inspect",
                "ex:prepare",
                "ex:extract",
                "ex:verify",
                "ex:store",
            ],
        )];
        let r = search("ex:ocr", &methods, FRAGMENT_ACYCLIC, 100);
        assert_eq!(r.status, SearchStatus::CompleteForFragment);
        assert_eq!(r.candidates.len(), 1);
        assert_eq!(
            r.candidates[0],
            vec![
                "ex:inspect".to_owned(),
                "ex:prepare".to_owned(),
                "ex:extract".to_owned(),
                "ex:verify".to_owned(),
                "ex:store".to_owned()
            ],
            "the candidate preserves the METHOD'S order — inspect before verify before \
             store is the plan, and a search that reordered it would be proposing a \
             different plan than the one authored"
        );
    }

    #[test]
    fn two_methods_for_one_task_yield_two_candidates() {
        let methods = vec![
            m("ex:fast", "ex:ocr", &["ex:quickExtract"]),
            m("ex:thorough", "ex:ocr", &["ex:extract", "ex:verify"]),
        ];
        let r = search("ex:ocr", &methods, FRAGMENT_ACYCLIC, 100);
        assert_eq!(r.status, SearchStatus::CompleteForFragment);
        assert_eq!(r.candidates.len(), 2, "both methods are real alternatives");
    }

    #[test]
    fn nested_methods_expand_transitively() {
        let methods = vec![
            m("ex:top", "ex:ingest", &["ex:ocr", "ex:store"]),
            m("ex:sub", "ex:ocr", &["ex:extract", "ex:verify"]),
        ];
        let r = search("ex:ingest", &methods, FRAGMENT_ACYCLIC, 100);
        assert_eq!(r.status, SearchStatus::CompleteForFragment);
        assert_eq!(r.candidates.len(), 1);
        assert!(
            !r.candidates[0].contains(&"ex:ocr".to_owned()),
            "a task with a method must not survive into a candidate as a leaf"
        );
        assert!(r.candidates[0].contains(&"ex:extract".to_owned()));
    }

    /// A budget cut is a DIFFERENT answer from an out-of-fragment refusal, and this is
    /// the pair of tests that keeps them apart.
    #[test]
    fn a_budget_cut_reports_incomplete_and_does_not_claim_closure() {
        let methods = vec![
            m("ex:a", "ex:t", &["ex:t1", "ex:t2"]),
            m("ex:b", "ex:t1", &["ex:t3"]),
            m("ex:c", "ex:t2", &["ex:t4"]),
        ];
        let r = search("ex:t", &methods, FRAGMENT_ACYCLIC, 1);
        assert_eq!(r.status, SearchStatus::IncompleteByBudget { budget: 1 });
        assert_ne!(
            r.status,
            SearchStatus::CompleteForFragment,
            "a cut roster must never present as closed — that is the whole failure this \
             status exists to prevent"
        );
    }

    #[test]
    fn a_method_cycle_is_out_of_fragment_not_a_budget_problem() {
        let methods = vec![m("ex:a", "ex:t", &["ex:u"]), m("ex:b", "ex:u", &["ex:t"])];
        let r = search("ex:t", &methods, FRAGMENT_ACYCLIC, 1000);
        match &r.status {
            SearchStatus::UnsupportedFragment { condition } => {
                assert!(
                    condition.contains("cycle"),
                    "the refusal must name the cycle, not merely refuse: {condition}"
                );
            }
            other => panic!("a cyclic method set must be out-of-fragment, got {other:?}"),
        }
        assert_eq!(
            r.expansions, 0,
            "an out-of-fragment method set is refused BEFORE expansion — spending budget \
             on it would produce a budget cut that hides the real cause"
        );
    }

    #[test]
    fn an_unmethodised_root_is_primitive_not_a_failure() {
        let r = search("ex:atomic", &[], FRAGMENT_ACYCLIC, 10);
        assert_eq!(r.status, SearchStatus::CompleteForFragment);
        assert_eq!(r.candidates, vec![vec!["ex:atomic".to_owned()]]);
    }

    /// A triple `(s, p, o)` with owned strings, for the reader tests.
    fn t(s: &str, p: &str, o: &str) -> (String, String, String) {
        (s.to_owned(), p.to_owned(), o.to_owned())
    }

    /// The `rdf:first`/`rdf:rest` triples of a named-cell list `cell_prefix`0..n → `rdf:nil`.
    fn list_rows(cell_prefix: &str, members: &[&str]) -> Vec<(String, String, String)> {
        let mut rows = Vec::new();
        for (i, m) in members.iter().enumerate() {
            let cell = format!("{cell_prefix}{i}");
            let next = if i + 1 == members.len() {
                RDF_NIL.to_owned()
            } else {
                format!("{cell_prefix}{}", i + 1)
            };
            rows.push(t(&cell, RDF_FIRST, m));
            rows.push(t(&cell, RDF_REST, &next));
        }
        rows
    }

    #[test]
    fn methods_are_read_from_triples_and_a_yieldless_method_is_skipped() {
        let ns = LOGIC_NS;
        let mut rows = vec![
            t("ex:m1", &format!("{ns}methodDecomposes"), "ex:t"),
            t("ex:m1", &format!("{ns}methodYields"), "ex:c0"),
            // Declares a task but yields nothing: skipped rather than silently treated
            // as reducing the task to the empty plan.
            t("ex:m2", &format!("{ns}methodDecomposes"), "ex:t"),
        ];
        rows.extend(list_rows("ex:c", &["ex:s1"]));
        let got = methods_from_triples(&rows);
        assert_eq!(
            got.len(),
            1,
            "the yieldless method must not become a method"
        );
        assert_eq!(got[0].iri, "ex:m1");
        assert_eq!(got[0].yields, vec!["ex:s1".to_owned()]);
    }

    /// The reader returns the AUTHORED sequence, and the test proves it by choosing a
    /// sequence no accidental ordering would reproduce.
    ///
    /// `inspect → prepare → extract → verify → store` is neither alphabetical
    /// (`extract, inspect, prepare, store, verify`) nor reverse-alphabetical, so a reader
    /// that sorted, reversed, or fell back on triple-insertion order would fail here.
    /// Alphabetised, this plan verifies before it extracts.
    #[test]
    fn an_ordered_yields_list_is_read_in_authored_order_not_sorted() {
        let ns = LOGIC_NS;
        let mut rows = vec![
            t("ex:ocrMethod", &format!("{ns}methodDecomposes"), "ex:ocr"),
            t("ex:ocrMethod", &format!("{ns}methodYields"), "ex:cell0"),
        ];
        rows.extend(list_rows(
            "ex:cell",
            &[
                "ex:inspect",
                "ex:prepare",
                "ex:extract",
                "ex:verify",
                "ex:store",
            ],
        ));
        let got = methods_from_triples(&rows);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].yields,
            vec![
                "ex:inspect".to_owned(),
                "ex:prepare".to_owned(),
                "ex:extract".to_owned(),
                "ex:verify".to_owned(),
                "ex:store".to_owned()
            ],
            "the list order IS the plan; a reader that sorted it would propose verifying \
             before extracting"
        );
    }

    /// The whole point, end to end: an authored ordered method decomposes into its steps
    /// IN ORDER through the public search, not merely through the reader.
    #[test]
    fn a_task_refines_through_an_ordered_list_into_its_authored_sequence() {
        let ns = LOGIC_NS;
        let mut rows = vec![
            t("ex:ocrMethod", &format!("{ns}methodDecomposes"), "ex:ocr"),
            t("ex:ocrMethod", &format!("{ns}methodYields"), "ex:cell0"),
        ];
        rows.extend(list_rows(
            "ex:cell",
            &["ex:inspect", "ex:prepare", "ex:extract"],
        ));
        let methods = methods_from_triples(&rows);
        let r = search("ex:ocr", &methods, FRAGMENT_ACYCLIC, 100);
        assert_eq!(r.status, SearchStatus::CompleteForFragment);
        assert_eq!(r.expansions, 1, "the one applicable method was applied once");
        assert_eq!(
            r.candidates,
            vec![vec![
                "ex:inspect".to_owned(),
                "ex:prepare".to_owned(),
                "ex:extract".to_owned()
            ]]
        );
    }

    /// A malformed chain drops the method rather than yielding the prefix it could read.
    ///
    /// A truncated prefix is a DIFFERENT, shorter plan. Returning one would silently delete
    /// every step after the break — the failure mode where a system runs four fifths of a
    /// procedure and reports success.
    #[test]
    fn a_broken_yields_chain_drops_the_method_rather_than_truncating_the_plan() {
        let ns = LOGIC_NS;
        let rows = vec![
            t("ex:m", &format!("{ns}methodDecomposes"), "ex:t"),
            t("ex:m", &format!("{ns}methodYields"), "ex:c0"),
            t("ex:c0", RDF_FIRST, "ex:s1"),
            t("ex:c0", RDF_REST, "ex:c1"),
            // ex:c1 carries a first but no rest: the chain never reaches rdf:nil.
            t("ex:c1", RDF_FIRST, "ex:s2"),
        ];
        assert!(
            methods_from_triples(&rows).is_empty(),
            "a chain that never terminates at rdf:nil denotes no sequence, so it must not \
             become a method whose plan is silently one step short"
        );
    }

    /// A cyclic chain terminates the walk instead of hanging it.
    #[test]
    fn a_cyclic_yields_chain_is_rejected_rather_than_walked_forever() {
        let ns = LOGIC_NS;
        let rows = vec![
            t("ex:m", &format!("{ns}methodDecomposes"), "ex:t"),
            t("ex:m", &format!("{ns}methodYields"), "ex:c0"),
            t("ex:c0", RDF_FIRST, "ex:s1"),
            t("ex:c0", RDF_REST, "ex:c1"),
            t("ex:c1", RDF_FIRST, "ex:s2"),
            t("ex:c1", RDF_REST, "ex:c0"),
        ];
        assert!(methods_from_triples(&rows).is_empty());
    }

    /// Two yields lists on one method is an ambiguity, and the ambiguity removes the method.
    ///
    /// `logic:methodYields` is single-valued. Silently keeping one of the two sequences
    /// would restore exactly what the ordered carrier abolished: an arbitrary choice among
    /// candidate plans, decided by read order.
    #[test]
    fn a_method_naming_two_yields_lists_is_skipped() {
        let ns = LOGIC_NS;
        let mut rows = vec![
            t("ex:m", &format!("{ns}methodDecomposes"), "ex:t"),
            t("ex:m", &format!("{ns}methodYields"), "ex:a0"),
            t("ex:m", &format!("{ns}methodYields"), "ex:b0"),
        ];
        rows.extend(list_rows("ex:a", &["ex:s1"]));
        rows.extend(list_rows("ex:b", &["ex:s2"]));
        assert!(
            methods_from_triples(&rows).is_empty(),
            "two candidate sequences is not one sequence, and choosing between them by read \
             order is the defect the ordered carrier exists to prevent"
        );
    }

    /// An explicitly empty list is the "reduces to nothing" claim, and is refused like a
    /// missing one.
    #[test]
    fn a_yields_list_of_rdf_nil_is_skipped() {
        let ns = LOGIC_NS;
        let rows = vec![
            t("ex:m", &format!("{ns}methodDecomposes"), "ex:t"),
            t("ex:m", &format!("{ns}methodYields"), RDF_NIL),
        ];
        assert!(methods_from_triples(&rows).is_empty());
    }

    #[test]
    fn search_is_deterministic_across_method_input_order() {
        let a = vec![
            m("ex:fast", "ex:ocr", &["ex:quick"]),
            m("ex:thorough", "ex:ocr", &["ex:extract", "ex:verify"]),
        ];
        let b = vec![
            m("ex:thorough", "ex:ocr", &["ex:extract", "ex:verify"]),
            m("ex:fast", "ex:ocr", &["ex:quick"]),
        ];
        assert_eq!(
            search("ex:ocr", &a, FRAGMENT_ACYCLIC, 100),
            search("ex:ocr", &b, FRAGMENT_ACYCLIC, 100),
            "candidate order must not depend on the order methods happened to be read"
        );
    }

    #[test]
    fn primitives_are_the_tasks_no_method_decomposes() {
        let methods = vec![
            m("ex:top", "ex:ingest", &["ex:ocr", "ex:store"]),
            m("ex:sub", "ex:ocr", &["ex:extract"]),
        ];
        let p = primitives(&methods);
        assert!(p.contains("ex:store"));
        assert!(p.contains("ex:extract"));
        assert!(
            !p.contains("ex:ocr"),
            "ex:ocr has a method, so it is not primitive"
        );
    }
}
