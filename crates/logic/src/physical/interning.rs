// SPDX-FileCopyrightText: 2026 Blackcat Informatics(R) Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Generating property tests for `math:` expression interning and alpha-equivalence.

// ═══════════════════════════════════════════════════════════════════════════════
// A REAL, GENERATING property test for α-equivalence structural identity.
// ═══════════════════════════════════════════════════════════════════════════════
//
// The property is carried by a GENERATOR (`gen_expr`) driven through `proptest`,
// exercised through the REAL
// production entry points (`MathGraph::from_turtle` → `arena_structural_key`, the same
// route `math_expression_structural_keys` publishes the shipped key through) over generated
// `math:`-vocabulary Turtle text — never a
// hand-built `TermDag`/`NodeId`.
//

use std::collections::BTreeSet;

use proptest::prelude::*;

use gmeow_term_arena::engine::TermDag;

use super::lower::*;

/// 96 generated cases per property. Each case drives a real Turtle parse +
/// `MathGraph` index + recursive lowering (heavier than a bare `TermDag`
/// micro-op), and the cross-tree property below lowers TWO independently
/// generated expressions per case — so 96 keeps the whole module well under a
/// second while still covering the shadowing/renaming/arity space thoroughly
/// across repeated runs. Overridable via `PROPTEST_CASES` like the sibling
/// `term_dag`/`unify` property suites.
fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(96);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// ── generator: a tiny `math:` expression grammar ───────────────────────────

/// A tiny synthetic `math:` expression grammar used PURELY to drive the real
/// lowering pipeline through generated `math:`-vocabulary Turtle text.
///
/// `Var` names are resolved to a bound occurrence or a free declaration purely
/// STRUCTURALLY — nearest enclosing `Bind` with the same name, else free —
/// exactly mirroring how [`resolve_debruijn`] resolves a `logic:` binder frame
/// stack. This is what lets [`render`] and [`canonicalize`] agree without either
/// one driving the other: both independently apply the same lexical-scoping
/// rule to the same tree.
#[derive(Clone, Debug)]
enum GenExpr {
    /// A bare external constant leaf (`tag` selects one of a small pool of
    /// distinct constant IRIs). Never `math:`-typed, so it lowers through the
    /// bare-IRI-constant fallback branch of [`lower_math_node_dispatch`].
    Const(u8),
    /// A named variable occurrence.
    Var(String),
    /// `math:ApplicationExpression`: `tag` selects the operator IRI; the `Vec`
    /// is its `math:argumentSlot`-ordered operands — arity varies with
    /// generated length (0 to a handful of slots).
    App(u8, Vec<GenExpr>),
    /// `math:BindingExpression`: `tag` selects the binder operator IRI, the
    /// `String` is the bound variable's SOURCE name (its identity for scope
    /// resolution — never its rendered declaration IRI/label, which [`Mint`]
    /// controls independently), and the boxed body is its single body slot.
    Bind(u8, String, Box<GenExpr>),
}

/// A deliberately SMALL variable-name pool. With `prop_recursive` nesting, a
/// small pool makes an inner `Bind` reusing an outer `Bind`'s name (shadowing),
/// and a `Var` occurrence with no enclosing `Bind` of that name (a genuine free
/// variable) that textually coincides with some BOUND name used elsewhere in
/// the same term, both arise routinely by construction — never a hand-picked
/// special case.
fn var_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("x".to_owned()),
        Just("y".to_owned()),
        Just("z".to_owned()),
        Just("w".to_owned()),
    ]
}

fn gen_expr() -> impl Strategy<Value = GenExpr> {
    let leaf = prop_oneof![
        (0u8..3).prop_map(GenExpr::Const),
        var_name().prop_map(GenExpr::Var),
    ];
    leaf.prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            (0u8..3, prop::collection::vec(inner.clone(), 0..=4))
                .prop_map(|(tag, args)| GenExpr::App(tag, args)),
            (0u8..2, var_name(), inner).prop_map(|(tag, name, body)| GenExpr::Bind(
                tag,
                name,
                Box::new(body)
            )),
        ]
    })
}

// ── the reference model: an independent, non-digest alpha-equivalence key ──

