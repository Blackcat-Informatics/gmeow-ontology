// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The R lift tier: a parsed script → `math:` structures.
//!
//! The lift map is `MATHEMATICS-BRIDGES.md`'s, discharged edge for edge:
//!
//! | R | `math:` |
//! |---|---|
//! | `y ~ x1 + x2` | `math:ModelFormula` (a `math:BindingExpression`) over indexed `math:ArgumentSlot`s, response at index 0 |
//! | `lm`/`glm`/`lmer` | `math:FittedModel` with `math:modelFormula` **and** `math:fittedToData` |
//! | `data = mtcars` | `math:DatasetMatrix`, **by reference** — named and framed, never inlined |
//! | `rnorm`/`dbinom`/… | `math:Distribution` with its family, parameterization, roles, and parameters |
//! | `coef()` / `summary()$coefficients` | `math:Estimate` with `math:estimatedParameter` and `math:estimator` |
//! | `residuals()` / `resid()` | `math:Residual` with `math:residualOf` |
//! | arithmetic / transforms | `math:ApplicationExpression` over `math:ArgumentSlot`s |
//! | control flow, general computation | `logic:` via `math:compilesToLogicFormula` |
//!
//! # The three OWL restrictions that make this a hard-fail bridge
//!
//! - `math:ModelFormula` carries **min 1** `math:argumentSlot` (`module.ttl` —
//!   `math:UnframedFunction`). A formula that expands to no slot is [`RUnliftable`].
//! - `math:FittedModel` carries **min 1** `math:modelFormula` **and min 1**
//!   `math:fittedToData` (`math:UnfittedModel`). A model call missing either is
//!   [`RUnliftable`]; neither is ever faked.
//! - `math:Estimate` carries `math:estimatesEstimand` **or** `math:estimatedParameter` (an
//!   `owl:unionOf` restriction) **and** names its `math:estimator`. This lift supplies the
//!   parameter arm and a real estimator (`lm` IS ordinary least squares), and deliberately
//!   does **not** claim a `math:Estimand`: an estimand is well-posed only with its six
//!   framing coordinates, none of which an R script states.
//!
//! # The `logic:` lowering trap
//!
//! `math:LogicLoweringDeclaredConstraint` fires `math:UndeclaredLogicLowering` on any
//! subject of `math:compilesToLogicFormula` that does not also declare
//! `math:denotationKind` and `math:logicLoweringPreservation`. `Lift::lower_to_logic` is
//! the ONLY place this crate emits that edge, and it emits all three together, so the
//! constraint cannot be violated by construction.
//!
//! # The `logic:` lowering itself
//!
//! `MATHEMATICS-BRIDGES.md` forbids a "degraded or string-valued placeholder" as its
//! flagship criterion, so a routed construct lowers into a REAL `logic:` AST — not an
//! opaque node tagged with the construct's name and labelled with a truncation. The
//! computation node is the anchor; every operand the parser recovered lives beneath it:
//!
//! | R | `logic:` |
//! |---|---|
//! | `if (c) t` | `logic:antecedent` = lowered `c`, `logic:consequent` = lowered `t` |
//! | `if (c) t else e` | `logic:and` of `c → t` and `¬c → e` (`logic:not` over the same condition node) |
//! | `for (v in s) b` | `logic:forall` binding `v`, body `r-for-in(v, s) → b` |
//! | `while (c) b` | `c → b` — one iteration's guard implying its body |
//! | `{ s1; s2 }` | `logic:and` over the lowered statements |
//! | `x && y` / `x \|\| y` / `!x` | `logic:and` / `logic:or` / `logic:not` |
//! | `x > y` | an atom on the `r-greater` relation over two term carriers |
//! | `f(a, b)` | an atom on the `r-call:f` relation over ordered `logic:TermCarrier`s |
//! | `f(g(x))` | `g(x)` rides as a `logic:FunctionTerm` on `logic:termApplication` |
//! | `function(p) b` | an atom on `r-function` over the formals and the body |
//!
//! Two properties are load-bearing. First, **the relation identity includes the callee**:
//! `library(stats)` and `set.seed(20260725)` predicate DIFFERENT reified `logic:Type`
//! individuals, so the predicate says which call it was. Second, **every node selects
//! exactly one constructor** (`logic:FormulaConstructorConstraint`) and meets the typed-IR
//! frontend's arity invariants (`crates/logic-compile/src/frontend.rs`): `logic:not` one
//! child, `logic:and`/`logic:or` two or more DISTINCT operands, an implication both halves,
//! a quantifier at least one bound variable, an atom at least one argument carrier.
//!
//! # The residue is enumerated, not merely declared
//!
//! The R rung is [`Rung::lossy_vague_with_witness`], and `math:unmappedConstruct`'s own
//! definition demands that the witness "let a consumer recover WHAT THE LIFT DID NOT MAP".
//! So each R semantics the lowering cannot carry — a loop's repetition, a block's
//! evaluation order, an assignment's store update, `NA`'s three-valued missingness, a
//! closure's environment, and each callee whose own behaviour is uninterpreted — is
//! recorded through [`RunFrame::record_unmapped`] and rides on the `math:parseSource`
//! witness. A script that loses nothing enumerates nothing, so the property is a claim
//! rather than a constant.
//!
//! # Content-addressed interning
//!
//! `MATHEMATICS-RUNTIME.md`'s acceptance bar #2 requires that identical normalized
//! subexpressions resolve to ONE interned node. Every lifted expression is interned into a
//! [`TermArena`], and the resulting [`ContentKey`] — not the
//! source text, not a counter — is what mints the codomain node's IRI. A script that
//! mentions `log(wt)` twice therefore produces one `math:ApplicationExpression`, and the
//! fact count grows with distinct structure rather than with textual repetition.
//!
//! # Blob-by-reference
//!
//! A `math:DatasetMatrix` is a NAME and a FRAME. `data.frame(x = 1:10)` lifts to a dataset
//! node carrying the binding's label; the column payload is never walked and never emitted.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_term_arena::{Arena, ContentKey, StructNode, TermArena};
use purrdf::TermValue;

use crate::error::{RUnliftable, SourceNotUtf8};
use crate::frame::{BridgeKind, Lifted, RunFrame, Rung};
use crate::ns::{XSD_BOOLEAN, XSD_DECIMAL, XSD_INTEGER, gmeow, logic, math};
use crate::r::parser::{
    Arg, BinaryOp, Formula, FormulaTerm, Param, RExpr, RScript, RStmt, RStmtKind, TermKind,
    UnaryOp, desugar_pipe, parse,
};
use crate::sink::Sink;

/// `rdfs:label`.
///
/// Declared here rather than in [`crate::ns`] because it is the one non-`math:`/`logic:`
/// term the R lift needs: a `math:DatasetMatrix` is held BY REFERENCE, so the R name that
/// frames it has to travel with the node or the reference addresses nothing. The literal is
/// PLAIN — [`Sink`] deliberately exposes no language-tagged constructor, because lifted
/// graphs leave through the shipped CLI where no `x-gmeow-*` private-use tag may appear.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `xsd:double` — the datatype an R numeric literal is actually carried in.
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// Half an ULP of `value` at IEEE-754 double precision: the exact absolute bound by which
/// the stored double may deviate from the decimal the source wrote.
///
/// This is a derived property of the representation, not a chosen tolerance. A subnormal or
/// non-finite input has no meaningful ULP, so it reports zero rather than a fabricated bound.
fn half_ulp(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let next = f64::from_bits(value.abs().to_bits() + 1);
    (next - value.abs()) / 2.0
}

/// Lift an R script into `math:` structures.
///
/// `mint_base` must end in `/` or `#`; every codomain IRI is minted beneath the run it
/// names, so a re-lift of the same bytes under the same base is byte-identical.
///
/// # Errors
///
/// - [`SourceNotUtf8`] when `source` is not valid UTF-8.
/// - [`RParse`](crate::error::RParse) when the script is not syntactically well-formed.
/// - [`RUnliftable`] when the script parses but carries no `math:` statistical content, or
///   when a model call cannot satisfy an OWL restriction the codomain declares.
pub fn lift(source: &[u8], mint_base: &str) -> gmeow_errors::Result<Lifted> {
    let text = std::str::from_utf8(source).map_err(|e| {
        gmeow_errors::Diag::of_kind(SourceNotUtf8 {
            detail: format!(
                "the R source is not valid UTF-8 (invalid byte sequence at offset {}): a text \
                 front-end cannot read it, and re-decoding it under a guessed encoding would be a \
                 degraded parse",
                e.valid_up_to()
            ),
        })
    })?;
    let script = parse(text)?;

    let mut frame = RunFrame::mint(BridgeKind::R, mint_base, source);

    // The lift runs FIRST, against an immutable borrow of the frame (every codomain IRI is
    // minted beneath it). Only afterwards is the frame itself written to — the residue the
    // lowering enumerated rides on the `math:parseSource` witness, and that witness is not
    // emitted until the walk that discovers the residue has finished. The sink canonicalizes
    // triple order, so emitting the frame last is byte-identical to emitting it first.
    let mut lift = Lift {
        frame: &frame,
        sink: Sink::new(),
        arena: TermArena::new(),
        emitted: BTreeSet::new(),
        env: BTreeMap::new(),
        unmapped: BTreeSet::new(),
        statistical: 0,
        lowerings: 0,
    };
    lift.script(&script)?;

    if lift.statistical == 0 {
        return Err(gmeow_errors::Diag::of_kind(RUnliftable {
            detail: format!(
                "the R script parses but carries no statistical content for the math: codomain: \
                 {} construct(s) routed to logic: as general computation and nothing lifted into \
                 math:. The R bridge lifts a script's MATHEMATICAL content; a script that is only \
                 control flow, I/O, and string handling is an unliftable ingest, not a lift",
                lift.lowerings
            ),
        }));
    }

    let Lift {
        mut sink,
        emitted,
        unmapped,
        ..
    } = lift;
    for construct in unmapped {
        frame.record_unmapped(construct);
    }
    frame.emit(&mut sink, Rung::lossy_vague_with_witness());

    Lifted::seal(&frame, sink, emitted.len())
}

// ── Lift state ────────────────────────────────────────────────────────────────

/// What an R name is bound to, once lifted.
#[derive(Debug, Clone)]
enum Binding {
    /// A `math:DatasetMatrix`, held by reference.
    Dataset(String),
    /// A fitted model and everything a later `coef()`/`residuals()` needs from it.
    Fit(FitInfo),
    /// A lifted `math:` expression node. The node's IRI is content-addressed on the
    /// expression, so a later mention re-derives it rather than carrying it here.
    Expression,
    /// A `math:Distribution`.
    Distribution,
    /// A bound name with no `math:` image (a string, a plot handle, a control-flow result).
    Opaque,
}

/// A fitted model, as much of it as the script determines.
#[derive(Debug, Clone)]
struct FitInfo {
    iri: String,
    key: String,
    /// The coefficient names `coef()` would return, in R's order.
    coefficients: Vec<String>,
    estimator: Estimator,
}

/// Which estimation procedure a model call names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Estimator {
    /// `lm` — ordinary least squares.
    OrdinaryLeastSquares,
    /// `gls` — generalized least squares.
    GeneralizedLeastSquares,
    /// `glm`, `lmer`, `glmer`, `lme` — maximum likelihood.
    MaximumLikelihood,
    /// `nls` — nonlinear least squares.
    NonlinearLeastSquares,
}

impl Estimator {
    fn slug(self) -> &'static str {
        match self {
            Self::OrdinaryLeastSquares => "ordinary-least-squares",
            Self::GeneralizedLeastSquares => "generalized-least-squares",
            Self::MaximumLikelihood => "maximum-likelihood",
            Self::NonlinearLeastSquares => "nonlinear-least-squares",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::OrdinaryLeastSquares => "ordinary least squares",
            Self::GeneralizedLeastSquares => "generalized least squares",
            Self::MaximumLikelihood => "maximum likelihood",
            Self::NonlinearLeastSquares => "nonlinear least squares",
        }
    }
}

/// A lifted expression: its codomain IRI and its arena node.
#[derive(Debug, Clone, Copy)]
struct ExprRef {
    node: StructNode,
}

struct Lift<'f> {
    frame: &'f RunFrame,
    sink: Sink,
    arena: TermArena,
    emitted: BTreeSet<String>,
    env: BTreeMap<String, Binding>,
    /// R semantics the lowering did NOT carry, collected during the walk and replayed onto
    /// the frame's `math:parseSource` witness as `math:unmappedConstruct`. A `BTreeSet`
    /// because the residue is a SET — the same construct met twice is one loss — and
    /// because a sorted set keeps a re-lift byte-identical.
    unmapped: BTreeSet<String>,
    statistical: usize,
    lowerings: usize,
}

