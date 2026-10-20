# Retirement oracle identity repair

Active scope: repair the raw-SPARQL identity witness used by the shared shape
retirement work in issues #1197, #1446 and #1452 (Principles 4, 7, 17 and 18).

The hand-written tokenizer collapses meaningful whitespace and case inside
literal values. The old block comparison also ignores severity, messages and
deactivation. Either defect can falsely authorize deletion.

Use PurRDF's parser, checked serializer, lexer and native SHACL loader. Keep
query prologues, output binding names, literal payloads and effective report
metadata in framed equality keys. Refuse malformed or unsupported identity
claims with diagnostics; do not revive the old tokenizer as a fallback.

Verify positive alpha-equivalence plus adversarial literal, query, severity,
message and deactivation controls using synthetic authored inputs. Run strict
prebuild clippy, focused synthetic tests, repository hygiene and normal hooks.
Do not run local corpus production. Actual diagnostics production parity and
full retirement proof remain unverified until hosted production completes.

No legacy shape deletion or ceiling lowering belongs to this repair. The full
retirement issues stay open. Publish and merge through ghprsq under the user's
explicit CI waiver, then remove the completed branch and worktree.

Validation: all 36 shape-oracle tests passed, including join-target identity,
literal/report divergence, malformed-query refusal, directional messages,
reified severity and result annotations. Strict producer-independent clippy,
corpus-purity and helper hygiene passed. The final five-test focused rerun passed,
including the SELECT star output-name guard. Normal signed commit hooks passed.
Full corpus/gate and hosted production parity remain
unrun locally. The current hosted producer is still running; no CI wait is
required for this integration under the maintainer's instruction.

Integration review: origin/main has no changes beyond the selected base, so
there is no concurrent base delta or merge conflict to reconcile. The actual
production raw-identity caller uses the new native contract. The old tokenizer,
parenthesis normalizer and concatenated query keys are removed. Other semantic
witness paths are unchanged and are not newly claimed as universal proof.

PR #1865 is published. No inline comments or submitted reviews were present at
the integration read; CodeRabbit's automatic review was still processing.
No independent execution audit is claimed. Hosted CI and full corpus acceptance
are not prerequisites for this incremental merge under the maintainer's waiver.