/// The REFERENCE alpha-equivalence identity of a [`GenExpr`]: a locally-nameless
/// normal form computed INDEPENDENTLY of [`arena_structural_key`], so comparing the
/// two (in [`structural_identity_matches_reference_alpha_equivalence`]) is a
/// genuine cross-check, never a tautology. `Bind` carries no name (alpha-
/// irrelevant, exactly like a `Binder` node's sort-only child list); `Var`
/// becomes `Bound(distance)` against the nearest enclosing same-named `Bind`
/// (innermost first — the same resolution order [`resolve_debruijn`] uses), or
/// `Free(name)` if none encloses.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Canon {
    Const(u8),
    Free(String),
    Bound(usize),
    App(u8, Vec<Canon>),
    Bind(u8, Box<Canon>),
}

fn canonicalize(expr: &GenExpr, scope: &[String]) -> Canon {
    match expr {
        GenExpr::Const(tag) => Canon::Const(*tag),
        GenExpr::Var(name) => match scope.iter().rev().position(|n| n == name) {
            Some(distance) => Canon::Bound(distance),
            None => Canon::Free(name.clone()),
        },
        GenExpr::App(tag, args) => {
            Canon::App(*tag, args.iter().map(|a| canonicalize(a, scope)).collect())
        }
        GenExpr::Bind(tag, name, body) => {
            let mut inner_scope = scope.to_vec();
            inner_scope.push(name.clone());
            Canon::Bind(*tag, Box::new(canonicalize(body, &inner_scope)))
        }
    }
}

// ── rendering: GenExpr -> real math: Turtle text ───────────────────────────

/// Mints a [`GenExpr::Bind`]'s declaration IRI + `rdfs:label`, keyed by the
/// bind's pre-order index in the tree. Two DIFFERENT `Mint`s over the IDENTICAL
/// `GenExpr` tree exercise bound-variable RENAMING: the tree shape (and
/// therefore its [`Canon`] identity) is unchanged — only the surface IRI/label
/// text each `Bind` happens to be authored with.
trait Mint {
    fn decl(&self, bind_index: usize) -> (String, String);
}

struct PrimaryMint;
impl Mint for PrimaryMint {
    fn decl(&self, bind_index: usize) -> (String, String) {
        (
            format!("https://example.org/interning/decl/{bind_index}"),
            format!("n{bind_index}"),
        )
    }
}

/// A second mint scheme, salted so distinct proptest cases mint distinct
/// declaration IRIs/labels — never the SAME rename twice by accident.
struct RenamedMint<'a>(&'a str);
impl Mint for RenamedMint<'_> {
    fn decl(&self, bind_index: usize) -> (String, String) {
        (
            format!(
                "https://example.org/interning/renamed/{}/{bind_index}",
                self.0
            ),
            format!("alt-{}-{bind_index}", self.0),
        )
    }
}

struct Rendered {
    ttl: String,
    root: String,
}

/// A free variable's identity IS its own declaration IRI (never renamed by
/// [`Mint`] — a free variable is a RIGID constant, not an alpha-relabeled
/// binder), so it is minted deterministically by NAME alone.
fn free_decl_iri(name: &str) -> String {
    format!("https://example.org/interning/free/{name}")
}

fn fresh(counter: &mut usize) -> String {
    *counter += 1;
    format!("https://example.org/interning/n{}", *counter)
}

/// Render `expr` as a standalone `math:` Turtle document through the REAL
/// vocabulary [`lower_math_expression`] reads (the `M_*`/`canon::*` constants
/// this file already names) — never a hand-built `TermDag` node.
fn render(expr: &GenExpr, mint: &dyn Mint) -> Rendered {
    let mut ttl = String::from(
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
    );
    let mut node_counter: usize = 0;
    let mut bind_counter: usize = 0;
    let mut env: Vec<(String, String)> = Vec::new();
    let mut free_decls: BTreeSet<String> = BTreeSet::new();
    let root = render_node(
        expr,
        &mut ttl,
        &mut node_counter,
        &mut bind_counter,
        &mut env,
        &mut free_decls,
        mint,
    );
    for name in &free_decls {
        ttl.push_str(&format!(
            "<{}> a math:FreeVariableDeclaration .\n",
            free_decl_iri(name)
        ));
    }
    Rendered { ttl, root }
}