impl Lift<'_> {
    /// Mint (and back-link) a codomain node, reporting whether it is new.
    ///
    /// The back edge `gmeow:wasGeneratedBy` is what the native `math:UnliftableIngest` lint
    /// enumerates, so it is attached HERE, once, for every node this lift creates — never
    /// left to a caller to remember.
    fn mint(&mut self, role: &str, key: &str) -> (String, bool) {
        let iri = self.frame.node(role, key);
        let fresh = self.emitted.insert(iri.clone());
        if fresh {
            self.frame.generated(&mut self.sink, &iri);
        }
        (iri, fresh)
    }

    fn label(&mut self, subject: &str, text: &str) {
        self.sink.string(subject, RDFS_LABEL, text);
    }

    fn key_of(&self, node: StructNode) -> ContentKey {
        self.arena
            .key(node)
            .expect("every node was minted by this lift's own arena")
    }

    fn atom(&mut self, text: &str) -> StructNode {
        self.arena.intern_leaf(TermValue::simple_literal(text))
    }

    fn free_atom(&mut self, text: &str) -> StructNode {
        self.arena.intern_free(TermValue::simple_literal(text))
    }

    fn app(&mut self, operator: &str, args: &[StructNode]) -> StructNode {
        let op = self.atom(operator);
        self.arena
            .intern_app(op, args)
            .expect("every node was minted by this lift's own arena")
    }

    // -- statements ----------------------------------------------------------

    fn script(&mut self, script: &RScript) -> gmeow_errors::Result<()> {
        for stmt in &script.statements {
            self.statement(stmt)?;
        }
        Ok(())
    }

    fn statement(&mut self, stmt: &RStmt) -> gmeow_errors::Result<()> {
        match &stmt.kind {
            RStmtKind::Assign { target, value, .. } => {
                let binding = self.value(value)?;
                if let RExpr::Ident(name) = target.unparenthesized() {
                    self.env.insert(name.clone(), binding);
                }
                Ok(())
            }
            RStmtKind::Expr(expr) => {
                self.value(expr)?;
                Ok(())
            }
        }
    }

    /// Lift one expression in statement position and report what it binds to.
    fn value(&mut self, expr: &RExpr) -> gmeow_errors::Result<Binding> {
        let expr = expr.unparenthesized();
        if expr.is_control_flow() {
            self.lower_to_logic(expr);
            return Ok(Binding::Opaque);
        }
        match expr {
            RExpr::Pipe { lhs, rhs, native } => {
                let desugared = desugar_pipe(lhs, rhs, *native);
                self.value(&desugared)
            }
            RExpr::Assign { target, value, .. } => {
                let binding = self.value(value)?;
                if let RExpr::Ident(name) = target.unparenthesized() {
                    self.env.insert(name.clone(), binding.clone());
                }
                Ok(binding)
            }
            RExpr::Call { callee, args } => self.call(callee, args),
            RExpr::Formula(formula) => {
                self.emit_formula(formula)?;
                Ok(Binding::Expression)
            }
            RExpr::Component { .. } => {
                if let Some(binding) = self.component(expr)? {
                    return Ok(binding);
                }
                self.lower_to_logic(expr);
                Ok(Binding::Opaque)
            }
            RExpr::Ident(name) => Ok(self.env.get(name).cloned().unwrap_or(Binding::Opaque)),
            RExpr::Number { .. }
            | RExpr::Str(_)
            | RExpr::Logical(_)
            | RExpr::Null
            | RExpr::Na
            | RExpr::NotANumber
            | RExpr::Infinity
            | RExpr::Namespace { .. } => Ok(Binding::Opaque),
            other => {
                if self.liftable_as_math(other) {
                    self.emit_expression(other)?;
                    return Ok(Binding::Expression);
                }
                self.lower_to_logic(other);
                Ok(Binding::Opaque)
            }
        }
    }

    // -- calls ---------------------------------------------------------------

    fn call(&mut self, callee: &RExpr, args: &[Arg]) -> gmeow_errors::Result<Binding> {
        let Some(name) = callable_name(callee) else {
            self.lower_to_logic(&RExpr::Call {
                callee: Box::new(callee.clone()),
                args: args.to_vec(),
            });
            return Ok(Binding::Opaque);
        };

        if let Some(estimator) = model_estimator(&name) {
            return self.emit_fit(&name, estimator, args);
        }
        if DATASET_FUNCTIONS.contains(&name.as_str()) {
            let label = render(&RExpr::Call {
                callee: Box::new(callee.clone()),
                args: args.to_vec(),
            });
            let iri = self.emit_dataset(&label, &label);
            return Ok(Binding::Dataset(iri));
        }
        if let Some(family) = distribution_family(&name) {
            self.emit_distribution(family, args)?;
            return Ok(Binding::Distribution);
        }
        if ESTIMATE_FUNCTIONS.contains(&name.as_str())
            && let Some(fit) = self.resolve_fit(args.first().and_then(|a| a.value.as_ref()))
        {
            self.emit_estimates(&fit);
            return Ok(Binding::Opaque);
        }
        if RESIDUAL_FUNCTIONS.contains(&name.as_str())
            && let Some(fit) = self.resolve_fit(args.first().and_then(|a| a.value.as_ref()))
        {
            self.emit_residual(&fit);
            return Ok(Binding::Opaque);
        }
        if SUMMARY_FUNCTIONS.contains(&name.as_str())
            && let Some(fit) = self.resolve_fit(args.first().and_then(|a| a.value.as_ref()))
        {
            self.emit_summary_observation(&fit);
            return Ok(Binding::Opaque);
        }

        let call = RExpr::Call {
            callee: Box::new(callee.clone()),
            args: args.to_vec(),
        };
        if self.liftable_as_math(&call) {
            self.emit_expression(&call)?;
            return Ok(Binding::Expression);
        }
        self.lower_to_logic(&call);
        Ok(Binding::Opaque)
    }

    /// `fit$residuals`, `fit$coefficients`, `summary(fit)$coefficients`, `d$column`.
    fn component(&mut self, expr: &RExpr) -> gmeow_errors::Result<Option<Binding>> {
        let RExpr::Component { object, name, slot } = expr.unparenthesized() else {
            return Ok(None);
        };
        if !*slot && let Some(fit) = self.resolve_fit(Some(object)) {
            match name.as_str() {
                "coefficients" | "coef" => {
                    self.emit_estimates(&fit);
                    return Ok(Some(Binding::Opaque));
                }
                "residuals" | "resid" => {
                    self.emit_residual(&fit);
                    return Ok(Some(Binding::Opaque));
                }
                _ => {}
            }
        }
        if self.liftable_as_math(expr) {
            self.emit_expression(expr)?;
            return Ok(Some(Binding::Expression));
        }
        Ok(None)
    }

    /// Follow `fit`, `summary(fit)`, `broom::tidy(fit)`, `(fit)` back to a fitted model.
    fn resolve_fit(&self, expr: Option<&RExpr>) -> Option<FitInfo> {
        let expr = expr?.unparenthesized();
        match expr {
            RExpr::Ident(name) => match self.env.get(name) {
                Some(Binding::Fit(fit)) => Some(fit.clone()),
                _ => None,
            },
            RExpr::Call { callee, args } => {
                let name = callable_name(callee)?;
                if SUMMARY_FUNCTIONS.contains(&name.as_str())
                    || ESTIMATE_FUNCTIONS.contains(&name.as_str())
                    || RESIDUAL_FUNCTIONS.contains(&name.as_str())
                {
                    return self.resolve_fit(args.first().and_then(|a| a.value.as_ref()));
                }
                None
            }
            RExpr::Component { object, .. } => self.resolve_fit(Some(object)),
            _ => None,
        }
    }

    // -- the fitted model ----------------------------------------------------

    fn emit_fit(
        &mut self,
        function: &str,
        estimator: Estimator,
        args: &[Arg],
    ) -> gmeow_errors::Result<Binding> {
        let formula = named_or_positional(args, "formula", 0)
            .and_then(|e| match e.unparenthesized() {
                RExpr::Formula(f) => Some(f.as_ref().clone()),
                _ => None,
            })
            .ok_or_else(|| {
                unliftable(format!(
                    "`{function}(…)` carries no model formula, so the math:FittedModel it would \
                     produce cannot satisfy the min-1 math:modelFormula restriction; a fitted \
                     model without its specification is math:UnfittedModel, and this lift will \
                     not invent one"
                ))
            })?;

        let Some(data) = named_or_positional(args, "data", 1) else {
            return Err(unliftable(format!(
                "`{function}(…)` names no data to fit to, so the math:FittedModel it would \
                 produce cannot satisfy the min-1 math:fittedToData restriction (a min-1 OWL \
                 restriction onClass math:DatasetMatrix); a fitted model without its data \
                 binding is math:UnfittedModel"
            )));
        };
        let Some(dataset) = self.dataset_reference(data) else {
            return Err(unliftable(format!(
                "the `data =` argument of `{function}(…)` is `{}`, which names no dataset this \
                 lift can hold by reference; inlining it, or emitting a string placeholder for \
                 it, is forbidden by the blob-by-reference doctrine",
                render(data)
            )));
        };

        let formula_node = self.emit_formula(&formula)?;
        let (formula_iri, formula_key) = self.formula_iri(formula_node);

        let fit_key = format!("fit|{function}|{formula_key}|{dataset}");
        let (fit_iri, fresh) = self.mint("fit", &fit_key);
        if fresh {
            self.sink.typed(&fit_iri, &math("FittedModel"));
            self.sink.iri(&fit_iri, &math("modelFormula"), &formula_iri);
            self.sink.iri(&fit_iri, &math("fittedToData"), &dataset);
            self.label(
                &fit_iri,
                &format!("{function}({})", render_formula(&formula)),
            );
            self.statistical += 1;
        }

        // A GLM's `family =` names a real distribution the script states; it lifts to a
        // math:Distribution with its family (non-parametric — the family argument supplies
        // no parameters, and inventing them would be fabrication).
        if let Some(family_arg) = named_argument(args, "family")
            && let Some(name) = family_link_name(family_arg)
            && let Some(family) = distribution_family(&format!("d{name}"))
        {
            self.emit_family_only_distribution(family);
        }

        Ok(Binding::Fit(FitInfo {
            iri: fit_iri,
            key: fit_key,
            coefficients: coefficient_names(&formula),
            estimator,
        }))
    }

    /// Resolve a `data =` argument to a `math:DatasetMatrix` IRI, minting one if the name
    /// is an unbound R dataset (`mtcars`, `iris`, a package dataset).
    fn dataset_reference(&mut self, expr: &RExpr) -> Option<String> {
        match expr.unparenthesized() {
            RExpr::Ident(name) => match self.env.get(name) {
                Some(Binding::Dataset(iri)) => Some(iri.clone()),
                Some(Binding::Fit(_) | Binding::Expression) => None,
                _ => Some(self.emit_dataset(name, name)),
            },
            RExpr::Str(path) => Some(self.emit_dataset(path, path)),
            call @ RExpr::Call { callee, .. } => {
                let name = callable_name(callee)?;
                if !DATASET_FUNCTIONS.contains(&name.as_str()) {
                    return None;
                }
                let label = render(call);
                Some(self.emit_dataset(&label, &label))
            }
            RExpr::Namespace { package, name, .. } => {
                let label = format!("{package}::{name}");
                Some(self.emit_dataset(&label, &label))
            }
            _ => None,
        }
    }

    /// A `math:DatasetMatrix` held BY REFERENCE: the node names and frames the data, and
    /// no column payload is ever walked.
    fn emit_dataset(&mut self, key: &str, label: &str) -> String {
        let (iri, fresh) = self.mint("data", key);
        if fresh {
            self.sink.typed(&iri, &math("DatasetMatrix"));
            self.label(&iri, label);
            self.statistical += 1;
        }
        iri
    }

    // -- the model formula ---------------------------------------------------

    fn formula_iri(&mut self, node: StructNode) -> (String, String) {
        let key = self.key_of(node).into_string();
        (self.frame.node("formula", &key), key)
    }

    fn expression_iri(&mut self, node: StructNode) -> String {
        let key = self.key_of(node).into_string();
        self.frame.node("expr", &key)
    }

    /// Emit a `math:ModelFormula`: the `~` as a real binder over indexed slots.
    fn emit_formula(&mut self, formula: &Formula) -> gmeow_errors::Result<StructNode> {
        let mut slots: Vec<(StructNode, String)> = Vec::new();

        if let Some(response) = &formula.response {
            if !self.liftable_as_math(response) {
                return Err(unliftable(format!(
                    "the response `{}` of model formula `{}` has no math: image, and a formula \
                     response is never carried as an opaque string",
                    render(response),
                    render_formula(formula)
                )));
            }
            let node = self.emit_expression(response)?;
            let iri = self.expression_iri(node.node);
            slots.push((node.node, iri));
        }

        for term in &formula.terms {
            let (node, iri) = self.emit_term(term, formula)?;
            slots.push((node, iri));
        }

        if !formula.intercept {
            // `- 1` / `+ 0`: the suppression is recorded structurally as an explicit zero
            // intercept slot rather than silently dropped.
            let node = self.emit_number("0.0", false);
            let iri = self.expression_iri(node.node);
            slots.push((node.node, iri));
        }

        if slots.is_empty() {
            return Err(unliftable(format!(
                "model formula `{}` expands to no argument slot, but math:ModelFormula carries a \
                 min-1 math:argumentSlot OWL restriction (math:UnframedFunction); a formula that \
                 binds nothing is not a formula",
                render_formula(formula)
            )));
        }

        let tilde = self.atom("r:~");
        let observation_sort = self.atom("r:observation");
        let slot_nodes: Vec<StructNode> = slots.iter().map(|(n, _)| *n).collect();
        let body = self.app("r:formula-terms", &slot_nodes);
        let node = self
            .arena
            .intern_binder(tilde, &[observation_sort], body)
            .expect("every node was minted by this lift's own arena");

        let (formula_iri, formula_key) = self.formula_iri(node);
        if self.emitted.insert(formula_iri.clone()) {
            self.frame.generated(&mut self.sink, &formula_iri);
            self.sink.typed(&formula_iri, &math("ModelFormula"));
            // The ~ is a BINDER: it names its binder operator, its bound variable (the
            // observation index the terms are evaluated at), and its body through indexed
            // slots. MATHEMATICS-BRIDGES.md: "the ~ is a binder, not a string".
            self.sink.typed(&formula_iri, &math("BindingExpression"));
            let operator = self.emit_operation("tilde", "~");
            self.sink.iri(&formula_iri, &math("operator"), &operator);
            let (bound, bound_fresh) = self.mint("bound", &formula_key);
            if bound_fresh {
                self.sink.typed(&bound, &math("VariableDeclaration"));
                self.label(&bound, "observation index bound by ~");
            }
            self.sink.iri(&formula_iri, &math("boundVariable"), &bound);
            self.label(&formula_iri, &render_formula(formula));

            for (index, (_, expr_iri)) in slots.iter().enumerate() {
                let slot_key = format!("{formula_key}#{index}");
                let (slot_iri, _) = self.mint("slot", &slot_key);
                self.sink.typed(&slot_iri, &math("ArgumentSlot"));
                let position = i64::try_from(index).unwrap_or(i64::MAX);
                self.sink.integer(&slot_iri, &math("slotIndex"), position);
                self.sink.iri(&slot_iri, &math("slotExpression"), expr_iri);
                self.sink
                    .iri(&formula_iri, &math("argumentSlot"), &slot_iri);
            }
            self.statistical += 1;
        }
        Ok(node)
    }

    fn emit_term(
        &mut self,
        term: &FormulaTerm,
        formula: &Formula,
    ) -> gmeow_errors::Result<(StructNode, String)> {
        match term.kind {
            TermKind::Main | TermKind::Transform => {
                let factor = &term.factors[0];
                if !self.liftable_as_math(factor) {
                    return Err(unliftable(format!(
                        "formula term `{}` has no math: image; a term that would survive only as \
                         an opaque string is an unliftable ingest",
                        render(factor)
                    )));
                }
                let node = self.emit_expression(factor)?;
                let iri = self.expression_iri(node.node);
                Ok((node.node, iri))
            }
            TermKind::Interaction => {
                let mut operands = Vec::with_capacity(term.factors.len());
                for factor in &term.factors {
                    if !self.liftable_as_math(factor) {
                        return Err(unliftable(format!(
                            "interaction factor `{}` has no math: image",
                            render(factor)
                        )));
                    }
                    let node = self.emit_expression(factor)?;
                    operands.push(node.node);
                }
                let node = self.app("r:interaction", &operands);
                let iri = self.emit_application(node, "interaction", ":", &operands);
                Ok((node, iri))
            }
            TermKind::Dot => {
                // `.` is "every remaining column": an operator over the removed terms, not
                // a variable and certainly not a string. `y ~ . - x3` therefore lifts as
                // `exclude(expand-all, x3)`.
                let mut operands = Vec::new();
                for removed in &formula.removed {
                    for factor in &removed.factors {
                        if !self.liftable_as_math(factor) {
                            return Err(unliftable(format!(
                                "the removed formula term `{}` has no math: image",
                                render(factor)
                            )));
                        }
                        let node = self.emit_expression(factor)?;
                        operands.push(node.node);
                    }
                }
                let node = self.app("r:dot-expansion", &operands);
                let iri = self.emit_application(node, "dot-expansion", ".", &operands);
                Ok((node, iri))
            }
        }
    }

    // -- expressions ---------------------------------------------------------

    /// Whether `expr` has a `math:` image at all.
    ///
    /// A pure predicate, checked BEFORE any emission, so a partially-lifted expression
    /// never leaves orphan nodes behind when a subterm turns out to be unliftable.
    fn liftable_as_math(&self, expr: &RExpr) -> bool {
        match expr.unparenthesized() {
            RExpr::Number { .. } | RExpr::Ident(_) => true,
            RExpr::Component { slot: false, .. } => true,
            RExpr::Unary {
                op: UnaryOp::Plus | UnaryOp::Negate,
                operand,
            } => self.liftable_as_math(operand),
            RExpr::Binary { op, lhs, rhs } => {
                (op.is_arithmetic() || *op == BinaryOp::Sequence)
                    && self.liftable_as_math(lhs)
                    && self.liftable_as_math(rhs)
            }
            RExpr::Call { callee, args } => {
                let Some(name) = callable_name(callee) else {
                    return false;
                };
                if !MATH_FUNCTIONS.iter().any(|(f, _)| *f == name) {
                    return false;
                }
                !args.is_empty()
                    && args
                        .iter()
                        .all(|a| a.value.as_ref().is_some_and(|v| self.liftable_as_math(v)))
            }
            _ => false,
        }
    }

    /// Emit an expression's `math:` AST, interning it as it goes.
    fn emit_expression(&mut self, expr: &RExpr) -> gmeow_errors::Result<ExprRef> {
        let expr = expr.unparenthesized();
        match expr {
            RExpr::Number { text, integer, .. } => Ok(self.emit_number(text, *integer)),
            RExpr::Ident(name) => Ok(self.emit_variable(name)),
            RExpr::Component { object, name, .. } => {
                let label = format!("{}${name}", render(object));
                Ok(self.emit_variable(&label))
            }
            RExpr::Unary {
                op: UnaryOp::Plus,
                operand,
            } => self.emit_expression(operand),
            RExpr::Unary {
                op: UnaryOp::Negate,
                operand,
            } => {
                let inner = self.emit_expression(operand)?;
                let node = self.app("r:negate", &[inner.node]);
                self.emit_application(node, "negate", "-", &[inner.node]);
                Ok(ExprRef { node })
            }
            RExpr::Binary { op, lhs, rhs } => {
                let l = self.emit_expression(lhs)?;
                let r = self.emit_expression(rhs)?;
                let operands = [l.node, r.node];
                let slug = binary_operator_slug(*op);
                let node = self.app(&format!("r:{slug}"), &operands);
                self.emit_application(node, slug, op.spelling(), &operands);
                Ok(ExprRef { node })
            }
            RExpr::Call { callee, args } => {
                let name = callable_name(callee).unwrap_or_default();
                let mut operands = Vec::with_capacity(args.len());
                for arg in args {
                    let Some(value) = &arg.value else {
                        return Err(unliftable(format!(
                            "`{name}(…)` has an empty argument, which has no math: image"
                        )));
                    };
                    let lifted = self.emit_expression(value)?;
                    operands.push(lifted.node);
                }
                if name == "I" && operands.len() == 1 {
                    // `I(x^2)` is R's "as is" guard: it suppresses the formula term
                    // algebra and denotes exactly its argument.
                    return Ok(ExprRef { node: operands[0] });
                }
                let slug = MATH_FUNCTIONS
                    .iter()
                    .find(|(f, _)| *f == name)
                    .map_or(name.as_str(), |(_, slug)| *slug)
                    .to_owned();
                let node = self.app(&format!("r:{slug}"), &operands);
                self.emit_application(node, &slug, &name, &operands);
                Ok(ExprRef { node })
            }
            other => Err(unliftable(format!(
                "`{}` has no math: image; emitting it as a string placeholder is forbidden",
                render(other)
            ))),
        }
    }

    fn emit_number(&mut self, text: &str, integer: bool) -> ExprRef {
        let node = self
            .arena
            .intern_leaf(TermValue::simple_literal(format!("r:num:{text}:{integer}")));
        let key = self.key_of(node).into_string();
        let (iri, fresh) = self.mint("expr", &key);
        if fresh {
            self.sink.typed(&iri, &math("NumberLiteral"));
            let (number, _) = self.mint("number", &key);
            self.sink.typed(&number, &math("Number"));
            self.sink
                .iri(&number, &math("inNumberSystem"), &math("realNumbers"));
            self.sink.boolean(&number, &math("isExact"), true);
            self.sink.iri(&iri, &math("literalValue"), &number);
            // R's numeric vector is IEEE-754: the source decimal is the approximation that
            // stands in for the exact number, which is precisely math:ApproximateValue's job.
            let (approx, _) = self.mint("approx", &key);
            self.sink.typed(&approx, &math("ApproximateValue"));
            self.sink.iri(&approx, &math("approximates"), &number);
            if let Ok(value) = text.parse::<f64>() {
                self.sink.decimal(&approx, &math("quantityValue"), value);
                // math:ApproximateValue carries a min-1 math:approximationError: an
                // approximation that does not say how far it may deviate is not an
                // approximation, it is an unlabelled number. R numerics ARE IEEE-754
                // doubles, so the bound is exactly half an ULP at this magnitude — a
                // derived fact about the representation, not an invented tolerance.
                self.sink
                    .decimal(&approx, &math("approximationError"), half_ulp(value));
                // …and the datatype that bound is relative to.
                self.sink.iri(&approx, &math("numericDatatype"), XSD_DOUBLE);
            }
            self.label(&iri, text);
        }
        ExprRef { node }
    }

    fn emit_variable(&mut self, name: &str) -> ExprRef {
        let node = self.free_atom(&format!("r:var:{name}"));
        let key = self.key_of(node).into_string();
        let (iri, fresh) = self.mint("expr", &key);
        if fresh {
            self.sink.typed(&iri, &math("VariableExpression"));
            self.label(&iri, name);
            // math:VariableExpression's definition: it stands for one math:VariableOccurrence,
            // and "there is no implicit free variable" — an occurrence that resolves to no
            // declaration is math:UnscopedVariableOccurrence. Both are emitted.
            let (occurrence, _) = self.mint("occurrence", &key);
            self.sink.typed(&occurrence, &math("VariableOccurrence"));
            let (declaration, _) = self.mint("declaration", &key);
            self.sink
                .typed(&declaration, &math("FreeVariableDeclaration"));
            self.label(&declaration, name);
            self.sink
                .iri(&occurrence, &math("declaredVariable"), &declaration);
            self.sink
                .iri(&iri, &math("variableOccurrence"), &occurrence);
        }
        ExprRef { node }
    }

    /// Emit a `math:ApplicationExpression`: exactly one operator, contiguous zero-based
    /// slots.
    fn emit_application(
        &mut self,
        node: StructNode,
        operator_slug: &str,
        operator_label: &str,
        operands: &[StructNode],
    ) -> String {
        let key = self.key_of(node).into_string();
        let (iri, fresh) = self.mint("expr", &key);
        if !fresh {
            return iri;
        }
        self.sink.typed(&iri, &math("ApplicationExpression"));
        let operator = self.emit_operation(operator_slug, operator_label);
        self.sink.iri(&iri, &math("operator"), &operator);
        for (index, operand) in operands.iter().enumerate() {
            let operand_iri = self.expression_iri(*operand);
            let slot_key = format!("{key}#{index}");
            let (slot_iri, _) = self.mint("slot", &slot_key);
            self.sink.typed(&slot_iri, &math("ArgumentSlot"));
            let position = i64::try_from(index).unwrap_or(i64::MAX);
            self.sink.integer(&slot_iri, &math("slotIndex"), position);
            self.sink
                .iri(&slot_iri, &math("slotExpression"), &operand_iri);
            self.sink.iri(&iri, &math("argumentSlot"), &slot_iri);
        }
        self.statistical += 1;
        iri
    }

    /// The operator individual an application applies.
    ///
    /// The five arithmetic operators, `sqrt`, and `log` resolve to the `math:` named
    /// individuals the slice already declares; everything else mints a `math:Operation`
    /// (NOT a `math:ArithmeticOperation`, whose exactly-one signature restriction this lift
    /// has no grounds to fill).
    fn emit_operation(&mut self, slug: &str, label: &str) -> String {
        if let Some(named) = NAMED_OPERATIONS.iter().find(|(s, _)| *s == slug) {
            return math(named.1);
        }
        let (iri, fresh) = self.mint("operation", slug);
        if fresh {
            self.sink.typed(&iri, &math("Operation"));
            self.label(&iri, label);
        }
        iri
    }

    // -- estimates, residuals, observations -----------------------------------

    /// One `math:Estimate` per model coefficient.
    fn emit_estimates(&mut self, fit: &FitInfo) {
        let estimator = self.emit_estimator(fit.estimator);
        for coefficient in &fit.coefficients {
            let key = format!("{}|{coefficient}", fit.key);
            let (parameter, parameter_fresh) = self.mint("parameter", &key);
            if parameter_fresh {
                self.sink.typed(&parameter, &math("MathematicalObject"));
                self.label(&parameter, coefficient);
            }
            let (estimate, fresh) = self.mint("estimate", &key);
            if fresh {
                self.sink.typed(&estimate, &math("Estimate"));
                // math:Estimate rdfs:subClassOf owl:unionOf(estimatesEstimand,
                // estimatedParameter) AND names its estimator. The parameter arm is what an
                // R script determines; an estimand needs six framing coordinates the script
                // never states, so claiming one would be fabrication.
                self.sink
                    .iri(&estimate, &math("estimatedParameter"), &parameter);
                self.sink.iri(&estimate, &math("estimator"), &estimator);
                self.label(&estimate, &format!("estimate of {coefficient}"));
                self.statistical += 1;
            }
        }
    }

    fn emit_estimator(&mut self, estimator: Estimator) -> String {
        let (iri, fresh) = self.mint("estimator", estimator.slug());
        if fresh {
            self.sink.typed(&iri, &math("Estimator"));
            self.label(&iri, estimator.label());
        }
        iri
    }

    fn emit_residual(&mut self, fit: &FitInfo) {
        let (iri, fresh) = self.mint("residual", &fit.key);
        if fresh {
            self.sink.typed(&iri, &math("Residual"));
            self.sink.iri(&iri, &math("residualOf"), &fit.iri);
            self.statistical += 1;
        }
    }

    /// `summary(fit)` is a READ of the fit: a vantage-grounded `gmeow:Observation`, never
    /// an intrinsic property of the model.
    fn emit_summary_observation(&mut self, fit: &FitInfo) {
        let (observation, fresh) = self.mint("observation", &fit.key);
        if !fresh {
            return;
        }
        self.sink.typed(&observation, &gmeow("Observation"));
        self.sink
            .iri(&observation, &gmeow("observedFeature"), &fit.iri);
        let (result, _) = self.mint("summary", &fit.key);
        self.sink.typed(&result, &math("MathematicalObject"));
        self.sink
            .iri(&observation, &gmeow("observationResult"), &result);
        let (vantage, vantage_fresh) = self.mint("vantage", "r-engine");
        if vantage_fresh {
            self.sink.typed(&vantage, &gmeow("Standpoint"));
            self.label(&vantage, "the R evaluator");
        }
        self.sink.iri(&observation, &gmeow("vantage"), &vantage);
        self.statistical += 1;
    }

    // -- distributions -------------------------------------------------------

    fn emit_family_only_distribution(&mut self, family: &'static DistributionSpec) {
        let key = format!("dist|{}|family-only", family.family);
        let (iri, fresh) = self.mint("distribution", &key);
        if !fresh {
            return;
        }
        let family_iri = self.emit_distribution_family(family);
        self.sink.typed(&iri, &math("Distribution"));
        self.sink
            .iri(&iri, &math("distributionFamily"), &family_iri);
        self.label(&iri, family.family);
        self.statistical += 1;
    }

    fn emit_distribution_family(&mut self, family: &'static DistributionSpec) -> String {
        let (iri, fresh) = self.mint("family", family.family);
        if fresh {
            self.sink.typed(&iri, &math("DistributionFamily"));
            self.label(&iri, family.family);
        }
        iri
    }

    fn emit_distribution(
        &mut self,
        spec: &'static DistributionSpec,
        args: &[Arg],
    ) -> gmeow_errors::Result<String> {
        // The first positional argument of `rnorm(n, …)` / `dnorm(x, …)` is the sample size
        // or the evaluation point, never a distribution parameter.
        let positional: Vec<&Arg> = args.iter().filter(|a| a.name.is_none()).collect();
        let mut supplied: Vec<Option<&RExpr>> = Vec::with_capacity(spec.roles.len());
        for (index, role) in spec.roles.iter().enumerate() {
            let by_name = args
                .iter()
                .find(|a| a.name.as_deref() == Some(role.name))
                .and_then(|a| a.value.as_ref());
            let by_position = positional.get(index + 1).and_then(|a| a.value.as_ref());
            supplied.push(by_name.or(by_position));
        }

        let mut resolved: Vec<(usize, ParameterValue)> = Vec::with_capacity(spec.roles.len());
        for (index, role) in spec.roles.iter().enumerate() {
            match supplied[index] {
                Some(expr) => match expr.unparenthesized() {
                    RExpr::Number { text, .. } => {
                        resolved.push((index, ParameterValue::Literal(text.clone())));
                    }
                    RExpr::Unary {
                        op: UnaryOp::Negate,
                        operand,
                    } if matches!(operand.unparenthesized(), RExpr::Number { .. }) => {
                        let RExpr::Number { text, .. } = operand.unparenthesized() else {
                            unreachable!("guarded by the match arm")
                        };
                        resolved.push((index, ParameterValue::Literal(format!("-{text}"))));
                    }
                    other if self.liftable_as_math(other) => {
                        resolved.push((index, ParameterValue::Symbolic(other.clone())));
                    }
                    other => {
                        return Err(unliftable(format!(
                            "the `{}` parameter of `{}{}(…)` is `{}`, which has no math: image; a \
                             distribution parameter is a math:Quantity or a math:MathematicalExpression, \
                             never an opaque string",
                            role.name,
                            spec.prefixes[0],
                            spec.suffix,
                            render(other)
                        )));
                    }
                },
                None => match role.default {
                    Some(default) => {
                        resolved.push((index, ParameterValue::Literal(default.to_owned())));
                    }
                    None => {
                        return Err(unliftable(format!(
                            "`{}{}(…)` supplies no `{}` and R declares no default for it, so the \
                             math:Distribution's parameterization cannot fill every required \
                             math:DistributionParameterRole exactly once \
                             (math:DistributionParameterRoleCardinality)",
                            spec.prefixes[0], spec.suffix, role.name
                        )));
                    }
                },
            }
        }

        let value_key: Vec<String> = resolved
            .iter()
            .map(|(index, value)| format!("{}={}", spec.roles[*index].name, value.key()))
            .collect();
        let key = format!("dist|{}|{}", spec.family, value_key.join(","));
        let (iri, fresh) = self.mint("distribution", &key);
        if !fresh {
            return Ok(iri);
        }

        let family_iri = self.emit_distribution_family(spec);
        let (parameterization, parameterization_fresh) =
            self.mint("parameterization", spec.parameterization);
        if parameterization_fresh {
            self.sink
                .typed(&parameterization, &math("DistributionParameterization"));
            self.label(&parameterization, spec.parameterization);
        }

        self.sink.typed(&iri, &math("Distribution"));
        self.sink
            .iri(&iri, &math("distributionFamily"), &family_iri);
        self.sink.iri(
            &iri,
            &math("distributionParameterization"),
            &parameterization,
        );
        self.label(&iri, spec.family);

        for (index, value) in resolved {
            let role = &spec.roles[index];
            let role_key = format!("{}|{}", spec.parameterization, role.name);
            let (role_iri, role_fresh) = self.mint("role", &role_key);
            if role_fresh {
                self.sink
                    .typed(&role_iri, &math("DistributionParameterRole"));
                self.sink.iri(
                    &role_iri,
                    &math("roleWithinParameterization"),
                    &parameterization,
                );
                self.sink
                    .boolean(&role_iri, &math("requiresPositiveValue"), role.positive);
                self.sink.iri(
                    &role_iri,
                    &math("quantityDimension"),
                    &math("dimensionless"),
                );
                self.label(&role_iri, role.name);
            }
            self.sink
                .iri(&parameterization, &math("requiresParameterRole"), &role_iri);

            let parameter_key = format!("{key}|{}", role.name);
            let (parameter, _) = self.mint("parameter", &parameter_key);
            self.sink.typed(&parameter, &math("DistributionParameter"));
            self.sink.iri(&parameter, &math("parameterRole"), &role_iri);
            match value {
                ParameterValue::Literal(text) => {
                    let (quantity, _) = self.mint("quantity", &parameter_key);
                    self.sink.typed(&quantity, &math("Quantity"));
                    self.sink
                        .iri(&quantity, &math("hasDimension"), &math("dimensionless"));
                    if let Ok(number) = text.parse::<f64>() {
                        self.sink.decimal(&quantity, &math("quantityValue"), number);
                    }
                    self.sink
                        .iri(&parameter, &math("parameterQuantity"), &quantity);
                }
                ParameterValue::Symbolic(expr) => {
                    let lifted = self.emit_expression(&expr)?;
                    let expr_iri = self.expression_iri(lifted.node);
                    self.sink
                        .iri(&parameter, &math("parameterExpression"), &expr_iri);
                }
            }
            self.sink
                .iri(&iri, &math("hasDistributionParameter"), &parameter);
        }
        self.statistical += 1;
        Ok(iri)
    }

    // -- the logic: seam -----------------------------------------------------

    /// Record R semantics the lowering did not carry.
    ///
    /// The residue lands on the `math:parseSource` witness as `math:unmappedConstruct`, so
    /// the `logic:LossyLens` rung this bridge declares has queryable CONTENT instead of
    /// being a bare adjective.
    fn record_unmapped(&mut self, construct: &str) {
        if !self.unmapped.contains(construct) {
            self.unmapped.insert(construct.to_owned());
        }
    }

    /// Route control flow and general computation into `logic:`.
    ///
    /// This is the ONLY emitter of `math:compilesToLogicFormula` in the crate, and it emits
    /// the two co-required declarations in the same breath —
    /// `math:LogicLoweringDeclaredConstraint` fires `math:UndeclaredLogicLowering` on any
    /// subject that carries the edge without them.
    ///
    /// The computation node is an ANCHOR, not the content: everything the R parser
    /// recovered rides in the `logic:` formula tree [`Lift::lower_formula`] builds beneath
    /// it. The `rdfs:label` is a full rendering for readability and carries no meaning that
    /// the structure does not already carry.
    fn lower_to_logic(&mut self, expr: &RExpr) {
        let key = expr.structure_key();
        let (iri, fresh) = self.mint("computation", &key);
        if !fresh {
            return;
        }
        let formula = self.lower_formula(expr);
        self.sink.typed(&iri, &math("MathematicalExpression"));
        self.sink
            .iri(&iri, &math("compilesToLogicFormula"), &formula);
        self.sink
            .iri(&iri, &math("denotationKind"), &math("denotesProposition"));
        self.sink.iri(
            &iri,
            &math("logicLoweringPreservation"),
            &logic("SoundUnderApproximation"),
        );
        self.label(&iri, &render(expr));
        self.lowerings += 1;
    }

    // -- logic: formulae -----------------------------------------------------

    /// Lower an R expression in PROPOSITION position into a `logic:Formula`, returning its
    /// IRI.
    ///
    /// Every node this produces selects EXACTLY ONE constructor from
    /// `logic:FormulaConstructorConstraint`'s eight, and honours the typed-IR frontend's
    /// arity invariants (`crates/logic-compile/src/frontend.rs` — `not` one child, `and`/`or`
    /// two or more, an implication one antecedent and one consequent, a quantifier at least
    /// one bound-variable carrier, an atom at least one argument carrier).
    ///
    /// The lowering is a `logic:SoundUnderApproximation` in the precise sense that each
    /// construct's PER-EXECUTION reading survives while its operational envelope does not:
    /// a loop's repetition, a block's ordering, and an assignment's store update are
    /// enumerated as `math:unmappedConstruct` residue rather than silently discarded.
    fn lower_formula(&mut self, expr: &RExpr) -> String {
        let expr = expr.unparenthesized();
        let key = expr.structure_key();
        match expr {
            RExpr::If {
                condition,
                then_branch,
                else_branch,
            } => self.lower_if(&key, condition, then_branch, else_branch.as_deref()),
            RExpr::For {
                variable,
                sequence,
                body,
            } => self.lower_for(&key, variable, sequence, body),
            RExpr::While { condition, body } => {
                self.record_unmapped(
                    "R `while` repetition: the lowered implication states the guard-to-body \
                     relation of ONE iteration; unbounded re-entry has no logic: formula image",
                );
                let guard = self.lower_formula(condition);
                let consequent = self.lower_formula(body);
                self.emit_implication(
                    &format!("while|{key}"),
                    &guard,
                    &consequent,
                    "R `while` loop: one iteration's guard implies its body",
                )
            }
            RExpr::Repeat { body } => {
                self.record_unmapped(
                    "R `repeat` repetition: the lowered predication names the body; unbounded \
                     re-entry has no logic: formula image",
                );
                let body_term = self.lower_term(body);
                self.emit_atom(
                    &format!("repeat|{key}"),
                    "r-repeat",
                    &[body_term],
                    &render(expr),
                )
            }
            RExpr::Block(statements) => self.lower_block(&key, statements),
            RExpr::Break | RExpr::Next => {
                let word = if matches!(expr, RExpr::Break) {
                    "break"
                } else {
                    "next"
                };
                self.record_unmapped(&format!(
                    "R `{word}` non-local control transfer: a logic: formula has no jump \
                     semantics, so the atom names the transfer without discharging it"
                ));
                self.emit_atom(&format!("keyword|{key}"), &format!("r-{word}"), &[], word)
            }
            RExpr::Function { params, body } => self.lower_function(&key, params, body, expr),
            RExpr::Assign { target, value, .. } => self.lower_assignment(target, value),
            RExpr::Pipe { lhs, rhs, native } => {
                let desugared = desugar_pipe(lhs, rhs, *native);
                self.lower_formula(&desugared)
            }
            RExpr::Call { callee, args } => self.lower_call_atom(&key, callee, args, expr),
            RExpr::Unary {
                op: UnaryOp::Not,
                operand,
            } => {
                let inner = self.lower_formula(operand);
                self.emit_negation(&format!("not|{key}"), &inner, &render(expr))
            }
            RExpr::Binary { op, lhs, rhs } => self.lower_binary_formula(&key, *op, lhs, rhs, expr),
            RExpr::Special { operator, lhs, rhs } => {
                let l = self.lower_term(lhs);
                let r = self.lower_term(rhs);
                self.emit_atom(
                    &format!("special|{key}"),
                    &format!("r-special:{operator}"),
                    &[l, r],
                    &render(expr),
                )
            }
            other => self.lower_value_atom(&key, other),
        }
    }

    /// `if (c) t` → `c → t`; `if (c) t else e` → `(c → t) ∧ (¬c → e)`.
    ///
    /// A real implication over the LOWERED condition, exactly as
    /// `MATHEMATICS-BRIDGES.md` requires — not an opaque node tagged `r-if`. Both arms are
    /// lowered, so an `else` branch cannot vanish.
    fn lower_if(
        &mut self,
        key: &str,
        condition: &RExpr,
        then_branch: &RExpr,
        else_branch: Option<&RExpr>,
    ) -> String {
        let guard = self.lower_formula(condition);
        let consequent = self.lower_formula(then_branch);
        let Some(else_branch) = else_branch else {
            return self.emit_implication(
                &format!("if|{key}"),
                &guard,
                &consequent,
                "R `if` without an `else` arm: the condition implies the consequent",
            );
        };
        let alternative = self.lower_formula(else_branch);
        let negated = self.emit_negation(
            &format!("if-not|{key}"),
            &guard,
            "the negated condition of an R `if`/`else`",
        );
        let positive = self.emit_implication(
            &format!("if-then|{key}"),
            &guard,
            &consequent,
            "R `if` consequent arm",
        );
        let negative = self.emit_implication(
            &format!("if-else|{key}"),
            &negated,
            &alternative,
            "R `if` alternative arm",
        );
        self.emit_conjunction(
            &format!("if|{key}"),
            &[positive, negative],
            "R `if`/`else`: both arms, each guarded by its own condition",
        )
    }

    /// `for (v in s) b` → `∀v. member(v, s) → b`.
    ///
    /// The traversal IS a universal quantification over the sequence's elements, so the loop
    /// variable becomes a real `logic:quantifiedVariable` and the membership guard a real
    /// antecedent. What does not survive is the ORDER the elements are visited in, which is
    /// enumerated as residue.
    fn lower_for(&mut self, key: &str, variable: &str, sequence: &RExpr, body: &RExpr) -> String {
        self.record_unmapped(
            "R `for` iteration order: the lowered universal quantification binds every element \
             of the sequence, not the order in which they are visited",
        );
        let bound = LogicTerm::Variable(variable.to_owned());
        let sequence_term = self.lower_term(sequence);
        let guard = self.emit_atom(
            &format!("for-in|{key}"),
            "r-for-in",
            &[bound.clone(), sequence_term],
            "R `for` loop membership guard",
        );
        let consequent = self.lower_formula(body);
        let implication = self.emit_implication(
            &format!("for-body|{key}"),
            &guard,
            &consequent,
            "R `for` body, guarded by sequence membership",
        );
        let quantifier_key = format!("for|{key}");
        let (iri, fresh) = self.mint("logic-formula", &quantifier_key);
        if fresh {
            self.sink.typed(&iri, &logic("Formula"));
            self.emit_carrier(&iri, "quantifiedVariable", &quantifier_key, 0, &bound);
            self.sink.iri(&iri, &logic("forall"), &implication);
            self.label(&iri, &format!("R `for` loop binding `{variable}`"));
        }
        iri
    }

    /// `{ s1; s2; … }` → the conjunction of its lowered statements.
    fn lower_block(&mut self, key: &str, statements: &[RStmt]) -> String {
        match statements {
            [] => self.emit_atom(&format!("block|{key}"), "r-empty-block", &[], "{ }"),
            [only] => self.lower_statement_formula(only),
            many => {
                self.record_unmapped(
                    "R `{ }` statement sequencing: conjunction is commutative, so the order the \
                     block's statements evaluate in does not survive the lowering",
                );
                let mut operands = Vec::with_capacity(many.len());
                for statement in many {
                    let lowered = self.lower_statement_formula(statement);
                    operands.push(lowered);
                }
                self.emit_conjunction(
                    &format!("block|{key}"),
                    &operands,
                    "R `{ }` block: every statement, conjoined",
                )
            }
        }
    }

    fn lower_statement_formula(&mut self, statement: &RStmt) -> String {
        match &statement.kind {
            RStmtKind::Assign { target, value, .. } => self.lower_assignment(target, value),
            RStmtKind::Expr(expr) => self.lower_formula(expr),
        }
    }

    /// `x <- v` → `r-assign(x, v)`.
    fn lower_assignment(&mut self, target: &RExpr, value: &RExpr) -> String {
        self.record_unmapped(
            "R `<-` assignment as state mutation: the lowered predication relates the target to \
             the value it was bound to; it does not update a store, so a later rebinding of the \
             same name is a second predication rather than an overwrite",
        );
        let target_term = self.lower_term(target);
        let value_term = self.lower_term(value);
        let key = format!(
            "assign|{}|{}",
            target.structure_key(),
            value.structure_key()
        );
        let label = format!("{} <- {}", render(target), render(value));
        self.emit_atom(&key, "r-assign", &[target_term, value_term], &label)
    }

    /// `function(p…) b` → `r-function(p…, b)`.
    ///
    /// A predication naming the formals and the body, not a quantification: a `function`
    /// literal is a VALUE, and asserting `∀p. body` would claim the body holds for every
    /// argument — a claim R's definition does not make.
    fn lower_function(
        &mut self,
        key: &str,
        params: &[Param],
        body: &RExpr,
        expr: &RExpr,
    ) -> String {
        self.record_unmapped(
            "R `function` closure: the lowered predication names the formals and the body, \
             not a first-class function value, its lexical environment, or its lazy-promise \
             argument evaluation",
        );
        let mut args = self.lower_params(params);
        let body_term = self.lower_term(body);
        args.push(body_term);
        self.emit_atom(
            &format!("function|{key}"),
            "r-function",
            &args,
            &render(expr),
        )
    }

    fn lower_params(&mut self, params: &[Param]) -> Vec<LogicTerm> {
        let mut out = Vec::with_capacity(params.len());
        for param in params {
            if param.name == "..." {
                self.record_unmapped(
                    "R `...` variadic formals: the marker binds an argument SEQUENCE whose \
                     length and names the definition does not state",
                );
                out.push(LogicTerm::SequenceMarker("...".to_owned()));
                continue;
            }
            let name = LogicTerm::Variable(param.name.clone());
            match &param.default {
                None => out.push(name),
                Some(default) => {
                    let default_term = self.lower_term(default);
                    let key = format!("t|default|{}|{}", param.name, default.structure_key());
                    let term =
                        self.emit_function_term(&key, "r-parameter-default", &[name, default_term]);
                    out.push(term);
                }
            }
        }
        out
    }

    /// A call in proposition position: ONE reified relation per callee.
    ///
    /// `library(stats)` and `set.seed(20260725)` therefore predicate DIFFERENT relations —
    /// the relation identity includes the function name, so the predicate carries which call
    /// it was instead of collapsing every call in the script onto one `r-call` node.
    fn lower_call_atom(&mut self, key: &str, callee: &RExpr, args: &[Arg], expr: &RExpr) -> String {
        let mut terms = self.lower_arguments(args);
        let key = format!("call|{key}");
        match callable_name(callee) {
            Some(name) => {
                self.record_uninterpreted_call(&name);
                self.emit_atom(&key, &format!("r-call:{name}"), &terms, &render(expr))
            }
            None => {
                // A computed callee (`handlers[[i]](x)`): the callee itself becomes the first
                // argument of a generic application relation, so nothing about it is dropped.
                self.record_unmapped(
                    "R computed callee: the applied function is a value the script computes, so \
                     the lowered relation is a generic application rather than a named one",
                );
                let callee_term = self.lower_term(callee);
                terms.insert(0, callee_term);
                self.emit_atom(&key, "r-apply", &terms, &render(expr))
            }
        }
    }

    /// Name an R function whose OWN semantics the lift does not interpret.
    ///
    /// The call itself is carried — a relation named for the callee, over ordered argument
    /// carriers. What is not carried is what the function DOES: `library` attaching a
    /// package, `set.seed` seeding the generator, `message` writing to a connection. This is
    /// exactly the residue `math:unmappedConstruct`'s own `skos:example` enumerates
    /// (`"r-call: message(...)"`), and enumerating it per callee is what keeps the
    /// `logic:LossyLens` rung falsifiable instead of decorative.
    fn record_uninterpreted_call(&mut self, name: &str) {
        self.record_unmapped(&format!(
            "R call `{name}`: lowered as an uninterpreted relation over its arguments, so what \
             the function itself does is not carried"
        ));
    }

    fn lower_binary_formula(
        &mut self,
        key: &str,
        op: BinaryOp,
        lhs: &RExpr,
        rhs: &RExpr,
        expr: &RExpr,
    ) -> String {
        match op {
            BinaryOp::And | BinaryOp::AndAnd => {
                let l = self.lower_formula(lhs);
                let r = self.lower_formula(rhs);
                self.emit_conjunction(&format!("and|{key}"), &[l, r], &render(expr))
            }
            BinaryOp::Or | BinaryOp::OrOr => {
                let l = self.lower_formula(lhs);
                let r = self.lower_formula(rhs);
                self.emit_disjunction(&format!("or|{key}"), &[l, r], &render(expr))
            }
            BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual
            | BinaryOp::Equal
            | BinaryOp::NotEqual => {
                let l = self.lower_term(lhs);
                let r = self.lower_term(rhs);
                self.emit_atom(
                    &format!("compare|{key}"),
                    &format!("r-{}", binary_operator_slug(op)),
                    &[l, r],
                    &render(expr),
                )
            }
            // Arithmetic and `:` are not propositions. R coerces the value to a condition at
            // run time; the lowering says only that this expression's VALUE is the statement's
            // content, and the value's own structure rides in the term.
            _ => self.lower_value_atom(key, expr),
        }
    }

    /// An expression with no propositional reading: `r-value(<term>)`, over its FULL term
    /// structure.
    fn lower_value_atom(&mut self, key: &str, expr: &RExpr) -> String {
        let term = self.lower_term(expr);
        self.emit_atom(&format!("value|{key}"), "r-value", &[term], &render(expr))
    }

    // -- logic: terms --------------------------------------------------------

    /// Lower an R expression in TERM position.
    ///
    /// Every arm produces exactly one `logic:TermCarrier` value kind. A compound expression
    /// becomes a `logic:FunctionTerm` reached through `logic:termApplication`, so a nested
    /// call such as `nrow(cars)` round-trips as a real application rather than a string.
    fn lower_term(&mut self, expr: &RExpr) -> LogicTerm {
        let expr = expr.unparenthesized();
        let key = format!("t|{}", expr.structure_key());
        match expr {
            RExpr::Number { text, integer, .. } => number_term(text, *integer),
            RExpr::Str(text) => LogicTerm::Literal {
                lexical: text.clone(),
                datatype: None,
            },
            RExpr::Logical(value) => LogicTerm::Literal {
                lexical: if *value { "true" } else { "false" }.to_owned(),
                datatype: Some(XSD_BOOLEAN),
            },
            // `NaN` and `Inf` ARE `xsd:double` values, so they ride as typed literals with no
            // loss at all.
            RExpr::NotANumber => LogicTerm::Literal {
                lexical: "NaN".to_owned(),
                datatype: Some(XSD_DOUBLE),
            },
            RExpr::Infinity => LogicTerm::Literal {
                lexical: "INF".to_owned(),
                datatype: Some(XSD_DOUBLE),
            },
            RExpr::Null => {
                self.record_unmapped(
                    "R `NULL`: its zero-length-vector semantics have no logic: term image, so \
                     the constant rides as a named individual rather than as an absent argument",
                );
                self.language_constant("NULL")
            }
            RExpr::Na => {
                self.record_unmapped(
                    "R `NA`: three-valued missingness has no logic: truth value, so the constant \
                     rides as a named individual and propagation is not modelled",
                );
                self.language_constant("NA")
            }
            RExpr::Break | RExpr::Next => {
                let word = if matches!(expr, RExpr::Break) {
                    "break"
                } else {
                    "next"
                };
                self.record_unmapped(&format!(
                    "R `{word}` non-local control transfer: a logic: formula has no jump \
                     semantics, so the atom names the transfer without discharging it"
                ));
                self.language_constant(word)
            }
            RExpr::Ident(name) => LogicTerm::Variable(name.clone()),
            RExpr::Namespace {
                package,
                name,
                internal,
            } => {
                let symbol = if *internal {
                    "r-namespace-internal"
                } else {
                    "r-namespace"
                };
                let args = [plain_literal(package), plain_literal(name)];
                self.emit_function_term(&key, symbol, &args)
            }
            RExpr::Component { object, name, slot } => {
                let object_term = self.lower_term(object);
                let symbol = if *slot { "r-slot" } else { "r-component" };
                self.emit_function_term(&key, symbol, &[object_term, plain_literal(name)])
            }
            RExpr::Index {
                object,
                args,
                double,
            } => {
                let mut terms = vec![self.lower_term(object)];
                terms.extend(self.lower_arguments(args));
                let symbol = if *double {
                    "r-subscript-double"
                } else {
                    "r-subscript"
                };
                self.emit_function_term(&key, symbol, &terms)
            }
            RExpr::Call { callee, args } => {
                let mut terms = self.lower_arguments(args);
                match callable_name(callee) {
                    Some(name) => {
                        self.record_uninterpreted_call(&name);
                        self.emit_function_term(&key, &format!("r-call:{name}"), &terms)
                    }
                    None => {
                        self.record_unmapped(
                            "R computed callee: the applied function is a value the script \
                             computes, so the lowered term is a generic application rather than a \
                             named one",
                        );
                        let callee_term = self.lower_term(callee);
                        terms.insert(0, callee_term);
                        self.emit_function_term(&key, "r-apply", &terms)
                    }
                }
            }
            RExpr::Pipe { lhs, rhs, native } => {
                let desugared = desugar_pipe(lhs, rhs, *native);
                self.lower_term(&desugared)
            }
            RExpr::Unary { op, operand } => {
                let operand_term = self.lower_term(operand);
                self.emit_function_term(&key, unary_operator_symbol(*op), &[operand_term])
            }
            RExpr::Binary { op, lhs, rhs } => {
                let l = self.lower_term(lhs);
                let r = self.lower_term(rhs);
                self.emit_function_term(&key, &format!("r-{}", binary_operator_slug(*op)), &[l, r])
            }
            RExpr::Special { operator, lhs, rhs } => {
                let l = self.lower_term(lhs);
                let r = self.lower_term(rhs);
                self.emit_function_term(&key, &format!("r-special:{operator}"), &[l, r])
            }
            RExpr::Formula(formula) => self.lower_model_formula_term(&key, formula),
            RExpr::Assign { target, value, .. } => {
                self.record_unmapped(
                    "R `<-` assignment as state mutation: the lowered predication relates the \
                     target to the value it was bound to; it does not update a store, so a later \
                     rebinding of the same name is a second predication rather than an overwrite",
                );
                let target_term = self.lower_term(target);
                let value_term = self.lower_term(value);
                self.emit_function_term(&key, "r-assign", &[target_term, value_term])
            }
            RExpr::Function { params, body } => {
                self.record_unmapped(
                    "R `function` closure: the lowered predication names the formals and the \
                     body, not a first-class function value, its lexical environment, or its \
                     lazy-promise argument evaluation",
                );
                let mut args = self.lower_params(params);
                let body_term = self.lower_term(body);
                args.push(body_term);
                self.emit_function_term(&key, "r-function", &args)
            }
            RExpr::Block(statements) => {
                if statements.len() > 1 {
                    self.record_unmapped(
                        "R `{ }` statement sequencing: conjunction is commutative, so the order \
                         the block's statements evaluate in does not survive the lowering",
                    );
                }
                let mut terms = Vec::with_capacity(statements.len());
                for statement in statements {
                    let term = match &statement.kind {
                        RStmtKind::Assign { target, value, .. } => {
                            let target_term = self.lower_term(target);
                            let value_term = self.lower_term(value);
                            self.emit_function_term(
                                &format!(
                                    "t|assign|{}|{}",
                                    target.structure_key(),
                                    value.structure_key()
                                ),
                                "r-assign",
                                &[target_term, value_term],
                            )
                        }
                        RStmtKind::Expr(expr) => self.lower_term(expr),
                    };
                    terms.push(term);
                }
                if terms.is_empty() {
                    return LogicTerm::SequenceMarker("r-empty-block".to_owned());
                }
                self.emit_function_term(&key, "r-block", &terms)
            }
            RExpr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut terms = vec![self.lower_term(condition), self.lower_term(then_branch)];
                if let Some(else_branch) = else_branch {
                    let alternative = self.lower_term(else_branch);
                    terms.push(alternative);
                }
                self.emit_function_term(&key, "r-if", &terms)
            }
            RExpr::For {
                variable,
                sequence,
                body,
            } => {
                self.record_unmapped(
                    "R `for` iteration order: the lowered universal quantification binds every \
                     element of the sequence, not the order in which they are visited",
                );
                let sequence_term = self.lower_term(sequence);
                let body_term = self.lower_term(body);
                let terms = [
                    LogicTerm::Variable(variable.clone()),
                    sequence_term,
                    body_term,
                ];
                self.emit_function_term(&key, "r-for", &terms)
            }
            RExpr::While { condition, body } => {
                self.record_unmapped(
                    "R `while` repetition: the lowered implication states the guard-to-body \
                     relation of ONE iteration; unbounded re-entry has no logic: formula image",
                );
                let guard = self.lower_term(condition);
                let body_term = self.lower_term(body);
                self.emit_function_term(&key, "r-while", &[guard, body_term])
            }
            RExpr::Repeat { body } => {
                self.record_unmapped(
                    "R `repeat` repetition: the lowered predication names the body; unbounded \
                     re-entry has no logic: formula image",
                );
                let body_term = self.lower_term(body);
                self.emit_function_term(&key, "r-repeat", &[body_term])
            }
            RExpr::Paren(_) => unreachable!("the expression was unparenthesized above"),
        }
    }

    /// Call and subscript arguments, in source order, with the argument NAMES retained.
    fn lower_arguments(&mut self, args: &[Arg]) -> Vec<LogicTerm> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            let value = match &arg.value {
                Some(value) => self.lower_term(value),
                None => {
                    // R's genuinely EMPTY argument (`x[, 1]`) means "the whole extent of this
                    // margin": a SEQUENCE of indices, not a missing one.
                    self.record_unmapped(
                        "R empty argument (`x[, i]`, `f(a = )`): the margin's extent rides as a \
                         logic:SequenceMarker because the source does not state its length",
                    );
                    LogicTerm::SequenceMarker("r-empty-argument".to_owned())
                }
            };
            match &arg.name {
                None => out.push(value),
                Some(name) => {
                    let key = format!(
                        "t|named|{name}|{}",
                        arg.value
                            .as_ref()
                            .map_or_else(|| "empty".to_owned(), RExpr::structure_key)
                    );
                    let term = self.emit_function_term(
                        &key,
                        "r-named-argument",
                        &[plain_literal(name), value],
                    );
                    out.push(term);
                }
            }
        }
        out
    }

    /// A model formula reaching the `logic:` seam (`plot(y ~ x)`): a real application over
    /// its response, its surviving terms, its explicitly removed terms, and its intercept
    /// flag — never a rendering of the `~`.
    fn lower_model_formula_term(&mut self, key: &str, formula: &Formula) -> LogicTerm {
        let mut args = Vec::with_capacity(formula.terms.len() + formula.removed.len() + 2);
        args.push(match &formula.response {
            Some(response) => self.lower_term(response),
            None => LogicTerm::SequenceMarker("r-no-response".to_owned()),
        });
        for term in &formula.terms {
            let lowered = self.lower_formula_factor(term);
            args.push(lowered);
        }
        for removed in &formula.removed {
            let inner = self.lower_formula_factor(removed);
            let term = self.emit_function_term(
                &format!("t|removed|{}", removed.structure_key()),
                "r-removed",
                &[inner],
            );
            args.push(term);
        }
        args.push(LogicTerm::Literal {
            lexical: formula.intercept.to_string(),
            datatype: Some(XSD_BOOLEAN),
        });
        self.emit_function_term(key, "r-model-formula", &args)
    }

    fn lower_formula_factor(&mut self, term: &FormulaTerm) -> LogicTerm {
        if let [only] = term.factors.as_slice() {
            return self.lower_term(only);
        }
        let mut factors = Vec::with_capacity(term.factors.len());
        for factor in &term.factors {
            let lowered = self.lower_term(factor);
            factors.push(lowered);
        }
        self.emit_function_term(
            &format!("t|interaction|{}", term.structure_key()),
            "r-interaction",
            &factors,
        )
    }

    // -- logic: AST emitters -------------------------------------------------

    /// The reified `logic:Type` individual an atomic predication predicates.
    ///
    /// `logic:relation`'s range is a reified `logic:Type` per the HiLog reflection:
    /// predicating over a reified relation keeps the object level first-order rather than
    /// admitting a predicate variable. ONE individual per relation NAME, so two calls to the
    /// same R function share a relation and two calls to different ones never do.
    fn emit_relation(&mut self, slug: &str) -> String {
        let (iri, fresh) = self.mint("logic-relation", slug);
        if fresh {
            self.sink.typed(&iri, &logic("Type"));
            self.label(&iri, slug);
        }
        iri
    }

    /// The reified `logic:Type` individual a `logic:FunctionTerm` applies.
    ///
    /// Minted under a DIFFERENT role than [`Lift::emit_relation`]: `f` used as a relation and
    /// `f` used as a function symbol are different things, and giving them one IRI would
    /// assert that a truth value and a term are the same node.
    fn emit_symbol(&mut self, slug: &str) -> String {
        let (iri, fresh) = self.mint("logic-symbol", slug);
        if fresh {
            self.sink.typed(&iri, &logic("Type"));
            self.label(&iri, slug);
        }
        iri
    }

    /// An R language constant (`NULL`, `NA`, `break`, `next`) as a named individual.
    ///
    /// A zero-arity symbol is an INDIVIDUAL, not a nullary function term
    /// (`logic:FunctionTerm`'s own `gmeow:avoidWhen`: "do not use a function term to wrap a
    /// constant"), so it rides on `logic:termIri`. It is typed `math:MathematicalObject` —
    /// the crate's existing type for a bare individual in a lifted run's domain — rather
    /// than left untyped, and what its R semantics actually MEAN is enumerated as residue by
    /// the caller.
    fn language_constant(&mut self, name: &str) -> LogicTerm {
        let (iri, fresh) = self.mint("r-constant", name);
        if fresh {
            self.sink.typed(&iri, &math("MathematicalObject"));
            self.label(&iri, name);
        }
        LogicTerm::Iri(iri)
    }

    /// An atomic predication: one reified relation over ordered argument carriers.
    fn emit_atom(
        &mut self,
        key: &str,
        relation_slug: &str,
        args: &[LogicTerm],
        label: &str,
    ) -> String {
        let relation = self.emit_relation(relation_slug);
        let (iri, fresh) = self.mint("logic-formula", key);
        if !fresh {
            return iri;
        }
        self.sink.typed(&iri, &logic("Formula"));
        self.sink.iri(&iri, &logic("relation"), &relation);
        if args.is_empty() {
            // An atomic predication carries at least one argument carrier (the typed-IR
            // frontend rejects a nullary atom), and an empty R argument list IS a sequence —
            // an empty one — so it rides as a sequence marker rather than as a fabricated
            // argument.
            self.emit_carrier(
                &iri,
                "argument",
                key,
                0,
                &LogicTerm::SequenceMarker("r-empty-argument-list".to_owned()),
            );
        }
        for (index, arg) in args.iter().enumerate() {
            self.emit_carrier(&iri, "argument", key, index, arg);
        }
        self.label(&iri, label);
        iri
    }

    /// A `logic:FunctionTerm`, or — for a zero-arity symbol — the reified symbol itself.
    fn emit_function_term(
        &mut self,
        key: &str,
        symbol_slug: &str,
        args: &[LogicTerm],
    ) -> LogicTerm {
        let symbol = self.emit_symbol(symbol_slug);
        if args.is_empty() {
            // `logic:FunctionTermArityConstraint` forbids a nullary application: a 0-ary
            // function symbol is a constant and belongs on `logic:termIri`.
            return LogicTerm::Iri(symbol);
        }
        let (iri, fresh) = self.mint("logic-term", key);
        if !fresh {
            return LogicTerm::Application(iri);
        }
        self.sink.typed(&iri, &logic("FunctionTerm"));
        self.sink.iri(&iri, &logic("functionSymbol"), &symbol);
        for (index, arg) in args.iter().enumerate() {
            self.emit_carrier(&iri, "argument", key, index, arg);
        }
        LogicTerm::Application(iri)
    }

    /// One ordered `logic:TermCarrier`: a zero-based index and EXACTLY one value kind.
    fn emit_carrier(
        &mut self,
        parent: &str,
        link: &str,
        key: &str,
        index: usize,
        term: &LogicTerm,
    ) {
        let (carrier, fresh) = self.mint("logic-argument", &format!("{key}|{link}|{index}"));
        if fresh {
            self.sink.typed(&carrier, &logic("TermCarrier"));
            let position = i64::try_from(index).unwrap_or(i64::MAX);
            self.sink.integer(&carrier, &logic("termIndex"), position);
            match term {
                LogicTerm::Iri(target) => self.sink.iri(&carrier, &logic("termIri"), target),
                LogicTerm::Variable(name) => {
                    self.sink.string(&carrier, &logic("termVariable"), name);
                }
                LogicTerm::Literal { lexical, datatype } => {
                    // The lexical form rides on `logic:termLiteral` as a PLAIN literal and the
                    // datatype on the companion `logic:termLiteralDatatype`, exactly as
                    // `crates/logic-compile/src/projections/rdf.rs` serializes a typed term.
                    self.sink.string(&carrier, &logic("termLiteral"), lexical);
                    if let Some(datatype) = datatype {
                        self.sink
                            .iri(&carrier, &logic("termLiteralDatatype"), datatype);
                    }
                }
                LogicTerm::Application(target) => {
                    self.sink.iri(&carrier, &logic("termApplication"), target);
                }
                LogicTerm::SequenceMarker(marker) => {
                    self.sink
                        .string(&carrier, &logic("termSequenceMarker"), marker);
                }
            }
        }
        self.sink.iri(parent, &logic(link), &carrier);
    }

    fn emit_implication(
        &mut self,
        key: &str,
        antecedent: &str,
        consequent: &str,
        label: &str,
    ) -> String {
        let (iri, fresh) = self.mint("logic-formula", key);
        if fresh {
            self.sink.typed(&iri, &logic("Formula"));
            self.sink.iri(&iri, &logic("antecedent"), antecedent);
            self.sink.iri(&iri, &logic("consequent"), consequent);
            self.label(&iri, label);
        }
        iri
    }

    fn emit_negation(&mut self, key: &str, inner: &str, label: &str) -> String {
        let (iri, fresh) = self.mint("logic-formula", key);
        if fresh {
            self.sink.typed(&iri, &logic("Formula"));
            self.sink.iri(&iri, &logic("not"), inner);
            self.label(&iri, label);
        }
        iri
    }

    fn emit_conjunction(&mut self, key: &str, operands: &[String], label: &str) -> String {
        self.emit_junction(key, "and", operands, label)
    }

    fn emit_disjunction(&mut self, key: &str, operands: &[String], label: &str) -> String {
        self.emit_junction(key, "or", operands, label)
    }

    /// A conjunction or disjunction over DISTINCT operands.
    ///
    /// Deduplicating first is not cosmetic: `logic:and` is a set-valued edge, so `{ x; x }`
    /// would serialize one operand and the typed-IR frontend rejects a junction with fewer
    /// than two. An idempotent junction IS its single operand, so that is what it returns.
    fn emit_junction(
        &mut self,
        key: &str,
        connective: &str,
        operands: &[String],
        label: &str,
    ) -> String {
        let mut distinct: Vec<&String> = Vec::with_capacity(operands.len());
        for operand in operands {
            if !distinct.contains(&operand) {
                distinct.push(operand);
            }
        }
        if let [only] = distinct.as_slice() {
            return (*only).clone();
        }
        let distinct: Vec<String> = distinct.into_iter().cloned().collect();
        let (iri, fresh) = self.mint("logic-formula", key);
        if fresh {
            self.sink.typed(&iri, &logic("Formula"));
            for operand in &distinct {
                self.sink.iri(&iri, &logic(connective), operand);
            }
            self.label(&iri, label);
        }
        iri
    }
}

