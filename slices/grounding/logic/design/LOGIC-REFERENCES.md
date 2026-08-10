<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — References

> The bibliography appendix for the [GMEOW Logic design set](LOGIC.md#the-document-set). The
> `logic:` design relies on a substantial body of external standards, foundational theory, and
> engines; this appendix names them so the claims in the other documents are *cited*, not merely
> name-dropped.

## Relationship to the citation ledger

Per [`docs/CITATIONS.md`](../../../../docs/CITATIONS.md), the **canonical** citation
record is the Turtle ledger `metadata/references.ttl`; Markdown, CSL JSON, and BibTeX are generated
lossy projections (Principle 4). This appendix is therefore a **staging surface**: correct,
identifiable references with stable URLs that `gmeow references-backfill` harvests from authored
files, after which precise DOIs and venue metadata are curated into the ledger. When the ledger
lands, each entry here becomes a `gmeow:CreativeWork` cited by a `gmeow:CitationAct`, with
`gmeow:viaSelector` pointing at the design file (and section) that cites it. The two groupings below
map directly to the two citation intents:

- **Standards, vocabularies, and engines** → `gmeow:intentBridgedByReference` (aligned to or built
  on, never copied in — Principle 5).
- **Foundational theory** → `gmeow:intentCitesAsDataSource` (relied on as the source for a design
  claim — AGM revision, decidability-as-projection, the chase, and so on).

Internal references (`GTS conformance design`, `references ledger`, the statement layer, the slices) are **not** bibliography items
unless they themselves cite an external work; they are cross-references within GMEOW.

## Standards and specifications (`intentBridgedByReference`)

- W3C. *RDF 1.2 Concepts and Abstract Syntax.* <https://www.w3.org/TR/rdf12-concepts/>
  (Full vs Basic conformance; triple terms in object position).
- W3C. *OWL 2 Web Ontology Language — Document Overview* (2012) and *Profiles* (EL, QL, RL).
  <https://www.w3.org/TR/owl2-overview/>, <https://www.w3.org/TR/owl2-profiles/>
- W3C. *Shapes Constraint Language (SHACL)* (2017). <https://www.w3.org/TR/shacl/>
- W3C. *SPARQL 1.1 Query Language* (2013). <https://www.w3.org/TR/sparql11-query/>
- W3C. *SWRL: A Semantic Web Rule Language Combining OWL and RuleML* (Member Submission, 2004).
  <https://www.w3.org/submissions/SWRL/>
- W3C. *RIF (Rule Interchange Format) Overview* (2013). <https://www.w3.org/TR/rif-overview/>
- ISO/IEC 24707:2018. *Information technology — Common Logic (CL).* Three normative dialects: CLIF
  (Common Logic Interchange Format — first-order textual syntax), CGIF (Conceptual Graph Interchange
  Format — graph-based notation), and XCL (XML-based Common Logic notation). Common Logic is
  operationalized in this design as both a *generated* output dialect (see LOGIC.md) and an
  *ingested* surface accepted by the reasoning pipeline (see LOGIC-CONFORMANCE.md), and the CLIF
  export is independently cross-checked by an external first-order reasoner run as a standalone
  validator-zoo lane outside the Docker-free gate (`validations/common-logic-fol/`).
- ISO/IEC 21838-2:2021. *Basic Formal Ontology (BFO).* See also Arp, Smith & Spear, *Building
  Ontologies with Basic Formal Ontology* (MIT Press, 2015).
- Berners-Lee, T., Connolly, D., Kagal, L., Scharf, Y. & Hendler, J. (2008). *N3Logic: A logical
  framework for the World Wide Web.* Theory and Practice of Logic Programming 8(3).

## Foundational ontologies (`intentBridgedByReference`; cited as source for UFO⁺)

- Guizzardi, G. (2005). *Ontological Foundations for Structural Conceptual Models* (UFO). PhD thesis,
  University of Twente.
- Almeida, J. P. A., Falbo, R. A. & Guizzardi, G. *gUFO: A Lightweight Implementation of the Unified
  Foundational Ontology.* <https://nemo-ufes.github.io/gufo/>
- OntoUML — Guizzardi et al.; anti-pattern catalogue: <https://ontouml.readthedocs.io/>
- Masolo, C., Borgo, S., Gangemi, A., Guarino, N. & Oltramari, A. (2003). *Ontology Library
  (WonderWeb Deliverable D18)* — DOLCE; Gangemi, A. & Mika, P. (2003). *Understanding the Semantic
  Web through Descriptions and Situations (DnS).* ODBASE.
- Niles, I. & Pease, A. (2001). *Towards a Standard Upper Ontology (SUMO).* FOIS.
- Mizoguchi, R. (2010). *YAMATO: Yet Another More Advanced Top-level Ontology.* Hozo ontology
  library: <https://www.hozo.jp/onto_library/YAMATO101216.pdf>. (Bridge view + refinement source:
  persistent quality identity, generic-quality→role ladder, unit-independent true quantity,
  process≠event, action/event open-closed, causal-vs-temporal parts — see
  [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md) and `docs/foundational-bridging.md`.)
