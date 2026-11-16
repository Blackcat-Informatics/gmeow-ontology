# Issue #1897: Ground model-serving tensors and metrics in the math substrate

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: terms

## Body

Parent: #1717. Blocks: #1717. This is one complete cluster from the original quantitative-grounding contract; the parent remains open until every remaining cluster and its production acceptance are complete.

## Scope and acceptance

Audit model-serving consumers and its deliberately distinct artifact/deployment/execution/session identities. Ground actual tensors and quantitative metrics through the math linear-algebra layer, TensorComputationGraph and LossFunction where semantically applicable; retain model metrics as dimensioned quantities and preserve procedure/result/observation separation. Reuse core math/AI owners rather than a parallel tensor vocabulary or an extension-to-extension dependency. Add real math artifact references and matching manifest dependency, guard retirement of superseded scalar vocabulary and execute actual model-serving consumers, valid/invalid tensor and metric controls and full required gates.

Principles 4, 7, 9, 11, 17 and 19 govern this delivery. No requirement is waived by this decomposition. External repository implementation or model/service lifecycle actions require separate authorization.


## Comments (6)

### paudley — 2026-10-21T04:46:27Z

Review of the model-serving grounding work found a remaining shared math implementation requirement: arbitrary authored TensorShape/TensorAxis RDF currently has no rank/count/index/extent consistency validator. The math ontology defines these terms and describes their intended consistency; ONNX lifting constructs consistent rank/index metadata and checks some initializer input failures. Those construction tests do not validate arbitrary authored tensor-shape graphs.

The current source unit covers selected artifact/model/graph/weight/parameter ownership, declared training objectives and held finite scalar metric observations. Its authored consumer covers those contracts; axis/count controls there concern ReferenceFrame. Worked tensor-shape metadata is not claimed as conformance evidence. All current source-unit execution remains unrun pending qualification.

Keep this missing implementation within this issue and the existing grounding program (#1717, #1433). It must be completed through canonical logic laws and actual consumers before full tensor-shape acceptance or issue closure. No duplicate authored SHACL constraint, silent fallback or new follow-up issue is being used to remove that requirement. Principles 4, 7 and 17 apply.


### paudley — 2026-10-21T05:38:31Z

The first focused qualification executed eight model/frame/tensor/ONNX controls: six passed and two failed. One failure was missing required formalizes metadata in an independently authored synthetic projection fixture; that fixture is repaired and awaits rerun.

The other is a real native dependency failure. A tiny datatype-union maximum-cardinality input returns an undecided native verdict even for one valid integer value. The reproducer and source-owner diagnosis are recorded on the existing reasoning owner in [#1803](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/1803#issuecomment-6308880599). This is neither a SHACL result nor completed native acceptance. The failing expected verdict remains intact, and this delivery stays uncommitted while that required check is red.

Source review also found ONNX type identity based on ambiguous display labels, delimiter collisions in shape keys, and missing Set domains on arbitrary tensor leaf declarations. Bounded repairs are in progress in the shared math/ONNX owners, with source-specific value spaces distinct from natural-number extent domains. No second evaluator or authored shape surface is being introduced. Source, generated/native, installed and full acceptance remain separate; #1897 and #1717 remain open. No local corpus producer or full gate ran.


### paudley — 2026-10-21T06:00:42Z

The independently qualified ONNX metadata repair is now tracked by #1902. It preserves structural type/shape identity, supplies explicit mathematical value-space domains for leaves, and refuses incompatible supplied annotations. All59 ONNX lift cases passed, including the new collision/domain controls; the transferred Rust files match that qualified source exactly. Strict producer-independent workspace/all-target Clippy also passed.

The generic arbitrary-RDF tensor laws, model-serving profiles, actual produced/native/installed acceptance and failing datatype-cardinality regression remain in #1897 under its existing full contract. The native completion blocker remains owned by #1803. The original uncommitted checkout and complete failure logs are preserved. No expected verdict or requirement was weakened, and no local corpus or full gate ran. This separation lets the complete metadata bug repair reach main while the broader native dependency remains open.


### paudley — 2026-10-31T01:11:22Z

The independently useful model-artifact profile and attributed-metric source portion is now owned by #2149 and is being implemented immediately from the preserved donor against current main. It reuses existing math owners and excludes already merged ONNX/value-space changes and donor testing scaffolding. Complete arbitrary-RDF tensor-shape laws, native verdicts, produced/installed consumers and the original full acceptance remain here; this source child does not close or weaken #1897 or #1717.

The retained integer-union failure is historical evidence from the older pin, not a verified current-main result. No native success is claimed. Source publication need not restart a corpus campaign, while normal hooks and honest validation limits remain mandatory (Principles 4, 7, 17 and 19).


### paudley — 2026-10-31T01:25:14Z

The second concrete source unit is #2151: the eight generic canonical tensor-shape laws and associated typed restrictions, documentation, paired catalogs and ordinary example. It is being implemented immediately using existing validation primitives. The model-profile/metric source unit remains #2149. Both preserve the dirty donor and exclude new tests, fixtures and engine changes.

Full native verdicts, produced/installed consumers and all original end-to-end acceptance remain on #1897 and its parents. Neither source child claims a current native-cardinality result or tensor payload/operator execution (Principles 4, 7, 17 and 19).


### paudley — 2026-11-14T23:30:53Z

Blocks: #2795 (Release 0.4.0).