/// One lowered `logic:` term — the single value a [`logic:TermCarrier`] may carry.
///
/// `logic:TermCarrierValueConstraint` requires EXACTLY one of the five kinds on every
/// carrier, so they are an enum here rather than five optional fields: a carrier with two
/// values is unrepresentable.
///
/// [`logic:TermCarrier`]: https://blackcatinformatics.ca/logic/TermCarrier
#[derive(Debug, Clone, PartialEq, Eq)]
enum LogicTerm {
    /// `logic:termIri` — an individual, or a reified relation/type, named by an IRI.
    Iri(String),
    /// `logic:termVariable` — an R identifier.
    Variable(String),
    /// `logic:termLiteral`, with its datatype on `logic:termLiteralDatatype`.
    Literal {
        /// The literal's lexical form.
        lexical: String,
        /// Its datatype IRI, absent for a plain literal (an R string).
        datatype: Option<&'static str>,
    },
    /// `logic:termApplication` — a nested `logic:FunctionTerm`.
    Application(String),
    /// `logic:termSequenceMarker` — a marker binding a SEQUENCE of terms.
    SequenceMarker(String),
}

/// A plain (untyped) literal term — an R string, or a name that is data rather than a
/// variable (an argument name, a component name, a package name).
fn plain_literal(text: &str) -> LogicTerm {
    LogicTerm::Literal {
        lexical: text.to_owned(),
        datatype: None,
    }
}