- Lenat, D. B. & Guha, R. V. (1990). *Building Large Knowledge-Based Systems: Representation and
  Inference in the Cyc Project.* Addison-Wesley. (Bridge view source: CycL microtheories as prior
  art for context/standpoint indexing, ordered by generality via `genlMt` — see
  [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md).)

### Realized grounding disposition

The survey has a concrete, fail-closed disposition; “listed here” is not an
implicit promise to add a mapping later.

| Family | Realized disposition |
|---|---|
| gUFO, BFO 2020, OBO/RO, SUMO, OWL/RDFS, SHACL Core/AF | Shipped `logic:GroundingCorrespondence` catalogs in `mappings/grounding-bridges.ttl` |
| DUL, IAO, OBI, PATO, YAMATO 2021-08-08, OpenCyc 2012-05-10 | Shipped commitment-shifting, validation-only bridge catalog in `mappings/foundation-bridges.ttl`; target axioms remain by-reference |
| P-Plan, PROV-O, schema.org HowTo/Recipe, OPMW, BPMN, RO-Crate Workflow-Run, Airflow/CWL/WDL/Temporal/Nextflow, openEHR Task Planning | Shipped commitment-shifting, validation-only process-model bridge catalog in `mappings/plan-enactment-bridges.ttl`; every engine surface is a by-reference lowering of `logic:Plan` with its structured lossy drops |
| OntoUML | Shipped executable, single-binding grounding correspondences in `mappings/projections-ontouml.ttl` |
| Sequence Ontology (`SO_`) and Emotion Ontology (`MFOEM_`) | Citation and domain-lineage only here; neither has a foundation-term identity warrant, so no synthetic `logic:` bridge is asserted |

OpenCyc rows use permanent `http://sw.opencyc.org/concept/` identifiers from
<https://github.com/therohk/opencyc-kb>. YAMATO rows use the version-pinned
`http://www.hozo.jp/owl/YAMATO20210808.miz.owl#` namespace. The complete row
ledger and morphism policy are in
[`docs/foundational-bridging.md`](../../../../docs/foundational-bridging.md).

## Foundational theory (`intentCitesAsDataSource`)

- Alchourrón, C., Gärdenfors, P. & Makinson, D. (1985). *On the Logic of Theory Change: Partial Meet
  Contraction and Revision Functions.* Journal of Symbolic Logic 50(2). (AGM belief revision; the
  Revision facet of the logic design — see LOGIC-FOUNDATION.md.)
- Gärdenfors, P. & Makinson, D. (1988). *Revisions of Knowledge Systems Using Epistemic
  Entrenchment.* TARK. (Entrenchment ↔ revision; deterministic revision.)
- Lewis, D. (1973). *Counterfactuals.* Harvard University Press. (Closeness ordering; ties.)
- Stalnaker, R. (1968). *A Theory of Conditionals.* In *Studies in Logical Theory.* (Unique closest
  world.)
