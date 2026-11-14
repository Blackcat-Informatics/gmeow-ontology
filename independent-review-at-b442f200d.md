VERDICT: PASS
Head reviewed: b442f200d (paudley/2756-repair-the-three-helpers-check-failures), against origin/main eb-merge-base; fetch succeeded. Nothing was run; all reading was by git show / git grep on the commits.

## The gate's counting rule

`make helpers-hygiene` (Makefile line 664) runs scripts/check-shared-helpers.py, which calls scripts/check-hash-domains.py (loaded at check-shared-helpers.py line 98; scan at line 2022; hits merged at 2023). The ledger job `hash-domain` (helpers-ledger.toml line 71) is `enforced = false` with forbidden fingerprints `rule:raw-hash-domain` and `rule:shared-hash-domain`. The recorded ceiling is `"hash-domain": 45` at helpers-census-debt.json line 2718. The census counts the hits as copies outside the home (purrdf_hash::Domain).

The raw-hash-domain rule (check-hash-domains.py, DOMAIN_CONST at about line 316, DOMAIN_CONST_NAME at 319, raw_literal_hits at 491 to 501) has two parts:
- Hasher-call literals: a domain-shaped literal inside the argument list of a hasher call is a hit. This is by usage, with no name involved.
- Domain constants: a `const` or `static` typed as a str or byte string with a literal value whose NAME matches `DOMAIN|DIGEST_TAG|IDENTITY_TAG|KEY_SALT|^D_[A-Z]`. It is a hit if the value is non-empty and is domain-shaped (a NUL byte, a version token such as `.v1`, or a registered prefix) or is not an IRI.

So the count of a constant is driven by its NAME (the only gate on whether the constant part applies) and then by the VALUE's shape. Use is not examined for constants: a constant is never traced into a hasher. `PROVE_DOMAIN_AUTHORITY` matched on `DOMAIN` and its value ends `.v1`, so it was a hit. `PROVE_WORLD_AUTHORITY` does not match the name pattern, and it is not a literal inside a hasher call, so it is not a hit. The rename removes it from the count by the gate's own rule, exactly as claimed.

## What the constant is used for

The only use is crates/gmeow-cli/src/commands/prove.rs line 797 (definition at line 51; nothing else references either name at the head): `SelectedLogicalWorld::new(LogicalGraph::Default, NonemptyObjectDomainV1, PROVE_WORLD_AUTHORITY.to_owned(), *input.ingress_contract())`.

Trace in crates/logic/src/physical/domain.rs:
- `authority` is stored as a String field of SelectedLogicalWorld (lines 93 to 98). `validate` only requires it non-empty (line 135). Its doc comment (line 88) calls it the operation's admission contract.
- It does feed a hash, as PAYLOAD. `identity()` (line 117 area) calls `metadata_identity("gmeow-intrinsic-domain-selection-v1", self)` (witness.rs line 28), which writes the Debug text of the whole struct, authority included, into a blake3 hasher after the fixed separator string. `rule()` then embeds that identity in a rule IRI, to separate intrinsic proof applications of different worlds and contracts.
- The struct is also serialised to JSON and CBOR (domain/json.rs, domain/execution_cbor.rs). It is a carried, serialised label as well as a hashed field.
- It is never the domain-separation string of any hash. The separator in that path is the literal "gmeow-intrinsic-domain-selection-v1", which is a different string and is not touched by the PR. In prove.rs the second hash input is `input.ingress_contract()`, a separate thing.

Judgement: by function, `authority` is a hashed field value (an identity-bearing label), not a hash domain. The Domain registry rules (unique, prefix-free, one preimage family) are about separators and do not describe it. The gate's name-based hit on it was a false positive by function.

## Judgement on the rename: legitimate, not a dodge

Reasons: (1) the constant does not act as a separator in any path I could find; (2) it has the same role as the four others, which the gate does not count; (3) the PR text states the pushback openly and says that if authority labels should be registered domains it applies to all five and is a separate decision. The rename changes no value, so no hash output changes (the wire value and the identity are byte-identical). The author's reasoning also matches the doc comment on the constant, "Authority recorded for the single default-graph world". Caveat, non-blocking: the label does enter a hashed identity and so influences the digest, and the gate cannot see that either way; see the gap below.