/// An R numeric literal as a typed `logic:` term.
///
/// R's `1L` is an integer and `1` is a double, and the parser's canonical `text` is always
/// an exponent-free decimal — so the `L` form drops its `.0` to make a legal `xsd:integer`
/// lexical form and everything else stays `xsd:decimal`.
fn number_term(text: &str, integer: bool) -> LogicTerm {
    if integer && let Some(whole) = text.strip_suffix(".0") {
        return LogicTerm::Literal {
            lexical: whole.to_owned(),
            datatype: Some(XSD_INTEGER),
        };
    }
    LogicTerm::Literal {
        lexical: text.to_owned(),
        datatype: Some(XSD_DECIMAL),
    }
}

/// The reified function symbol an R prefix operator applies.
fn unary_operator_symbol(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Plus => "r-unary-plus",
        UnaryOp::Negate => "r-negate",
        UnaryOp::Not => "r-not",
    }
}

// ── Tables ────────────────────────────────────────────────────────────────────

/// A distribution parameter's role within its parameterization.
struct ParameterRole {
    name: &'static str,
    /// Whether the role's value must be strictly positive.
    positive: bool,
    /// R's documented default, or `None` when the language requires the argument.
    default: Option<&'static str>,
}

/// One R distribution family and the conventional parameterization its `r`/`d`/`p`/`q`
/// functions use.
struct DistributionSpec {
    suffix: &'static str,
    prefixes: &'static [&'static str],
    family: &'static str,
    parameterization: &'static str,
    roles: &'static [ParameterRole],
}

