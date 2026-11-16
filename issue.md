# Issue #2823: Identities are minted from canonical blank-node labels and move with any relabelling

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, foundational, cross-cutting, quality, complexity-hard

## Body

## Problem

Several places mint identity from the blank-node labels a canonicalization assigns, so those identities change whenever the labelling changes, and under the current labelling they can change when an unrelated statement is added to the same dataset.

- `crates/pipeline/src/transform.rs`, `skolemized_dataset`: each canonical blank label becomes a persisted skolem IRI. The comment says the labels, "hence the persisted skolem IRIs", are stable.
- `crates/logic/src/facts.rs`, `skolem_iri`: the SHA-1 of a blank label, applied to the reasoner's input after `canonicalize_edb` (`crates/pipeline/src/stages/reason.rs`), so reasoning facts carry IRIs derived from canonical labels.
- `crates/logic-compile/src/frontend/prepared.rs`: the logic frontend calls `canonical_relabel_with_mapping`, keeps the mapping to attribute terms to source documents, and stores canonical labels in the compiled IR (`AtomicTerm::Blank`), from where they are reported to reach rule identities and the order of text projections.
- `metadata/releases/term-content-authority.json`: 5,609 committed per-term digests over canonical text that follows blank nodes.
- The GMN-1 codec spells a blank node as its canonical label.
- Committed canonical labels: about 626 occurrences in 119 files, 100 of them logic conformance expected files.

PurRDF is replacing its blank-node labelling in its next minor release. Every one of the above moves with it. Independently of that release, a canonical label is a function of the whole dataset, so an identity built from one is not a property of the node it names.

## Evidence and its limits

The first three sites and the digest count were read on `main`. The reach of labels into rule identities, the GMN codec and the committed-occurrence counts are a reader's findings and were not each re-read. No producer pass was run, so how many shipped identities are affected is not measured.

## Acceptance

- [ ] An inventory of every identity, key, order or committed value that depends on a canonical blank label, with the output each reaches.
- [ ] For each, a decision recorded with its reason: derive the identity from the node's own content (its component), keep it and declare the dependence with the canonical profile version in the identity's domain, or remove it.
- [ ] A compile-time assertion of the canonical profile version this repository was built against, so a change of labelling cannot arrive by a lockfile move alone.
- [ ] The move to the new profile is one change that re-blesses every committed value in the inventory, with a mechanical check that only labels moved.

Principles 4, 7.


## Comments (2)

### coderabbitai — 2026-11-15T18:55:25Z

<!-- coderabbit-bot-generated:v1 -->

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>
<dl>
<dd>
<br>

Blackcat-Informatics/gmeow-ontology#2220 - Complete machinery literal typing and structured tool payloads [closed]
</dd>
</dl>
</details>

---
<details>
<summary>📝 Issue Planner</summary>
<dl>
<dd>
<br>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId":"8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</dd>
</dl>
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>
<dl>
<dd>
<br>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</dd>
</dl>
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!

### paudley — 2026-11-15T19:30:12Z

Further sites found by a second read of `main` at `279ce7e40`, for the inventory this issue asks for. The first three were read directly; the rest are a reader's findings with file and line and have not each been re-read.

Read directly:

- `crates/pipeline/src/stages/carrier.rs`, `canonical_authored_base`: the authored base is canonicalized to N-Quads text and parsed back, only to obtain a dataset with canonical labels.
- `crates/lang-bridge/src/gmn1_witness.rs`, `partition_by_subject`: canonical N-Quads lines are parsed for their leading subject term to group them into per-claim digests.
- `crates/logic-compile/src/relational_core/identity.rs`: a content key is framed from canonical text.

Reported:

- `AtomicTerm::Blank` holds a canonical label as a string, is ordered by that string, is printed verbatim into the Datalog, N3, CLIF and TPTP projections, and one constructor infers a term's kind from the shape of a string.
- A null is recognized by a substring test on a displayed IRI.
- Conformance comparison, the run drift check, the YAML-LD check and the math producers split canonical text into lines and compare or sort them; a sweep takes the first lines of the sorted text.
- The snapshot stratum's writer and reader canonicalize through different entries and agree only while those two byte streams are equal.
- A marker statement is added so that declared but empty graphs are visible to canonicalization.

Each of these either builds meaning from a string at the canonicalization boundary or depends on canonical text beyond hashing it. PurRDF's next minor release is expected to return typed lines, a stable identity per blank node and a typed map; each site above has a direct typed replacement.


