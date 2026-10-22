# Complete ONNX semantic reconciliation

Root reproduced the full line reconciliation independently from the review mapping:190unchanged,85replaced by15exact IRI substitutions,19additions,zero unexplained losses or additions. No native graph-isomorphism run or producer rerun is claimed.

All local suffixes use https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/

| Old | New |
| --- | --- |
| type-13c5892a09d8573a | type-6b57504bf34e58db |
| type-26941a4fdfb5ad92 | type-72b11e96c5fb111c |
| type-913744355430c884 | type-9d7b8925a6ecb23a |
| type-913a44355432fe5b | type-9d7b8425a6eca9bb |
| shape-3fb3eb4f0dfbbafc | shape-dc5a8927e7644867 |
| shape-e72677796b2bb834 | shape-1663a3ecf0629d95 |
| shape-e72937796b2d814b | shape-16639cecf06291b0 |
| shape-f7fd85662eba4e9f | shape-7f136f5819df2ae4 |
| axis-1127ae06e9c28dd4 | axis-7205715a8bac3227 |
| axis-1127af06e9c28f87 | axis-7205705a8bac3074 |
| axis-a3aadbd15e6afa2c | axis-5c22efdf2d0ff769 |
| axis-a3aadcd15e6afbdf | axis-5c22eedf2d0ff5b6 |
| axis-a684a2d686d8a018 | axis-1eaddddf0a04fb1b |
| axis-a684a3d686d8a1cb | axis-1eaddcdf0a04f968 |
| axis-b7430e414d1efe8c | axis-dfe861cb5d3a33af |

The complete additions are:

```nquads
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/declaration-4689e84362a450f0> <https://blackcatinformatics.ca/math/variableDomain> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8925a6ecb23a> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/declaration-daafa35e23b7354f> <https://blackcatinformatics.ca/math/variableDomain> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-6b57504bf34e58db> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/declaration-f43776e104191e0d> <https://blackcatinformatics.ca/math/variableDomain> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-72b11e96c5fb111c> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-6b57504bf34e58db> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/math/Set> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-6b57504bf34e58db> <http://www.w3.org/2000/01/rdf-schema#label> "values admitted by tensor(float)[3]" .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-6b57504bf34e58db> <https://blackcatinformatics.ca/gmeow/wasGeneratedBy> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-72b11e96c5fb111c> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/math/Set> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-72b11e96c5fb111c> <http://www.w3.org/2000/01/rdf-schema#label> "values admitted by tensor(float)[4,3]" .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-72b11e96c5fb111c> <https://blackcatinformatics.ca/gmeow/wasGeneratedBy> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8425a6eca9bb> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/math/Set> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8425a6eca9bb> <http://www.w3.org/2000/01/rdf-schema#label> "values admitted by tensor(float)[1,3]" .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8425a6eca9bb> <https://blackcatinformatics.ca/gmeow/wasGeneratedBy> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8925a6ecb23a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/math/Set> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8925a6ecb23a> <http://www.w3.org/2000/01/rdf-schema#label> "values admitted by tensor(float)[1,4]" .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8925a6ecb23a> <https://blackcatinformatics.ca/gmeow/wasGeneratedBy> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/type-6b57504bf34e58db> <https://blackcatinformatics.ca/math/expressionValueSpace> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-6b57504bf34e58db> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/type-72b11e96c5fb111c> <https://blackcatinformatics.ca/math/expressionValueSpace> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-72b11e96c5fb111c> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/type-9d7b8425a6eca9bb> <https://blackcatinformatics.ca/math/expressionValueSpace> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8425a6eca9bb> .
<https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/type-9d7b8925a6ecb23a> <https://blackcatinformatics.ca/math/expressionValueSpace> <https://blackcatinformatics.ca/gmeow/examples/math/producers/onnx-run-7978a9f31743d3c2/tensor-value-space-9d7b8925a6ecb23a> .
```

Root read onnx/lift.rs attach_tensor_type and shape_key/tensor_type_key: framed structural keys replace rendered labels; admitted-value Sets and declaration domains are intentional in00b7ccdc2. All old triples survive after these exact identity substitutions; dimensions, axes, element type, operators, weights and correspondence facts remain. Four new generated Set nodes explain55to59generatednodes. Hosted producer source is unchanged in current lift/model/producers paths. Recommended bounded repair is exact reviewed hosted canonical payload as fixture body, with corrected explanatory header, preserving parity enforcement. Numerical execution/model-serving/fullcorpus acceptance remain unverified.