enum ParameterValue {
    Literal(String),
    Symbolic(RExpr),
}

impl ParameterValue {
    fn key(&self) -> String {
        match self {
            Self::Literal(text) => format!("lit:{text}"),
            Self::Symbolic(expr) => format!("expr:{}", expr.structure_key()),
        }
    }
}

const DISTRIBUTIONS: &[DistributionSpec] = &[
    DistributionSpec {
        suffix: "norm",
        prefixes: &["r", "d", "p", "q"],
        family: "normal",
        parameterization: "normal mean/standard-deviation",
        roles: &[
            ParameterRole {
                name: "mean",
                positive: false,
                default: Some("0.0"),
            },
            ParameterRole {
                name: "sd",
                positive: true,
                default: Some("1.0"),
            },
        ],
    },
    DistributionSpec {
        suffix: "binom",
        prefixes: &["r", "d", "p", "q"],
        family: "binomial",
        parameterization: "binomial size/probability",
        roles: &[
            ParameterRole {
                name: "size",
                positive: true,
                default: None,
            },
            ParameterRole {
                name: "prob",
                positive: true,
                default: None,
            },
        ],
    },
    DistributionSpec {
        suffix: "pois",
        prefixes: &["r", "d", "p", "q"],
        family: "poisson",
        parameterization: "poisson rate",
        roles: &[ParameterRole {
            name: "lambda",
            positive: true,
            default: None,
        }],
    },
    DistributionSpec {
        suffix: "unif",
        prefixes: &["r", "d", "p", "q"],
        family: "uniform",
        parameterization: "uniform min/max",
        roles: &[
            ParameterRole {
                name: "min",
                positive: false,
                default: Some("0.0"),
            },
            ParameterRole {
                name: "max",
                positive: false,
                default: Some("1.0"),
            },
        ],
    },
    DistributionSpec {
        suffix: "exp",
        prefixes: &["r", "d", "p", "q"],
        family: "exponential",
        parameterization: "exponential rate",
        roles: &[ParameterRole {
            name: "rate",
            positive: true,
            default: Some("1.0"),
        }],
    },
    DistributionSpec {
        suffix: "gamma",
        prefixes: &["r", "d", "p", "q"],
        family: "gamma",
        parameterization: "gamma shape/rate",
        roles: &[
            ParameterRole {
                name: "shape",
                positive: true,
                default: None,
            },
            ParameterRole {
                name: "rate",
                positive: true,
                default: Some("1.0"),
            },
        ],
    },
    DistributionSpec {
        suffix: "beta",
        prefixes: &["r", "d", "p", "q"],
        family: "beta",
        parameterization: "beta shape1/shape2",
        roles: &[
            ParameterRole {
                name: "shape1",
                positive: true,
                default: None,
            },
            ParameterRole {
                name: "shape2",
                positive: true,
                default: None,
            },
        ],
    },
    DistributionSpec {
        suffix: "chisq",
        prefixes: &["r", "d", "p", "q"],
        family: "chi-squared",
        parameterization: "chi-squared degrees-of-freedom",
        roles: &[ParameterRole {
            name: "df",
            positive: true,
            default: None,
        }],
    },
    DistributionSpec {
        suffix: "t",
        prefixes: &["r", "d", "p", "q"],
        family: "student-t",
        parameterization: "student-t degrees-of-freedom",
        roles: &[ParameterRole {
            name: "df",
            positive: true,
            default: None,
        }],
    },
];