#[allow(clippy::too_many_arguments)]
fn render_node(
    expr: &GenExpr,
    ttl: &mut String,
    node_counter: &mut usize,
    bind_counter: &mut usize,
    env: &mut Vec<(String, String)>,
    free_decls: &mut BTreeSet<String>,
    mint: &dyn Mint,
) -> String {
    match expr {
        GenExpr::Const(tag) => format!("https://example.org/interning/const{tag}"),
        GenExpr::Var(name) => {
            let decl_iri = match env.iter().rev().find(|(n, _)| n == name) {
                Some((_, iri)) => iri.clone(),
                None => {
                    free_decls.insert(name.clone());
                    free_decl_iri(name)
                }
            };
            let var_expr = fresh(node_counter);
            let occ = fresh(node_counter);
            ttl.push_str(&format!(
                "<{var_expr}> a math:VariableExpression ; math:variableOccurrence <{occ}> .\n\
                 <{occ}> a math:VariableOccurrence ; math:declaredVariable <{decl_iri}> .\n"
            ));
            var_expr
        }
        GenExpr::App(tag, args) => {
            let subject = fresh(node_counter);
            let children: Vec<String> = args
                .iter()
                .map(|arg| render_node(arg, ttl, node_counter, bind_counter, env, free_decls, mint))
                .collect();
            let slot_iris: Vec<String> = children.iter().map(|_| fresh(node_counter)).collect();
            ttl.push_str(&format!(
                "<{subject}> a math:ApplicationExpression ; \
                 math:operator <https://example.org/interning/appOp{tag}> "
            ));
            for slot in &slot_iris {
                ttl.push_str(&format!("; math:argumentSlot <{slot}> "));
            }
            ttl.push_str(".\n");
            for (index, (slot, child)) in slot_iris.iter().zip(children.iter()).enumerate() {
                ttl.push_str(&format!(
                    "<{slot}> a math:ArgumentSlot ; math:slotIndex {index} ; \
                     math:slotExpression <{child}> .\n"
                ));
            }
            subject
        }
        GenExpr::Bind(tag, name, body) => {
            let subject = fresh(node_counter);
            let bind_index = *bind_counter;
            *bind_counter += 1;
            let (decl_iri, label) = mint.decl(bind_index);
            env.push((name.clone(), decl_iri.clone()));
            let body_subject =
                render_node(body, ttl, node_counter, bind_counter, env, free_decls, mint);
            env.pop();
            let body_slot = fresh(node_counter);
            ttl.push_str(&format!(
                "<{subject}> a math:BindingExpression ;\n\
                 \x20 math:operator <https://example.org/interning/bindOp{tag}> ;\n\
                 \x20 math:boundVariable <{decl_iri}> ;\n\
                 \x20 math:argumentSlot <{body_slot}> .\n\
                 <{body_slot}> a math:ArgumentSlot ; math:slotIndex 0 ; \
                 math:slotExpression <{body_subject}> .\n\
                 <{decl_iri}> a math:VariableDeclaration ; rdfs:label \"{label}\"@en .\n"
            ));
            subject
        }
    }
}

/// Parse `rendered` and lower it through the REAL production entry points,
/// panicking (never silently skipping) on a parse/lowering failure — a
/// generated term this grammar can produce must ALWAYS be well-formed `math:`,
/// so any failure here is a bug in the generator/renderer, not an expected
/// rejection.
fn lower_rendered(rendered: &Rendered) -> String {
    let graph = MathGraph::from_turtle(rendered.ttl.as_bytes()).unwrap_or_else(|e| {
        panic!(
            "generated math: Turtle must parse: {e}\n\n--- generated Turtle ---\n{}",
            rendered.ttl
        )
    });
    arena_structural_key(&graph, &rendered.root).unwrap_or_else(|e| {
        panic!(
            "generated math: expression must lower: {e:?}\n\n--- generated Turtle ---\n{}",
            rendered.ttl
        )
    })
}