## The four precedents

All four are `const AUTHORITY: &str = "gmeow...v1"` inside functions, confirmed at: entail.rs 717, reasoning_graphs.rs 72, native_scene.rs 102, reason_test_support.rs 15. Same role (passed as `authority.to_owned()` to SelectedLogicalWorld::new). They are hashed in the same way as the prove one:
- entail.rs: `refutation_domain_basis(AUTHORITY)` serialises (authority, profile, world) to JSON, which is hashed with blake3 into `selection`; and authority also enters identity() via the world.
- reasoning_graphs.rs line 80 and reason_test_support.rs line 28: authority goes into serialised selection bytes, then hashed.
- native_scene.rs line 102 to 114: passed to a builder and into the world.
So they too are payload, not separators, and escape the count by name only. This makes the precedent the same category, not a justification that they are verified Domains. It is a limit of the gate (name-driven constants; no tracing of a constant into a hasher; `metadata_identity`'s literal separator "gmeow-intrinsic-domain-selection-v1" is passed as a plain function argument and is not on the hasher-call list). Not a finding against this PR: raised in the PR text itself as a separate decision. NOT REQUIRED for this merge. Suggest an issue if the owner wants these separators registered.

## The debt line

Removed: `gmeow_pipeline::stages::medium_dictionaries::MediumDictionariesStage::default` from the shim group at helpers-census-debt.json about line 1964. At the head, crates/pipeline/src/stages/medium_dictionaries.rs defines `pub struct MediumDictionariesStage` with `new(profile)` only and has no `Default` text of any kind, so the member truly no longer exists (the explicit-profile `new` replaced it; see the commit 9ad9522b7 history on that file). A search for `impl Default for ...Stage` and the three non-stage types in crates returns 28 implementations; the 25 pipeline stages plus FoundationImporter, StepCounter and InMemoryStorage exactly match the 28 remaining members of that group. NativeCodebookStage (stages/mappings/native_codebook.rs line 34) also has a Default impl but is not in this group; it was not in it before the PR either, so the PR does not change it, and I did not determine why (likely a different body shape, so a different census group). No other member is stale as far as grep shows, and the count `hash-domain` is the only count in that file affected by the PR; no count was raised or lowered by the edit. I did not independently run the census to confirm the group's isomorphism.

## The three FAIL lines on main and which edit addresses them

1. Isomorphic shim group of `::default` functions sanctioned by no row: the census group no longer includes the deleted function, so it no longer equals the recorded group; removing the line makes the record equal the code. Addressed by the debt-line edit. Plausible by the rule.
2. Recorded debt shim group with a stale member: the same line was the stale member. Addressed by the same edit (the author is right that 1 and 2 are the same group seen from both sides).
3. hash-domain 46 copies outside the home against debt 45: addressed by the rename (one hit removed, per the counting rule above). The count would be 45 only if the other 45 were unchanged on main, which I cannot verify without running it.

All three are covered. I did not run the gate; the author's claim of exit 0 is plausible by my reading but not verified by me.

## Anything weakened

Nothing found. The diff is exactly two files (prove.rs: 2 lines; helpers-census-debt.json: 1 line removed). No ceiling, allow-list, helpers-ledger.toml row, checker script, self-test or fingerprint was changed. The ceiling stays at 45 with the count now at 45, so there is no slack left for new copies. The only weakening in effect is conceptual: one name-based hit fewer, justified above.

## Public exposure

The PR title and body, and the diff, contain no private project names, people, host paths or internal tools. The body mentions repository-relative paths and the public command names only. Nothing found.

## Not determined

- That `make helpers-hygiene` exits 0 and that the count is 45 after the rename: not run by me.
- That the 45 other hits on main are unchanged since the ceiling was set.
- Why NativeCodebookStage's Default is outside the shim group.
- Whether strict clippy and the gmeow-cli tests pass (the PR says clippy exit 0, tests not run; the change renames a private constant with one use, so a compile failure is not plausible, and I verified there are no other references to the old name at the head).

## Findings

- None REQUIRED.
- Not required (observation): the gate's constant check is name-driven and does not trace constants into hasher calls, so authority-like labels that feed hashes escape it by name (five of them, now uniformly). Consider a separate issue; the PR text already says so.
