# Independent review of PR 2779 at `fdca3d769` (the scope fan-out)

Saved by the primary session from the report of reviewer `a43ef2e63acffa3c2` (2026-11-14), condensed in
wording only; the full report is in the session transcript. By reading only; nothing built or run. The
reviewer reviewed the commit, not the uncommitted edits it saw in the worktree.

## Scope

Read at the head: `crates/logic-compile/src/frontend.rs` (`scope_values`, `ScopeDraft`, `fan_out`,
`scope_from_node` 208-413; `extract_scoped_axioms` 1395-1547; `read_reified_axiom` 2028-2063;
`extract_rules` 2086-2251; `compile_source_graph` 6944-7133); `frontend/source_admission.rs` (the diff,
`admit_rules_and_owners`, `admit_remaining_axioms`, `rule_disposition`, `scope_is_flat`, `node_anchor`); all
of `frontend/native_statement_tests.rs`; `docs/PIPELINE_SPINE.md` 205-235; the `logic:provenance`,
`logic:inModule` and `logic:confidence` notes in the logic slice; both catalog diffs. Read at the base:
`frontend/emission.rs`, `ir.rs`, `graphutil.rs`, `frontend/presentation.rs`, `relational_core.rs`,
`relational_core/identity.rs`, `projections/rdf.rs`, `logic/src/lower.rs`; `gmeow-cli` `prove.rs` by search;
`pipeline/src/stages/{meta_findings,gate_verdict,compile_logic}.rs`.

Not read: the TPTP writer and Datalog internals beyond search; other runtime consumers of `rule.scope`.

## Evidence (summary)

One value per field behaves as before; nothing accepted before is refused; no weaker form is accepted; no
`HashMap` or `HashSet` iteration; fan-out order is term-text order. All four callers updated; axioms are
all-or-nothing per node; axiom deduplication includes scope; each fanned rule gets its own owner lowering
and index. The "recorded once" rule in source admission cannot hide a real second unit (collapse needs the
same source node, documents and disposition); admitted rules stay separate; `prove.rs` is unaffected. No
mention of the removed code remains; catalog `msgid`s equal the English; translations faithful.

## Findings

REQUIRED 1. `crates/pipeline/src/stages/meta_findings.rs` 344-367: a `BTreeMap<SourceNode, usize>` collected
over Rule owner lowerings; with the fan-out the last index overwrites, so a diagnostic meta-rule node with
several values of a coordinate, refused loudly before, silently folds one lowered rule and drops the rest.
Refuse such a node or fold every rule; add a test.

REQUIRED 2. Confidence and modality are counted by text (`frontend.rs` 208-243, 347-357): `"0.5"`, `"0.50"`
and `"5e-1"` are three values, as are spellings of one modality; identical scopes are emitted and a node is
wrongly refused as ambiguous. Related: two different terms with the same text collapse to whichever the
store returns first.

REQUIRED 3. Named behaviour with no test: (a) each lowered claim or rule keeps its source node in the
emission evidence (the test named "...one_source" does not assert it); (b) the classic `rdf:Statement`
carrier path. Also untested: two multi-valued coordinates on a rule node; an invalid value among several on
a rule node; a rule fanned out on a coordinate other than provenance; the default modality beside another;
both authoring orders for rules; values in different physical tables.

OPTIONAL: an early return loses a second diagnostic on the same node; the ambiguity message and usage note
say "one reifier" where a rule node needs "one rule"; the default scope is dropped in the claim paths
(consistent, worth a note or a test); confidence order by text; rule units collapse per node while axiom
units stay one per index.

VERDICT: BLOCKED fdca3d769