/// R functions whose result is a data frame this lift holds by reference.
const DATASET_FUNCTIONS: &[&str] = &[
    "data.frame",
    "as.data.frame",
    "tibble",
    "as_tibble",
    "read.csv",
    "read.csv2",
    "read.table",
    "read.delim",
    "read_csv",
    "na.omit",
    "subset",
    "model.frame",
];

/// R functions that yield broom-tidy-shaped coefficient estimates.
const ESTIMATE_FUNCTIONS: &[&str] = &["coef", "coefficients", "tidy"];

/// R functions that yield per-observation residuals.
const RESIDUAL_FUNCTIONS: &[&str] = &["residuals", "resid", "augment"];

/// R functions that READ a fit, producing a vantage-held observation.
const SUMMARY_FUNCTIONS: &[&str] = &["summary", "glance"];

/// The R functions with a `math:` operator image, and the operator slug each uses.
const MATH_FUNCTIONS: &[(&str, &str)] = &[
    ("I", "identity"),
    ("log", "log"),
    ("log2", "log2"),
    ("log10", "log10"),
    ("exp", "exp"),
    ("sqrt", "sqrt"),
    ("abs", "abs"),
    ("scale", "scale"),
    ("poly", "poly"),
    ("offset", "offset"),
    ("factor", "factor"),
    ("as.factor", "factor"),
    ("sin", "sin"),
    ("cos", "cos"),
    ("tan", "tan"),
    ("round", "round"),
    ("mean", "mean"),
    ("median", "median"),
    ("sd", "sd"),
    ("var", "var"),
    ("sum", "sum"),
    ("prod", "prod"),
    ("min", "min"),
    ("max", "max"),
    ("cor", "cor"),
    ("cov", "cov"),
    ("quantile", "quantile"),
];

/// Operator slugs that resolve to a `math:` named individual the slice already declares.
const NAMED_OPERATIONS: &[(&str, &str)] = &[
    ("add", "Addition"),
    ("subtract", "Subtraction"),
    ("multiply", "Multiplication"),
    ("divide", "Division"),
    ("power", "Exponentiation"),
    ("sqrt", "Root"),
    ("log", "Logarithm"),
];

fn binary_operator_slug(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Subtract => "subtract",
        BinaryOp::Multiply => "multiply",
        BinaryOp::Divide => "divide",
        BinaryOp::Power => "power",
        BinaryOp::Sequence => "sequence",
        BinaryOp::Less => "less",
        BinaryOp::Greater => "greater",
        BinaryOp::LessEqual => "less-equal",
        BinaryOp::GreaterEqual => "greater-equal",
        BinaryOp::Equal => "equal",
        BinaryOp::NotEqual => "not-equal",
        BinaryOp::And => "and",
        BinaryOp::AndAnd => "and-scalar",
        BinaryOp::Or => "or",
        BinaryOp::OrOr => "or-scalar",
    }
}

fn model_estimator(name: &str) -> Option<Estimator> {
    match name {
        "lm" => Some(Estimator::OrdinaryLeastSquares),
        "gls" => Some(Estimator::GeneralizedLeastSquares),
        "glm" | "lmer" | "glmer" | "lme" => Some(Estimator::MaximumLikelihood),
        "nls" => Some(Estimator::NonlinearLeastSquares),
        _ => None,
    }
}

fn distribution_family(function: &str) -> Option<&'static DistributionSpec> {
    DISTRIBUTIONS.iter().find(|spec| {
        spec.prefixes
            .iter()
            .any(|p| function == format!("{p}{}", spec.suffix))
    })
}

/// A GLM `family =` argument names a family either bare (`binomial`) or as a call
/// (`binomial(link = "logit")`).
fn family_link_name(expr: &RExpr) -> Option<String> {
    match expr.unparenthesized() {
        RExpr::Ident(name) => Some(name.clone()),
        RExpr::Str(name) => Some(name.clone()),
        RExpr::Call { callee, .. } => callable_name(callee),
        _ => None,
    }
}

fn callable_name(callee: &RExpr) -> Option<String> {
    match callee.unparenthesized() {
        RExpr::Ident(name) => Some(name.clone()),
        RExpr::Namespace { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn named_argument<'a>(args: &'a [Arg], name: &str) -> Option<&'a RExpr> {
    args.iter()
        .find(|a| a.name.as_deref() == Some(name))
        .and_then(|a| a.value.as_ref())
}

/// The argument bound to `name`, else the `index`-th argument that carries no name.
fn named_or_positional<'a>(args: &'a [Arg], name: &str, index: usize) -> Option<&'a RExpr> {
    if let Some(found) = named_argument(args, name) {
        return Some(found);
    }
    args.iter()
        .filter(|a| a.name.is_none())
        .nth(index)
        .and_then(|a| a.value.as_ref())
}

/// The coefficient names R's `coef()` would return for a formula, in R's order.
fn coefficient_names(formula: &Formula) -> Vec<String> {
    let mut names = Vec::new();
    if formula.intercept {
        names.push("(Intercept)".to_owned());
    }
    for term in &formula.terms {
        names.push(
            term.factors
                .iter()
                .map(render)
                .collect::<Vec<_>>()
                .join(":"),
        );
    }
    names
}

fn unliftable(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(RUnliftable { detail })
}

// ── Rendering (labels only) ───────────────────────────────────────────────────

/// Render an expression back to compact R-ish source, for `rdfs:label` only.
///
/// NEVER the identity of anything: identity is the arena's `ContentKey`. This exists so a
/// by-reference node (`math:DatasetMatrix`, a `math:VariableExpression`) carries the R name
/// it stands for, which is what makes "held by reference" a reference rather than a hash.
fn render(expr: &RExpr) -> String {
    match expr {
        RExpr::Number { text, integer, .. } => {
            // `text` is a canonical xsd:decimal, so a whole number carries a `.0` the
            // source never wrote. A LABEL should read back as R, so it is trimmed here —
            // and only here; the emitted literal keeps its lexical form.
            let trimmed = text.strip_suffix(".0").unwrap_or(text);
            if *integer {
                format!("{trimmed}L")
            } else {
                trimmed.to_owned()
            }
        }
        RExpr::Str(s) => format!("\"{s}\""),
        RExpr::Logical(true) => "TRUE".to_owned(),
        RExpr::Logical(false) => "FALSE".to_owned(),
        RExpr::Null => "NULL".to_owned(),
        RExpr::Na => "NA".to_owned(),
        RExpr::NotANumber => "NaN".to_owned(),
        RExpr::Infinity => "Inf".to_owned(),
        RExpr::Ident(name) => name.clone(),
        RExpr::Namespace {
            package,
            name,
            internal,
        } => format!("{package}{}{name}", if *internal { ":::" } else { "::" }),
        RExpr::Call { callee, args } => {
            let rendered: Vec<String> = args
                .iter()
                .map(|a| match (&a.name, &a.value) {
                    (Some(n), Some(v)) => format!("{n} = {}", render(v)),
                    (None, Some(v)) => render(v),
                    (Some(n), None) => n.clone(),
                    (None, None) => String::new(),
                })
                .collect();
            format!("{}({})", render(callee), rendered.join(", "))
        }
        RExpr::Index {
            object,
            args,
            double,
        } => {
            let rendered: Vec<String> = args
                .iter()
                .map(|a| a.value.as_ref().map(render).unwrap_or_default())
                .collect();
            let (open, close) = if *double { ("[[", "]]") } else { ("[", "]") };
            format!("{}{open}{}{close}", render(object), rendered.join(", "))
        }
        RExpr::Component { object, name, slot } => {
            format!("{}{}{name}", render(object), if *slot { "@" } else { "$" })
        }
        RExpr::Unary { op, operand } => format!("{}{}", op.spelling(), render(operand)),
        RExpr::Binary { op, lhs, rhs } => {
            format!("{} {} {}", render(lhs), op.spelling(), render(rhs))
        }
        RExpr::Special { operator, lhs, rhs } => {
            format!("{} {operator} {}", render(lhs), render(rhs))
        }
        RExpr::Pipe { lhs, rhs, native } => format!(
            "{} {} {}",
            render(lhs),
            if *native { "|>" } else { "%>%" },
            render(rhs)
        ),
        RExpr::Formula(f) => render_formula(f),
        RExpr::Assign { target, value, .. } => {
            format!("{} <- {}", render(target), render(value))
        }
        RExpr::Function { params, body } => {
            let rendered: Vec<String> = params
                .iter()
                .map(|p| match &p.default {
                    Some(d) => format!("{} = {}", p.name, render(d)),
                    None => p.name.clone(),
                })
                .collect();
            format!("function({}) {}", rendered.join(", "), render(body))
        }
        RExpr::Block(stmts) => {
            let rendered: Vec<String> = stmts.iter().map(render_statement).collect();
            format!("{{ {} }}", rendered.join("; "))
        }
        RExpr::If {
            condition,
            then_branch,
            else_branch,
        } => match else_branch {
            Some(alternative) => format!(
                "if ({}) {} else {}",
                render(condition),
                render(then_branch),
                render(alternative)
            ),
            None => format!("if ({}) {}", render(condition), render(then_branch)),
        },
        RExpr::For {
            variable,
            sequence,
            body,
        } => format!("for ({variable} in {}) {}", render(sequence), render(body)),
        RExpr::While { condition, body } => {
            format!("while ({}) {}", render(condition), render(body))
        }
        RExpr::Repeat { body } => format!("repeat {}", render(body)),
        RExpr::Break => "break".to_owned(),
        RExpr::Next => "next".to_owned(),
        RExpr::Paren(inner) => format!("({})", render(inner)),
    }
}

/// Render one statement back to compact R-ish source, for `rdfs:label` only.
fn render_statement(statement: &RStmt) -> String {
    match &statement.kind {
        RStmtKind::Assign { target, value, .. } => {
            format!("{} <- {}", render(target), render(value))
        }
        RStmtKind::Expr(expr) => render(expr),
    }
}