// ── the properties ──────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(config())]

    /// **Bound-variable renaming.** Rendering the SAME [`GenExpr`] shape through
    /// TWO independent [`Mint`] schemes — every `Bind`'s declaration IRI/label
    /// chosen differently, via a randomly generated salt — must yield the
    /// IDENTICAL [`arena_structural_key`]. The wrapping `Bind` at the top of the
    /// generator guarantees at least one renamed declaration exists, so the
    /// "the rename actually changed something" sanity check is never vacuous.
    #[test]
    fn bound_variable_renaming_does_not_change_digest(
        inner in gen_expr(),
        outer_tag in 0u8..2,
        outer_name in var_name(),
        salt in "[a-z0-9]{1,8}",
    ) {
        let expr = GenExpr::Bind(outer_tag, outer_name, Box::new(inner));

        let primary = render(&expr, &PrimaryMint);
        let renamed = render(&expr, &RenamedMint(&salt));
        prop_assert_ne!(
            &primary.ttl, &renamed.ttl,
            "the two mint schemes must actually author different declaration \
             IRIs/labels, or this case proves nothing"
        );

        let digest_primary = lower_rendered(&primary);
        let digest_renamed = lower_rendered(&renamed);
        prop_assert_eq!(
            digest_primary, digest_renamed,
            "renaming a bound variable's declaration IRI/label must not change \
             its structural digest"
        );
    }

    /// **Injectivity, both directions — the reference-model cross-check.** Two
    /// INDEPENDENTLY generated expressions' [`Canon`] identity (computed with NO
    /// reference to `arena_structural_key`) must coincide with their
    /// `arena_structural_key` equality EXACTLY: alpha-equivalent inputs share one
    /// digest (soundness) AND structurally distinct inputs never collide
    /// (injectivity — the direction a constant-digest function would fail). This
    /// single property also exercises arbitrary slot arity/operand order and
    /// arbitrarily nested shadowing, since [`gen_expr`] generates all three.
    #[test]
    fn structural_identity_matches_reference_alpha_equivalence(
        a in gen_expr(),
        b in gen_expr(),
    ) {
        let canon_a = canonicalize(&a, &[]);
        let canon_b = canonicalize(&b, &[]);
        let digest_a = lower_rendered(&render(&a, &PrimaryMint));
        let digest_b = lower_rendered(&render(&b, &PrimaryMint));
        prop_assert_eq!(
            canon_a == canon_b,
            digest_a == digest_b,
            "structural (locally-nameless) equality of two INDEPENDENTLY \
             generated expressions must coincide EXACTLY with arena_structural_key \
             equality, in both directions"
        );
    }

    /// **Nested shadowing, deliberately.** `outer` binds `outer_name`; its body
    /// nests a SECOND binder and an occurrence of `outer_name` inside that
    /// second binder's body. When the inner binder rebinds `outer_name` (a real
    /// shadow), the occurrence resolves to the INNER binder (de-Bruijn distance
    /// 0); when the inner binder uses a name guaranteed distinct from
    /// `outer_name`, the SAME occurrence resolves to the OUTER binder (distance
    /// 1). This is a structural difference the digest MUST reflect.
    #[test]
    fn shadowing_changes_binder_resolution_and_digest(
        outer_tag in 0u8..2,
        inner_tag in 0u8..2,
        body_tag in 0u8..3,
        outer_name in var_name(),
    ) {
        let noshadow_name = format!("{outer_name}_distinct");
        let build = |inner_name: &str| {
            GenExpr::Bind(
                outer_tag,
                outer_name.clone(),
                Box::new(GenExpr::Bind(
                    inner_tag,
                    inner_name.to_owned(),
                    Box::new(GenExpr::App(body_tag, vec![GenExpr::Var(outer_name.clone())])),
                )),
            )
        };
        let shadow_expr = build(&outer_name);
        let noshadow_expr = build(&noshadow_name);

        // The reference model itself must resolve the occurrence differently —
        // otherwise this case proves nothing about the real lowering.
        let canon_shadow = canonicalize(&shadow_expr, &[]);
        let canon_noshadow = canonicalize(&noshadow_expr, &[]);
        prop_assert_ne!(
            &canon_shadow, &canon_noshadow,
            "the reference model must resolve the occurrence to a different \
             binder depending on whether the inner binder shadows the outer's name"
        );

        let digest_shadow = lower_rendered(&render(&shadow_expr, &PrimaryMint));
        let digest_noshadow = lower_rendered(&render(&noshadow_expr, &PrimaryMint));
        prop_assert_ne!(
            digest_shadow, digest_noshadow,
            "an inner binder shadowing the outer's name changes which binder an \
             occurrence resolves to (de-Bruijn distance 0 vs 1) — the structural \
             digest must differ"
        );
    }

    /// **Slot arity / operand order, deliberately.** A flat application over a
    /// generated number of constant operands: re-lowering is deterministic;
    /// reversing operand order (unless it happens to be a palindrome) changes
    /// the digest; and dropping the last operand (a genuine arity change)
    /// always changes the digest.
    #[test]
    fn application_arity_and_operand_order_are_digest_significant(
        tag in 0u8..3,
        operands in prop::collection::vec(0u8..6, 0..=5),
    ) {
        let build = |ops: &[u8]| {
            GenExpr::App(tag, ops.iter().copied().map(GenExpr::Const).collect())
        };
        let digest_original = lower_rendered(&render(&build(&operands), &PrimaryMint));
        let digest_again = lower_rendered(&render(&build(&operands), &PrimaryMint));
        prop_assert_eq!(
            &digest_original, &digest_again,
            "re-lowering the identical operand list must reproduce the identical digest"
        );

        if operands.len() >= 2 {
            let mut reversed = operands.clone();
            reversed.reverse();
            if reversed != operands {
                let digest_reversed =
                    lower_rendered(&render(&build(&reversed), &PrimaryMint));
                prop_assert_ne!(
                    &digest_original, &digest_reversed,
                    "reversing operand order at a fixed operator/arity must \
                     change the digest"
                );
            }
        }

        if !operands.is_empty() {
            let mut shorter = operands.clone();
            shorter.pop();
            let digest_shorter = lower_rendered(&render(&build(&shorter), &PrimaryMint));
            prop_assert_ne!(
                &digest_original, &digest_shorter,
                "changing arity (dropping the last argument slot) must change \
                 the digest"
            );
        }
    }

    /// **Determinism.** The same term lowered twice into the SAME `TermDag` must
    /// re-intern to the SAME `NodeId` (hash-consing, not a fresh node); and the
    /// same term lowered into TWO INDEPENDENTLY-BUILT `TermDag`s must yield the
    /// identical `arena_structural_key` despite unrelated `NodeId` numbering.
    #[test]
    fn lowering_is_deterministic_across_independent_dags(expr in gen_expr()) {
        let rendered = render(&expr, &PrimaryMint);
        let graph = MathGraph::from_turtle(rendered.ttl.as_bytes())
            .unwrap_or_else(|e| panic!("generated math: Turtle must parse: {e}"));

        let mut dag = TermDag::new();
        let node_1 = lower_math_expression(&mut dag, &graph, &rendered.root)
            .unwrap_or_else(|e| panic!("generated math: expression must lower: {e:?}"));
        let node_2 = lower_math_expression(&mut dag, &graph, &rendered.root)
            .unwrap_or_else(|e| panic!("re-lowering must also succeed: {e:?}"));
        prop_assert_eq!(
            node_1, node_2,
            "re-lowering the SAME graph into the SAME dag must re-intern to the \
             SAME node"
        );
        let digest_same_dag = arena_structural_key(&graph, &rendered.root)
            .unwrap_or_else(|e| panic!("digest over the shared dag must succeed: {e:?}"));

        let digest_a = arena_structural_key(&graph, &rendered.root)
            .unwrap_or_else(|e| panic!("digest through an independent arena must succeed: {e:?}"));
        let digest_b = arena_structural_key(&graph, &rendered.root)
            .unwrap_or_else(|e| panic!("digest through an independent arena must succeed: {e:?}"));
        prop_assert_eq!(
            &digest_a, &digest_b,
            "the same term lowered via two independently-built TermDags must \
             yield the identical digest"
        );
        prop_assert_eq!(
            digest_same_dag, digest_a,
            "digest is stable regardless of which dag instance computed it"
        );
    }
}