- Ramsey, F. P. (1931). *General Propositions and Causality.* (The Ramsey test.)
- Church, A. (1936). *An Unsolvable Problem of Elementary Number Theory.* American Journal of
  Mathematics 58. — Turing, A. M. (1936). *On Computable Numbers, with an Application to the
  Entscheidungsproblem.* Proc. London Math. Soc. (Undecidability / the halting problem.)
- Blackburn, P., de Rijke, M. & Venema, Y. (2001). *Modal Logic.* Cambridge University Press. (The
  standard translation of modal logic into first-order logic.)
- Belnap, N. D. (1977). *A Useful Four-Valued Logic.* In Dunn, J. M. & Epstein, G. (Eds.), *Modern
  Uses of Multiple-Valued Logic.* Reidel. — Belnap, N. D. (1977). *How a Computer Should Think.* In
  Ryle, G. (Ed.), *Contemporary Aspects of Philosophy.* Oriel Press. (Four-valued / first-degree
  entailment (FDE) logic: the four truth-values True, False, Both, Neither underpin the
  information-status lattice for reasoning results — see LOGIC-SEMANTICS.md.)
- Gelfond, M. & Lifschitz, V. (1988). *The Stable Model Semantics for Logic Programming.* ICLP.
- Van Gelder, A., Ross, K. & Schlipf, J. (1991). *The Well-Founded Semantics for General Logic
  Programs.* Journal of the ACM 38(3).
- Bonner, A. J. & Kifer, M. (1993). *Transaction Logic Programming.* ICLP. — Bonner, A. J. & Kifer,
  M. (1995). *An Overview of Transaction Logic.* Theoretical Computer Science 133(2). (State-change
  semantics for sequential and concurrent actions within logic programs; grounds the state-change
  facet of the transaction layer — see LOGIC-TRANSACTION.md.)
- Chen, W., Kifer, M. & Warren, D. S. (1993). *HiLog: A Foundation for Higher-Order Logic
  Programming.* Journal of Logic Programming. (Second-order-as-first-order reification.)
- Kifer, M., Lausen, G. & Wu, J. (1995). *Logical Foundations of Object-Oriented and Frame-Based
  Languages (F-logic).* Journal of the ACM 42(4).
- Fagin, R., Kolaitis, P., Miller, R. & Popa, L. (2005). *Data Exchange: Semantics and Query
  Answering.* Theoretical Computer Science 336. (The chase; weak acyclicity / termination.)
- Calì, A., Gottlob, G. & Lukasiewicz, T. (2012). *A general Datalog-based framework for tractable
  query answering over ontologies* (Datalog±). Journal of Web Semantics.
- Bancilhon, F., Maier, D., Sagiv, Y. & Ullman, J. (1986). *Magic Sets and Other Strange Ways to
  Implement Logic Programs.* PODS. (Goal-directed materialization.)
- Chen, W. & Warren, D. S. (1996). *Tabled Evaluation with Delaying for General Logic Programs.*
  Journal of the ACM 43(1). (SLG resolution / tabling.)
- Green, T., Karvounarakis, G. & Tannen, V. (2007). *Provenance Semirings.* PODS. (Why-provenance;
  proof-trace provenance.)
