<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — References

> Status: bibliography appendix for the [GMEOW Logic design set](LOGIC.md#the-document-set). The
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
  *ingested* surface accepted by the reasoning pipeline (see LOGIC-CONFORMANCE.md).
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

## Engines and tools (`intentBridgedByReference`)

- Oxigraph — an RDF store with SPARQL and RDF 1.2 support (Rust). <https://oxigraph.org/>
- Nemo — a Datalog-based rule engine with existential rules and stratified negation (knowsys, Rust).
  <https://knowsys.github.io/nemo/>
- Soufflé — Jordan, H., Scholz, B. & Subotić, P. (2016). *Soufflé: On Synthesis of Program
  Analyzers.* CAV. <https://souffle-lang.github.io/>
- RDFox — Nenov, Y., Piro, R., Motik, B., Horrocks, I., Wu, Z. & Banerjee, J. (2015). *RDFox: A
  Highly-Scalable RDF Store.* ISWC. (Cited as prior art only; not a dependency — Principle 5.)
- Scryer Prolog — an ISO Prolog system in Rust. <https://www.scryer.pl/>
- Trealla Prolog — a compact ISO Prolog in C/Rust bindings. <https://trealla-prolog.github.io/>
- ProbLog — De Raedt, L., Kimmig, A. & Toivonen, H. (2007). *ProbLog: A Probabilistic Prolog and Its
  Application in Link Discovery.* IJCAI.
- EYE — the Euler Yet another proof Engine (N3 reasoning). <https://eyereasoner.github.io/eye/>
- cwm — the Closed World Machine (N3 rules). <https://www.w3.org/2000/10/swap/doc/cwm.html>
- ELK — Kazakov, Y., Krötzsch, M. & Simančík, F. (2014). *The Incredible ELK.* Journal of Automated
  Reasoning 53.
- HermiT — Glimm, B., Horrocks, I., Motik, B., Stoilos, G. & Wang, Z. (2014). *HermiT: An OWL 2
  Reasoner.* Journal of Automated Reasoning 53.
- OWL-RL — a Python OWL 2 RL/RDFS reasoner (cross-check oracle). <https://owl-rl.readthedocs.io/>
- PyO3 / maturin — Rust bindings for Python. <https://pyo3.rs/>
- WebAssembly. <https://webassembly.org/>

## Design influence (`intentCitesAsDataSource`)

- Quijada, J. (2011). *A Grammar of the Ithkuil Language.* <https://ithkuil.net/> (Orthogonal
  factorization; obligatory evidentiality; precision without a usable surface.)