fn render_formula(formula: &Formula) -> String {
    let mut s = String::new();
    if let Some(response) = &formula.response {
        s.push_str(&render(response));
        s.push(' ');
    }
    s.push('~');
    let terms: Vec<String> = formula
        .terms
        .iter()
        .map(|t| t.factors.iter().map(render).collect::<Vec<_>>().join(":"))
        .collect();
    if terms.is_empty() {
        s.push_str(" 1");
    } else {
        s.push(' ');
        s.push_str(&terms.join(" + "));
    }
    for removed in &formula.removed {
        s.push_str(" - ");
        s.push_str(
            &removed
                .factors
                .iter()
                .map(render)
                .collect::<Vec<_>>()
                .join(":"),
        );
    }
    if !formula.intercept {
        s.push_str(" - 1");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

    /// The flagship fixture: a real R statistics script.
    const MTCARS: &str = include_str!("../../fixtures/mtcars.R");
    /// A syntactically valid script with no statistical content at all.
    const UNLIFTABLE: &str = include_str!("../../fixtures/unliftable.R");

    fn turtle(src: &str) -> String {
        lift(src.as_bytes(), BASE)
            .unwrap_or_else(|e| panic!("`{src}` must lift: {e}"))
            .turtle
    }

    fn count(ttl: &str, needle: &str) -> usize {
        ttl.matches(needle).count()
    }

    /// How many subjects the graph types as `math:{class}`.
    ///
    /// Exact rather than substring: `Estimate` must not be counted by `Estimator`, nor
    /// `Distribution` by `DistributionFamily`.
    fn typed(ttl: &str, class: &str) -> usize {
        let suffix = format!("{RDF_TYPE_LINE} <{}> .", math(class));
        ttl.lines().filter(|line| line.ends_with(&suffix)).count()
    }

    /// The subjects carrying `predicate`.
    fn subjects_with(ttl: &str, predicate: &str) -> BTreeSet<String> {
        let marker = format!(" <{predicate}> ");
        ttl.lines()
            .filter(|line| line.contains(&marker))
            .filter_map(|line| line.split(' ').next())
            .map(str::to_owned)
            .collect()
    }

    const RDF_TYPE_LINE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";

    #[test]
    fn a_non_utf8_source_is_a_typed_encoding_failure() {
        let err = lift(&[0x66, 0x69, 0x74, 0xff, 0xfe], BASE).expect_err("must not lift");
        assert!(format!("{err}").contains("not valid UTF-8"), "{err}");
    }

    #[test]
    fn a_malformed_script_is_an_rparse_failure() {
        let err = lift(b"fit <- lm(mpg ~ wt, data = mtcars", BASE).expect_err("must not lift");
        assert!(
            format!("{err}").contains("R parse failure at line"),
            "{err}"
        );
    }

    #[test]
    fn the_unliftable_fixture_hard_fails_rather_than_degrading() {
        let err = lift(UNLIFTABLE.as_bytes(), BASE).expect_err("must not lift");
        let text = format!("{err}");
        assert!(text.contains("no statistical content"), "{text}");
        assert!(
            text.contains("routed to logic:"),
            "the diagnostic must say what it DID do: {text}"
        );
    }

    #[test]
    fn a_model_call_without_data_refuses_rather_than_faking_the_restriction() {
        let err = lift(b"fit <- lm(mpg ~ wt)\n", BASE).expect_err("must not lift");
        assert!(format!("{err}").contains("math:fittedToData"), "{err}");
    }

    #[test]
    fn a_model_call_without_a_formula_refuses() {
        let err = lift(b"fit <- lm(y, data = d)\n", BASE).expect_err("must not lift");
        assert!(format!("{err}").contains("math:modelFormula"), "{err}");
    }

    #[test]
    fn the_formula_binder_indexes_the_response_at_zero() {
        let ttl = turtle("fit <- lm(mpg ~ wt + hp, data = mtcars)\n");
        assert!(ttl.contains("ModelFormula"));
        assert!(ttl.contains("BindingExpression"), "the ~ is a binder");
        assert!(ttl.contains("boundVariable"));
        assert_eq!(typed(&ttl, "ArgumentSlot"), 3, "mpg, wt, hp");
        assert_eq!(count(&ttl, "slotIndex"), 3);
        assert!(ttl.contains(r#""0"^^"#), "the response sits at index 0");
        assert_eq!(typed(&ttl, "VariableExpression"), 3);
        // Index 0 holds the response.
        let response_slot = ttl
            .lines()
            .find(|l| l.contains("slotIndex") && l.contains(r#""0"^^"#))
            .expect("a slot at index 0");
        let slot_iri = response_slot.split(' ').next().unwrap_or_default();
        let expression = ttl
            .lines()
            .find(|l| l.starts_with(slot_iri) && l.contains("slotExpression"))
            .and_then(|l| l.split(' ').nth(2))
            .expect("slot 0 has an expression");
        assert!(
            ttl.contains(&format!(
                "{expression} <http://www.w3.org/2000/01/rdf-schema#label> \"mpg\" ."
            )),
            "index 0 must carry the response `mpg`"
        );
    }

    #[test]
    fn a_suppressed_intercept_is_recorded_as_an_explicit_slot() {
        let with = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
        let without = turtle("fit <- lm(mpg ~ wt - 1, data = mtcars)\n");
        assert_eq!(typed(&with, "ArgumentSlot"), 2);
        assert_eq!(
            typed(&without, "ArgumentSlot"),
            3,
            "`- 1` adds an explicit zero intercept slot rather than vanishing"
        );
        assert!(without.contains("NumberLiteral"));
    }

    #[test]
    fn an_interaction_term_lifts_as_an_application_over_its_factors() {
        let ttl = turtle("fit <- lm(y ~ a * b, data = d)\n");
        // Response + a + b + a:b.
        assert_eq!(typed(&ttl, "ModelFormula"), 1);
        assert_eq!(
            typed(&ttl, "ApplicationExpression"),
            1,
            "a:b is an application"
        );
        assert_eq!(
            typed(&ttl, "VariableExpression"),
            3,
            "y, a, b interned once"
        );
        assert_eq!(
            typed(&ttl, "ArgumentSlot"),
            4 + 2,
            "4 formula slots + 2 in a:b"
        );
    }

    #[test]
    fn a_dot_formula_lifts_the_removal_structurally() {
        let ttl = turtle("fit <- lm(y ~ . - x3, data = d)\n");
        assert!(ttl.contains("ApplicationExpression"));
        assert!(
            ttl.contains("dot-expansion") || ttl.contains("Operation"),
            "the `.` is an operator, never a string"
        );
        assert!(
            ttl.contains("VariableExpression"),
            "the removed x3 survives as structure"
        );
    }

    #[test]
    fn the_fitted_model_carries_both_min_one_restrictions() {
        let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
        assert!(ttl.contains("FittedModel"));
        assert!(ttl.contains("modelFormula"));
        assert!(ttl.contains("fittedToData"));
        assert!(ttl.contains("DatasetMatrix"));
    }

    #[test]
    fn the_dataset_is_held_by_reference_and_never_inlined() {
        let ttl =
            turtle("d <- data.frame(x = c(1, 2, 3), y = c(4, 5, 6))\nfit <- lm(y ~ x, data = d)\n");
        assert!(ttl.contains("DatasetMatrix"));
        assert!(
            !ttl.contains("\"4\"") && !ttl.contains("\"5\""),
            "no column payload may reach the graph:\n{ttl}"
        );
    }

    #[test]
    fn a_distribution_call_lifts_family_parameterization_and_roles() {
        let ttl = turtle("draws <- rnorm(100, mean = 0, sd = 1)\n");
        assert!(ttl.contains("Distribution"));
        assert!(ttl.contains("DistributionFamily"));
        assert!(ttl.contains("DistributionParameterization"));
        assert_eq!(typed(&ttl, "Distribution"), 1);
        assert_eq!(typed(&ttl, "DistributionParameterRole"), 2, "mean and sd");
        assert_eq!(
            typed(&ttl, "DistributionParameter"),
            2,
            "n is not a parameter"
        );
        assert!(ttl.contains("requiresPositiveValue"));
        assert!(ttl.contains("hasDimension"));
    }

    #[test]
    fn r_supplies_its_own_documented_defaults_rather_than_dropping_a_role() {
        let ttl = turtle("draws <- rnorm(100)\n");
        assert_eq!(
            typed(&ttl, "DistributionParameterRole"),
            2,
            "mean = 0, sd = 1 are R language semantics, not invention"
        );
    }

    #[test]
    fn a_distribution_missing_a_defaultless_parameter_refuses() {
        let err = lift(b"x <- rpois(10)\n", BASE).expect_err("must not lift");
        assert!(format!("{err}").contains("lambda"), "{err}");
    }

    #[test]
    fn coefficients_lift_to_estimates_with_a_parameter_and_an_estimator() {
        let ttl = turtle("fit <- lm(mpg ~ wt + hp, data = mtcars)\nb <- coef(fit)\n");
        assert_eq!(typed(&ttl, "Estimate"), 3, "(Intercept), wt, hp");
        assert_eq!(typed(&ttl, "Estimator"), 1, "one shared OLS estimator");
        assert_eq!(count(&ttl, "estimatedParameter"), 3);
        assert_eq!(
            count(&ttl, "> <https://blackcatinformatics.ca/math/estimator>"),
            3
        );
        assert!(
            !ttl.contains("estimatesEstimand"),
            "an estimand needs six framing coordinates an R script never states"
        );
    }

    #[test]
    fn every_broom_shaped_coefficient_accessor_reaches_the_same_estimates() {
        for accessor in [
            "b <- coef(fit)",
            "b <- summary(fit)$coefficients",
            "b <- fit$coefficients",
            "b <- broom::tidy(fit)",
        ] {
            let ttl = turtle(&format!("fit <- lm(mpg ~ wt, data = mtcars)\n{accessor}\n"));
            assert_eq!(
                typed(&ttl, "Estimate"),
                2,
                "`{accessor}` produced no estimate"
            );
            assert_eq!(
                typed(&ttl, "Estimator"),
                1,
                "`{accessor}` named no estimator"
            );
        }
    }

    #[test]
    fn residual_accessors_lift_to_a_residual_of_the_fit() {
        for accessor in [
            "r <- residuals(fit)",
            "r <- resid(fit)",
            "r <- fit$residuals",
        ] {
            let ttl = turtle(&format!("fit <- lm(mpg ~ wt, data = mtcars)\n{accessor}\n"));
            assert_eq!(typed(&ttl, "Residual"), 1, "`{accessor}`");
            assert_eq!(count(&ttl, "residualOf"), 1, "`{accessor}`");
        }
    }

    #[test]
    fn a_summary_read_is_a_vantage_held_observation() {
        let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\ns <- summary(fit)\n");
        assert!(ttl.contains("Observation"));
        assert!(ttl.contains("observedFeature"));
        assert!(ttl.contains("vantage"));
        assert!(ttl.contains("Standpoint"));
    }

    #[test]
    fn arithmetic_lifts_to_application_expressions() {
        let ttl = turtle("z <- log(wt) * 2\n");
        assert!(ttl.contains("ApplicationExpression"));
        assert!(ttl.contains("Multiplication"), "* is math:Multiplication");
        assert!(ttl.contains("Logarithm"), "log is math:Logarithm");
        assert!(ttl.contains("NumberLiteral"));
    }

    #[test]
    fn control_flow_routes_to_logic_with_both_co_required_declarations() {
        // Every graph this crate can produce, checked subject by subject: a
        // math:compilesToLogicFormula edge without BOTH declarations is
        // math:UndeclaredLogicLowering.
        for src in [
            "z <- log(wt)\nif (z > 0) {\n  z <- z\n}\n",
            "z <- log(wt)\nfor (i in 1:3) print(i)\n",
            "z <- log(wt)\nwhile (TRUE) break\n",
            "z <- log(wt)\nf <- function(a) a\n",
            "z <- log(wt)\nlabel <- paste0(\"a\", \"b\")\n",
            MTCARS,
        ] {
            let ttl = turtle(src);
            let lowered = subjects_with(&ttl, &math("compilesToLogicFormula"));
            assert!(!lowered.is_empty(), "`{src}` routed nothing to logic:");
            assert_eq!(
                lowered,
                subjects_with(&ttl, &math("denotationKind")),
                "a lowering with no math:denotationKind: {src}"
            );
            assert_eq!(
                lowered,
                subjects_with(&ttl, &math("logicLoweringPreservation")),
                "a lowering with no math:logicLoweringPreservation: {src}"
            );
            assert!(ttl.contains(&logic("SoundUnderApproximation")));
            assert!(ttl.contains(&math("denotesProposition")));
        }
    }

    #[test]
    fn every_lowered_formula_selects_exactly_one_constructor() {
        // logic:FormulaConstructorConstraint: every logic:Formula must select EXACTLY ONE of
        // {and, antecedent, exists, forall, iff, not, or, relation}. A node carrying none is
        // as malformed as one carrying two, and a bare `a logic:Formula` carries none — the
        // shape this lift originally emitted, copied from the hand-authored bridges.ttl
        // template, and caught only once a validate lane finally consumed the output.
        const CONSTRUCTORS: [&str; 8] = [
            "and",
            "antecedent",
            "exists",
            "forall",
            "iff",
            "not",
            "or",
            "relation",
        ];
        for src in [
            "z <- log(wt)\nif (z > 0) {\n  z <- z\n}\n",
            "z <- log(wt)\nfor (i in 1:3) print(i)\n",
            "z <- log(wt)\nwhile (TRUE) break\n",
            "z <- log(wt)\nf <- function(a) a\n",
            MTCARS,
        ] {
            let ttl = turtle(src);
            let formulas = subjects_with(&ttl, &logic("Formula"));
            assert!(!formulas.is_empty(), "`{src}` lowered nothing");
            for formula in &formulas {
                let selected: Vec<&str> = CONSTRUCTORS
                    .iter()
                    .copied()
                    .filter(|c| {
                        ttl.lines().any(|l| {
                            l.starts_with(formula.as_str())
                                && l.contains(&format!("<{}>", logic(c)))
                        })
                    })
                    .collect();
                assert_eq!(
                    selected.len(),
                    1,
                    "<{formula}> selected {selected:?}; exactly one constructor is required\n{ttl}"
                );
            }
        }
    }

    #[test]
    fn a_lowered_atom_carries_a_typed_relation_and_an_indexed_argument() {
        // logic:relation's range is a reified logic:Type; logic:TermCarrierIndexConstraint
        // requires an index on every carrier; logic:TermCarrierValueConstraint requires
        // exactly one term-value kind on it.
        let ttl = turtle("z <- log(wt)\nfor (i in 1:3) print(i)\n");
        assert!(ttl.contains(&format!("<{}> .", logic("Type"))), "{ttl}");
        assert!(
            ttl.contains(&format!("<{}> .", logic("TermCarrier"))),
            "{ttl}"
        );
        assert!(ttl.contains(&format!("<{}>", logic("termIndex"))), "{ttl}");
        assert!(
            ttl.contains(&format!("<{}>", logic("termVariable"))),
            "the loop variable is a real logic: variable term:\n{ttl}"
        );
        // The relation individuals are named for R's own constructs, not invented
        // categories: the loop's membership guard, and the call it makes.
        assert!(
            ttl.contains("r-for-in"),
            "the loop guard names R's own `in`:\n{ttl}"
        );
        assert!(
            ttl.contains("r-call:print"),
            "the call's relation names the callee:\n{ttl}"
        );
    }

    /// Every `logic:Formula` subject reachable in `ttl` that carries `constructor`.
    fn formulas_with_constructor(ttl: &str, constructor: &str) -> BTreeSet<String> {
        subjects_with(ttl, &logic(constructor))
    }

    /// The objects of `<subject> <predicate> ?o`, as raw Turtle terms.
    fn objects_of(ttl: &str, subject: &str, predicate: &str) -> Vec<String> {
        let marker = format!(" <{predicate}> ");
        ttl.lines()
            .filter(|line| line.starts_with(subject) && line.contains(&marker))
            .filter_map(|line| line.split(' ').nth(2))
            .map(str::to_owned)
            .collect()
    }

    /// Every `logic:TermCarrier` value the atom `formula` predicates over, as raw terms.
    fn atom_arguments(ttl: &str, formula: &str) -> Vec<String> {
        let mut values = Vec::new();
        for carrier in objects_of(ttl, formula, &logic("argument")) {
            let carrier = carrier.trim_matches(['<', '>']);
            for kind in [
                "termIri",
                "termVariable",
                "termLiteral",
                "termApplication",
                "termSequenceMarker",
            ] {
                values.extend(objects_of(ttl, &format!("<{carrier}>"), &logic(kind)));
            }
        }
        values
    }

    #[test]
    fn an_if_lowers_to_a_real_implication_over_the_lowered_condition() {
        // The flagship: `if (c) t else e` is `(c → t) ∧ (¬c → e)`, not an opaque node
        // tagged `r-if` whose only content is a truncated label.
        let ttl = turtle(
            "z <- log(wt)\nif (nrow(cars) > 10) {\n  message(\"plenty\")\n} else {\n  \
             warning(\"sparse\")\n}\n",
        );
        let implications = formulas_with_constructor(&ttl, "antecedent");
        assert_eq!(implications.len(), 2, "one implication per arm:\n{ttl}");
        assert_eq!(
            formulas_with_constructor(&ttl, "consequent").len(),
            2,
            "an implication carries both halves"
        );
        assert_eq!(
            formulas_with_constructor(&ttl, "not").len(),
            1,
            "the else arm is guarded by the NEGATED condition"
        );
        assert_eq!(
            formulas_with_constructor(&ttl, "and").len(),
            1,
            "the two arms are conjoined"
        );

        // The condition itself is present as a real comparison over a real nested call —
        // `nrow(cars) > 10`, not a string.
        assert!(ttl.contains("\"r-greater\""), "{ttl}");
        assert!(ttl.contains("\"r-call:nrow\""), "{ttl}");
        assert!(
            ttl.contains(&format!("<{}>", logic("termApplication"))),
            "the nested `nrow(cars)` rides as a compound function term:\n{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{}> .", logic("FunctionTerm"))),
            "{ttl}"
        );
        assert!(
            ttl.contains("\"cars\""),
            "the condition's operand survives:\n{ttl}"
        );
        assert!(ttl.contains("\"10.0\""), "the threshold survives:\n{ttl}");
    }

    #[test]
    fn both_branches_of_the_mtcars_conditional_survive_the_lowering() {
        // The finding this test closes: `grep -c "enough observations" lifted-r.ttl` was 0,
        // and so was `grep -c "message\|warning"`. The whole else arm had vanished.
        let ttl = turtle(MTCARS);
        assert!(
            ttl.contains("enough observations for the interaction term"),
            "the `message(…)` argument must survive:\n{ttl}"
        );
        assert!(
            ttl.contains("the interaction term is underpowered"),
            "the ELSE arm's `warning(…)` argument must survive"
        );
        assert!(ttl.contains("\"r-call:message\""));
        assert!(ttl.contains("\"r-call:warning\""));
    }

    #[test]
    fn no_lowered_label_truncates_the_construct_it_names() {
        // A `…` in a label was the old lowering's only carrier of meaning. Labels remain
        // for readability, but nothing is elided out of one any more.
        let ttl = turtle(MTCARS);
        assert!(
            !ttl.contains('…'),
            "an ellipsis is a truncation, not a lowering:\n{ttl}"
        );
        assert!(
            ttl.contains(
                "if (nrow(cars) > 10) { message(\\\"enough observations for the \
                          interaction term\\\") } else"
            ),
            "the computation node's label renders the WHOLE construct:\n{ttl}"
        );
    }

    #[test]
    fn distinct_callees_never_share_one_relation() {
        // `library(stats)` and `set.seed(20260725)` both used to predicate ONE `r-call`
        // relation, so the formula's predicate said nothing about which call it was.
        let ttl = turtle("z <- log(wt)\nlibrary(stats)\nset.seed(20260725)\n");
        let library = subjects_with(&ttl, &logic("relation"))
            .into_iter()
            .filter_map(|f| {
                let relation = objects_of(&ttl, &f, &logic("relation")).pop()?;
                Some((f, relation))
            })
            .collect::<BTreeMap<_, _>>();
        let relations: BTreeSet<&String> = library.values().collect();
        assert_eq!(
            relations.len(),
            2,
            "two calls, two relations — not one shared `r-call`:\n{ttl}"
        );
        assert!(ttl.contains("\"r-call:library\""), "{ttl}");
        assert!(ttl.contains("\"r-call:set.seed\""), "{ttl}");
        // …and each atom carries its own argument, in order.
        for (formula, _) in library {
            assert_eq!(
                atom_arguments(&ttl, &formula).len(),
                1,
                "each call predicates over its own single argument"
            );
        }
        assert!(ttl.contains("\"stats\""), "{ttl}");
        assert!(ttl.contains("\"20260725.0\""), "{ttl}");
    }

    #[test]
    fn a_nested_call_argument_is_a_real_term_application() {
        let ttl = turtle("z <- log(wt)\nprint(paste0(toupper(label), \"!\"))\n");
        assert!(ttl.contains("\"r-call:print\""));
        assert!(ttl.contains("\"r-call:paste0\""));
        assert!(ttl.contains("\"r-call:toupper\""));
        assert_eq!(
            ttl.matches(&format!("<{}>", logic("FunctionTerm"))).count(),
            2,
            "paste0(…) and toupper(…) are function terms; print(…) is the atom:\n{ttl}"
        );
        assert!(ttl.contains(&format!("<{}>", logic("functionSymbol"))));
        assert!(
            ttl.contains("\"label\""),
            "the innermost variable survives three levels of nesting"
        );
    }

    #[test]
    fn a_for_loop_lowers_to_a_guarded_universal_quantification() {
        let ttl = turtle("z <- log(wt)\nfor (predictor in c(\"disp\", \"wt\")) cat(predictor)\n");
        assert_eq!(
            formulas_with_constructor(&ttl, "forall").len(),
            1,
            "the traversal IS a universal quantification:\n{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{}>", logic("quantifiedVariable"))),
            "the loop variable is bound, not free:\n{ttl}"
        );
        assert_eq!(
            formulas_with_constructor(&ttl, "antecedent").len(),
            1,
            "the body is guarded by sequence membership"
        );
        assert!(ttl.contains("\"r-for-in\""));
        assert!(ttl.contains("\"disp\"") && ttl.contains("\"wt\""), "{ttl}");
    }

    #[test]
    fn a_while_loop_lowers_to_an_implication_over_its_guard() {
        let ttl = turtle("z <- log(wt)\nwhile (remaining > 0) remaining <- remaining - 1\n");
        assert_eq!(formulas_with_constructor(&ttl, "antecedent").len(), 1);
        assert!(ttl.contains("\"r-greater\""), "the guard survives:\n{ttl}");
        assert!(
            ttl.contains("\"r-assign\""),
            "the body's assignment survives:\n{ttl}"
        );
        assert!(ttl.contains("\"remaining\""));
    }

    #[test]
    fn a_function_literal_lowers_over_its_formals_and_its_body() {
        let ttl = turtle("z <- log(wt)\nbanner <- function(text, width = 8) strrep(text, width)\n");
        assert!(ttl.contains("\"r-function\""), "{ttl}");
        assert!(
            ttl.contains("\"r-parameter-default\""),
            "a formal's default is structure, not a dropped token:\n{ttl}"
        );
        assert!(
            ttl.contains("\"r-call:strrep\""),
            "the body survives:\n{ttl}"
        );
        assert!(ttl.contains("\"text\"") && ttl.contains("\"width\""));
        assert!(
            ttl.contains("\"8.0\""),
            "the default value survives:\n{ttl}"
        );
    }

    #[test]
    fn logical_connectives_lower_to_the_logic_connectives() {
        let ttl = turtle("z <- log(wt)\nif (a > 1 && !(b < 2)) print(a)\n");
        assert_eq!(formulas_with_constructor(&ttl, "and").len(), 1, "{ttl}");
        assert_eq!(formulas_with_constructor(&ttl, "not").len(), 1, "{ttl}");
        assert!(ttl.contains("\"r-greater\"") && ttl.contains("\"r-less\""));
    }

    #[test]
    fn every_lowered_term_carrier_selects_exactly_one_value_kind() {
        // logic:TermCarrierValueConstraint: exactly one of termIri / termVariable /
        // termLiteral / termSequenceMarker / termApplication per carrier.
        const KINDS: [&str; 5] = [
            "termIri",
            "termVariable",
            "termLiteral",
            "termSequenceMarker",
            "termApplication",
        ];
        for src in [
            MTCARS,
            "z <- log(wt)\nf <- function(a, ...) a[[1]]\n",
            "z <- log(wt)\nx <- d[, 1]\nplot(y ~ x)\n",
            "z <- log(wt)\nif (is.null(v)) v <- NA else repeat break\n",
        ] {
            let ttl = turtle(src);
            let carriers = subjects_with(&ttl, &logic("termIndex"));
            assert!(!carriers.is_empty(), "`{src}` produced no carrier");
            for carrier in &carriers {
                let selected: Vec<&str> = KINDS
                    .iter()
                    .copied()
                    .filter(|kind| {
                        ttl.lines().any(|l| {
                            l.starts_with(carrier.as_str())
                                && l.contains(&format!("<{}>", logic(kind)))
                        })
                    })
                    .collect();
                assert_eq!(
                    selected.len(),
                    1,
                    "<{carrier}> selected {selected:?}; exactly one value kind is required\n{ttl}"
                );
            }
        }
    }

    #[test]
    fn every_lowered_connective_meets_the_typed_ir_arity_invariant() {
        // crates/logic-compile/src/frontend.rs: `not` exactly one child, `and`/`or` at least
        // two, an implication exactly one antecedent AND one consequent, a quantifier at
        // least one bound variable, an atom at least one argument.
        for src in [
            MTCARS,
            "z <- log(wt)\nif (a) b else c\n",
            "z <- log(wt)\nwhile (a && b) { p(); q() }\n",
            "z <- log(wt)\nif (a || b) next\n",
        ] {
            let ttl = turtle(src);
            for formula in subjects_with(&ttl, &logic("Formula")) {
                for (link, minimum) in [("and", 2), ("or", 2)] {
                    let operands = objects_of(&ttl, &formula, &logic(link)).len();
                    assert!(
                        operands == 0 || operands >= minimum,
                        "<{formula}> logic:{link} has {operands} operand(s)\n{ttl}"
                    );
                }
                for link in ["not", "antecedent", "consequent", "forall", "exists"] {
                    let operands = objects_of(&ttl, &formula, &logic(link)).len();
                    assert!(operands <= 1, "<{formula}> logic:{link} × {operands}");
                }
                let antecedents = objects_of(&ttl, &formula, &logic("antecedent")).len();
                let consequents = objects_of(&ttl, &formula, &logic("consequent")).len();
                assert_eq!(
                    antecedents, consequents,
                    "<{formula}> is half an implication\n{ttl}"
                );
                if !objects_of(&ttl, &formula, &logic("relation")).is_empty() {
                    assert!(
                        !objects_of(&ttl, &formula, &logic("argument")).is_empty(),
                        "<{formula}> is a nullary atomic predication\n{ttl}"
                    );
                }
                if !objects_of(&ttl, &formula, &logic("forall")).is_empty() {
                    assert!(
                        !objects_of(&ttl, &formula, &logic("quantifiedVariable")).is_empty(),
                        "<{formula}> is a vacuous binder\n{ttl}"
                    );
                }
            }
            // Every argument carrier family is zero-based and contiguous.
            for parent in subjects_with(&ttl, &logic("argument")) {
                let mut indexes: Vec<i64> = objects_of(&ttl, &parent, &logic("argument"))
                    .into_iter()
                    .filter_map(|carrier| {
                        let carrier = carrier.trim_matches(['<', '>']);
                        objects_of(&ttl, &format!("<{carrier}>"), &logic("termIndex"))
                            .pop()?
                            .trim_matches('"')
                            .parse()
                            .ok()
                    })
                    .collect();
                indexes.sort_unstable();
                let expected: Vec<i64> = (0..i64::try_from(indexes.len()).unwrap_or(0)).collect();
                assert_eq!(indexes, expected, "<{parent}> argument indexes\n{ttl}");
            }
        }
    }

    #[test]
    fn the_declared_loss_is_enumerated_on_the_source_witness() {
        // Rung::lossy_vague_with_witness() declares a LossyLens. math:unmappedConstruct is
        // what makes that declaration have content: "a lift that declares a rung weaker than
        // logic:ExactPreservation and enumerates nothing is asserting a loss it cannot name".
        let ttl = turtle(MTCARS);
        let witness = subjects_with(&ttl, &math("unmappedConstruct"));
        assert_eq!(witness.len(), 1, "the residue rides on ONE witness:\n{ttl}");
        assert!(
            witness.iter().all(|s| s.contains("r-src-")),
            "…and that witness is math:parseSource's"
        );
        let residue = count(&ttl, &math("unmappedConstruct"));
        assert!(
            residue >= 6,
            "only {residue} construct(s) enumerated:\n{ttl}"
        );
        for expected in [
            "R `for` iteration order",
            // math:unmappedConstruct's own skos:example names exactly these two.
            "R call `message`",
            "R call `warning`",
            "R call `library`",
            "R call `set.seed`",
            "R call `cat`",
        ] {
            assert!(ttl.contains(expected), "residue is missing `{expected}`");
        }
        // …and the rung it qualifies is unchanged.
        assert!(ttl.contains("LossyLens") && ttl.contains("Vague"));
    }

    #[test]
    fn a_multi_statement_block_names_the_ordering_it_loses() {
        let ttl = turtle("z <- log(wt)\nif (a) {\n  p(1)\n  q(2)\n}\n");
        assert_eq!(
            formulas_with_constructor(&ttl, "and").len(),
            1,
            "the block's statements are conjoined:\n{ttl}"
        );
        assert!(
            ttl.contains("R `{ }` statement sequencing"),
            "and the ordering it loses is named:\n{ttl}"
        );
    }

    #[test]
    fn a_script_with_nothing_to_lose_enumerates_no_residue() {
        // An exact-for-this-script lift emits no math:unmappedConstruct, which is itself the
        // claim that nothing was lost — so the property is not a constant.
        let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
        assert_eq!(count(&ttl, &math("unmappedConstruct")), 0, "{ttl}");
    }

    #[test]
    fn an_r_language_constant_rides_as_a_named_individual_with_its_loss_named() {
        let ttl = turtle("z <- log(wt)\nhandle <- ifelse(is.na(x), NULL, x)\n");
        assert!(
            ttl.contains("\"NULL\"") && ttl.contains("\"r-call:is.na\""),
            "{ttl}"
        );
        assert!(
            ttl.contains(&format!("<{}>", logic("termIri"))),
            "a 0-ary constant is an individual, not a nullary function term:\n{ttl}"
        );
        assert!(ttl.contains("zero-length-vector semantics"), "{ttl}");
    }

    #[test]
    fn an_empty_subscript_argument_is_a_sequence_marker_rather_than_a_dropped_slot() {
        let ttl = turtle("z <- log(wt)\ncolumn <- frame[, 2]\n");
        assert!(
            ttl.contains(&format!("<{}>", logic("termSequenceMarker"))),
            "{ttl}"
        );
        assert!(ttl.contains("\"r-subscript\""), "{ttl}");
        assert!(
            ttl.contains("R empty argument"),
            "the loss is named:\n{ttl}"
        );
    }

    #[test]
    fn a_logic_lowering_is_always_generated_by_the_run() {
        // The r-bridge-source-fidelity-and-loss competency query joins on exactly this.
        let lifted = lift(MTCARS.as_bytes(), BASE).expect("the fixture lifts");
        let lowered = subjects_with(&lifted.turtle, &math("compilesToLogicFormula"));
        assert!(!lowered.is_empty());
        let generated = subjects_with(&lifted.turtle, &gmeow("wasGeneratedBy"));
        assert!(
            lowered.is_subset(&generated),
            "the competency query joins ?comp gmeow:wasGeneratedBy ?run with \
             math:compilesToLogicFormula, so every lowering must carry the back edge"
        );
        assert!(
            lifted
                .turtle
                .contains(&format!("{RDF_TYPE_LINE} <{}> .", logic("Formula"))),
            "the lowering target must be a logic:Formula"
        );
    }

    #[test]
    fn the_pipe_forms_reach_the_same_lift_as_the_plain_call() {
        let plain = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
        let piped = turtle("fit <- mtcars %>% lm(mpg ~ wt, data = .)\n");
        assert_eq!(
            count(&plain, "FittedModel"),
            count(&piped, "FittedModel"),
            "a pipe is R syntax and carries no math: content of its own"
        );
        assert!(piped.contains("DatasetMatrix"));
    }

    #[test]
    fn the_mtcars_fixture_lifts_every_expected_codomain_class() {
        let lifted = lift(MTCARS.as_bytes(), BASE).expect("the flagship fixture lifts");
        for class in [
            "RIngestRun",
            "ModelFormula",
            "BindingExpression",
            "ArgumentSlot",
            "VariableExpression",
            "VariableOccurrence",
            "FreeVariableDeclaration",
            "NumberLiteral",
            "FittedModel",
            "DatasetMatrix",
            "Distribution",
            "DistributionFamily",
            "DistributionParameterization",
            "Estimate",
            "Estimator",
            "Residual",
            "ApplicationExpression",
            "Operation",
            "MathematicalExpression",
        ] {
            assert!(
                typed(&lifted.turtle, class) > 0,
                "the mtcars fixture must produce a math:{class}"
            );
        }
        assert!(
            typed(&lifted.turtle, "RIngestRun") == 1,
            "exactly one ingest run"
        );
        assert!(
            lifted
                .turtle
                .contains(&format!("{RDF_TYPE_LINE} <{}> .", logic("Formula")))
        );
        assert!(
            lifted
                .turtle
                .contains(&format!("{RDF_TYPE_LINE} <{}> .", gmeow("Observation")))
        );
        assert!(lifted.codomain_nodes > 20, "a real script is dense");
        assert!(lifted.run_iri.contains("r-run-"));
    }

    #[test]
    fn every_codomain_node_carries_the_back_edge_the_native_lint_reads() {
        let lifted = lift(MTCARS.as_bytes(), BASE).expect("the fixture lifts");
        assert_eq!(
            count(&lifted.turtle, "wasGeneratedBy"),
            lifted.codomain_nodes,
            "exactly one gmeow:wasGeneratedBy per generated node"
        );
    }

    #[test]
    fn a_relift_of_the_same_source_is_byte_identical() {
        let a = lift(MTCARS.as_bytes(), BASE).expect("lifts").turtle;
        let b = lift(MTCARS.as_bytes(), BASE).expect("lifts").turtle;
        assert_eq!(a, b, "the lift is idempotent: no clock, no counter");
    }

    #[test]
    fn a_repeated_subexpression_interns_to_one_node() {
        // `log(wt)` twice: one variable node, one log application, two distinct sums.
        let shared = turtle("u <- log(wt) + 1\nv <- log(wt) + 2\n");
        let distinct = turtle("u <- log(wt) + 1\nv <- log(hp) + 2\n");
        assert_eq!(
            typed(&shared, "VariableExpression"),
            1,
            "one `wt`, mentioned twice:\n{shared}"
        );
        assert_eq!(
            typed(&shared, "ApplicationExpression"),
            3,
            "log(wt), log(wt)+1, log(wt)+2 — the repeated log(wt) collapses"
        );
        assert_eq!(typed(&distinct, "VariableExpression"), 2);
        assert_eq!(
            typed(&distinct, "ApplicationExpression"),
            4,
            "distinct structure DOES grow the fact count"
        );
    }

    #[test]
    fn textual_repetition_alone_does_not_grow_the_graph() {
        let once = lift(b"z <- log(wt) * 2\n", BASE).expect("lifts");
        let thrice = lift(
            b"z <- log(wt) * 2\ny <- log(wt) * 2\nx <- log(wt) * 2\n",
            BASE,
        )
        .expect("lifts");
        assert_eq!(
            once.codomain_nodes, thrice.codomain_nodes,
            "the fact count grows with DISTINCT structure, not textual repetition"
        );
    }

    #[test]
    fn a_lifted_graph_carries_no_private_use_language_tag() {
        let lifted = lift(MTCARS.as_bytes(), BASE).expect("lifts");
        assert!(
            !lifted.turtle.contains("x-gmeow-"),
            "consumer output must not leak a private-use tag"
        );
    }

    #[test]
    fn every_lifted_number_is_a_valid_xsd_decimal() {
        let ttl = turtle("z <- wt * 1e5\n");
        assert!(ttl.contains("100000.0"), "no exponent form in xsd:decimal");
    }

    #[test]
    fn the_run_frame_travels_with_every_lift() {
        let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
        for required in [
            "RIngestRun",
            "parseSource",
            "instantiatesSchema",
            "instantiatesPlan",
            "ingestCorrespondence",
            "LossyLens",
            "mnemomorphic",
        ] {
            assert!(ttl.contains(required), "the frame is missing `{required}`");
        }
    }
}