- Bry, F. (1990). *Query Evaluation in Recursive Databases: Bottom-Up and Top-Down Reconciled.*
  Data & Knowledge Engineering 5(4). (Magic sets and tabling as one Backward Fixpoint Procedure —
  the demand doctrine's one-engine-two-directions claim, see LOGIC-PERFORMANCE.md.)
- Tekle, K. T. & Liu, Y. A. (2011). *More Efficient Datalog Queries: Subsumptive Tabling Beats
  Magic Sets.* SIGMOD. (Subsumptive demand transformation — the sanctioned goal-directed rewrite,
  see LOGIC-PERFORMANCE.md.)
- Cuenca Grau, B., Horrocks, I., Krötzsch, M., Kupke, C., Magka, D., Motik, B. & Wang, Z. (2013).
  *Acyclicity Notions for Existential Rules and Their Application to Query Answering in
  Ontologies.* JAIR 47. (The chase-termination ladder — joint/super-weak acyclicity, MSA, MFA and
  the critical-instance check, see LOGIC-PERFORMANCE.md.)
- Carral, D., Dragoste, I. & Krötzsch, M. (2017). *Restricted Chase (Non)Termination for
  Existential Rules with Disjunctions.* IJCAI. — Krötzsch, M. et al. (2023). *Do Repeat Yourself:
  Understanding Sufficient Conditions for Restricted Chase Non-Termination.* KR. (Restricted-chase
  acyclicity refinements and the repaired non-termination criteria; the caution on non-termination
  verdicts.)
- Motik, B., Nenov, Y., Piro, R. & Horrocks, I. (2015). *Incremental Update of Datalog
  Materialisation: the Backward/Forward Algorithm.* AAAI. (Delete-heavy incremental maintenance —
  the fallback beside the Z-set circuits.)
- Ngo, H. Q., Porat, E., Ré, C. & Rudra, A. (2012). *Worst-Case Optimal Join Algorithms.* PODS /
  JACM 65(3) (2018). — Wang, Y. R., Willsey, M. & Suciu, D. (2023). *Free Join: Unifying
  Worst-Case Optimal and Traditional Joins.* SIGMOD. (WCOJ theory and the hybrid binary/multiway
  operator with column-oriented lazy tries — the join doctrine's cyclic-sub-plan lever.)
- McSherry, F., Lattuada, A., Schwarzkopf, M. & Roscoe, T. (2020). *Shared Arrangements: Practical
  Inter-Query Sharing for Streaming Dataflows.* PVLDB 13(10). (Sorted immutable batches with
  amortized merging — the data-shape doctrine's relation substrate.)
- Zhao, D., Subotić, P. & Scholz, B. (2020). *Debugging Large-Scale Datalog: A Scalable Provenance
  Evaluation Strategy.* ACM TOPLAS 42(2). (Minimal-proof-height annotations with lazy proof-tree
  reconstruction — the provenance doctrine's Record-mode cost bound.)
- Deutch, D., Milo, T., Roy, S. & Tannen, V. (2014). *Circuits for Datalog Provenance.* ICDT.
  (Absorptive semirings admit compact provenance circuits where full polynomial lineage does not.)
- Hu, X., Zhao, D., Jordan, H. & Scholz, B. (2021). *An Efficient Interpreter for Datalog by
  De-specializing Relations.* PLDI. (A well-built plan interpreter sits within a small constant
  factor of synthesized code — why machine-code generation stays in reserve, see
  LOGIC-PERFORMANCE.md.)
- Benedikt, M. et al. (2017). *Benchmarking the Chase.* PODS. (ChaseBench — the external corpus
  the measurement doctrine's existential-fragment lanes run on.)
- de Kleer, J. (1986). *An Assumption-Based TMS.* Artificial Intelligence 28. — Doyle, J. (1979). *A
  Truth Maintenance System.* Artificial Intelligence 12. (Contradiction witnesses / ATMS·JTMS.)
- Dung, P. M. (1995). *On the Acceptability of Arguments and Its Fundamental Role in Nonmonotonic
  Reasoning, Logic Programming and n-Person Games.* Artificial Intelligence 77(2). (Abstract
  argumentation frameworks; acceptability semantics (grounded, preferred, stable extensions) for
  defeasible inference — see LOGIC-FOUNDATION.md.)
- Koestler, A. (1967). *The Ghost in the Machine.* Hutchinson. (Holon / holarchy concept: every
  whole is simultaneously a part of a larger whole; grounds the contextual mereology of the logic
  layer's module and context hierarchy — see LOGIC-FOUNDATION.md.)
- Bratman, M. E. (1987). *Intention, Plans, and Practical Reason.* Harvard University Press. (The
  belief–desire–intention account: intentions as commitments distinct from desire, and plans as
  partial, hierarchical structures refined toward action; grounds the commitment-graded modes and
  goal decomposition of the goal-and-action layer — see LOGIC-TELEOLOGY.md.)
- von Wright, G. H. (1951). *Deontic Logic.* Mind 60(237). (The modal treatment of obligation,
  permission, and prohibition; grounds the deontic force that ranges over goals — see
  LOGIC-TELEOLOGY.md.)
- Peirce, C. S. (1903). *Pragmatism as a Principle and Method of Right Thinking* (the Harvard
  Lectures on Pragmatism). (Abduction as a third inference mode beside deduction and induction;
  grounds the abductive quality criterion of cognitive assessment — see LOGIC-COGNITION.md.)
- Toulmin, S. (1958). *The Uses of Argument.* Cambridge University Press. (Claim, data, warrant,
  backing, rebuttal; grounds the warrant-and-defeater structure reasoning quality is judged against
  — see LOGIC-COGNITION.md.)
- Pollock, J. L. (1987). *Defeasible Reasoning.* Cognitive Science 11(4). (Rebutting versus
  undercutting defeaters; grounds the defeater-kind axis of reasoning quality — see
  LOGIC-COGNITION.md.)
- Gentner, D. (1983). *Structure-Mapping: A Theoretical Framework for Analogy.* Cognitive Science
  7(2). (Systematicity as the criterion of a good analogical mapping; grounds the analogical quality
  criterion — see LOGIC-COGNITION.md.)
- Brier, G. W. (1950). *Verification of Forecasts Expressed in Terms of Probability.* Monthly Weather
  Review 78(1). (The Brier score; grounds the calibration-error measurement of cognitive assessment
  — see LOGIC-COGNITION.md.)
- Foster, J. N., Greenwald, M. B., Moore, J. T., Pierce, B. C. & Schmitt, A. (2007). *Combinators
  for Bidirectional Tree Transformations: A Linguistic Approach to the View-Update Problem.* ACM
  TOPLAS 29(3). (Asymmetric lenses; the GetPut/PutGet/PutPut laws — the correspondence get/put legs,
  see LOGIC-CORRESPONDENCE.md.)
- Hofmann, M., Pierce, B. & Wagner, D. (2011). *Symmetric Lenses.* POPL. (The complement object — the
  in-band complement materialized in the OpenEHR subsumption case.)
- Diskin, Z., Xiong, Y. & Czarnecki, K. (2011). *From State- to Delta-Based Bidirectional Model
  Transformations.* (Edit/delta lenses; traces — the mnemomorphism witness, deferred extension.)
- Pickering, M., Gibbons, J. & Wu, N. (2017). *Profunctor Optics: Modular Data Accessors.* The Art,
  Science, and Engineering of Programming 1(2). (The optic lattice iso/lens/prism/affine/traversal —
  the correspondence-class taxonomy / law-spine.)
- Meijer, E., Fokkinga, M. & Paterson, R. (1991). *Functional Programming with Bananas, Lenses,
  Envelopes and Barbed Wire.* FPCA. — Uustalu, T. & Vene, V. (1999). *Primitive (Co)Recursion and
  Course-of-Value (Co)Iteration.* (Recursion schemes — paramorphism/histomorphism, the lineage the
  *mnemomorphism* coins from; the recursion-scheme name is dropped, the witness idea kept.)
- Cousot, P. & Cousot, R. (1977). *Abstract Interpretation: A Unified Lattice Model for Static
  Analysis of Programs by Construction or Approximation of Fixpoints.* POPL. (Galois connections —
  the sound under/over-approximation that *is* the preservation polarities.)
- Goguen, J. A. & Burstall, R. M. (1992). *Institutions: Abstract Model Theory for Specification and
  Programming.* JACM 39(1). (Institution (co)morphisms — truth-preserving morphism vs commitment-
  shifting bridge view; the gUFO-vs-BFO/DOLCE split.)
- Lenzerini, M. (2002). *Data Integration: A Theoretical Perspective.* PODS. (GAV/LAV/GLAV and
  certain answers — deriving the `put` leg by query rewriting.)
- Spivak, D. I. (2012). *Functorial Data Migration.* Information and Computation 217. (The Δ/Σ/Π
  adjoint triple for schema mappings.)
- Schürr, A. (1994). *Specification of Graph Translators with Triple Graph Grammars.* WG. (The
  correspondence graph as a first-class link node.)

## Engines and tools (`intentBridgedByReference`)

- Oxigraph — an RDF store with SPARQL and RDF 1.2 support (Rust). <https://oxigraph.org/>
- Soufflé — Jordan, H., Scholz, B. & Subotić, P. (2016). *Soufflé: On Synthesis of Program
  Analyzers.* CAV. <https://souffle-lang.github.io/>
- RDFox — Nenov, Y., Piro, R., Motik, B., Horrocks, I., Wu, Z. & Banerjee, J. (2015). *RDFox: A
  Highly-Scalable RDF Store.* ISWC. (Cited as prior art only; not a dependency — Principle 5.)
- Trealla Prolog — a compact ISO Prolog in C/Rust bindings. <https://trealla-prolog.github.io/>
- ProbLog — De Raedt, L., Kimmig, A. & Toivonen, H. (2007). *ProbLog: A Probabilistic Prolog and Its
  Application in Link Discovery.* IJCAI.
- EYE — the Euler Yet another proof Engine (N3 reasoning). <https://eyereasoner.github.io/eye/>
- cwm — the Closed World Machine (N3 rules). <https://www.w3.org/2000/10/swap/doc/cwm.html>
- OWL-RL — a Python OWL 2 RL/RDFS reasoner (cross-check oracle). <https://owl-rl.readthedocs.io/>
- PyO3 / maturin — Rust bindings for Python. <https://pyo3.rs/>
- WebAssembly. <https://webassembly.org/>
- MLIR — Lattner, C. et al. (2021). *MLIR: Scaling Compiler Infrastructure for Domain Specific
  Computation.* CGO. (Dialects, per-op verifiers, progressive lowering, dialect conversion /
  legalization — the architecture the IR + execution stack borrows; the substrate is not.)
- LLVM — Lattner, C. & Adve, V. (2004). *LLVM: A Compilation Framework for Lifelong Program Analysis
  & Transformation.* CGO. (Mechanisms: the verifier, droppable `!metadata` vs load-bearing operand
  bundles, debug-info-through-optimization — the `logic:loadBearing` and validation patterns.)
- Lopes, N. P., Lee, J., Hur, C.-K., Liu, Z. & Regehr, J. (2021). *Alive2: Bounded Translation
  Validation for LLVM.* PLDI. (Refinement-checking transform validation — the overclaim/round-trip
  gate methodology.)
- Willsey, M., Nandi, C., Wang, Y. R., Flatt, O., Tatlock, Z. & Panchekha, P. (2021). *egg: Fast and
  Extensible Equality Saturation.* POPL (Rust). — Zhang, Y. et al. (2023). *Better Together: Unifying
  Datalog and Equality Saturation* (egglog). PLDI. (Plan normalization + the Datalog/e-graph
  execution substrate.)
- McSherry, F., Murray, D. G., Isaacs, R. & Isard, M. (2013). *Differential Dataflow.* CIDR (Rust:
  timely/differential-dataflow). — Ryzhyk, L. & Budiu, M. (2019). *Differential Datalog (DDlog).*
  Datalog 2.0. (Incremental recursive evaluation — the re-reason-after-edit lever.)
- Budiu, M., McSherry, F., Ryzhyk, L. & Tannen, V. (2023). *DBSP: Automatic Incremental View
  Maintenance for Rich Query Languages.* VLDB (Feldera, Rust). (Incremental view maintenance for the
  relational core.)
- Ngo, H. Q., Ré, C. & Rudra, A. (2013). *Skew Strikes Back: New Developments in the Theory of Join
  Algorithms.* SIGMOD Record. — Veldhuizen, T. L. (2014). *Leapfrog Triejoin: A Simple,
  Worst-Case-Optimal Join Algorithm.* ICDT. (WCOJ for cyclic graph patterns.)

## Design influence (`intentCitesAsDataSource`)

- Quijada, J. (2011). *A Grammar of the Ithkuil Language.* <https://ithkuil.net/> (Orthogonal
  factorization; obligatory evidentiality; precision without a usable surface.)
