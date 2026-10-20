Restore canonical Turtle parsing in three source files. The quality rubric range declaration lost its final period during the word-datatype migration. The agentic and observations competency additions omitted their existing `ot` namespace binding; agentic also needs `xsd` for its typed expected cells. Declare those namespaces and consolidate identical agentic `case` bindings at the header.

Generation A and fixture-prefix report the rubric syntax error; generation B reports the undeclared agentic prefix. The bounded audit found the corresponding observations omission before waiting for another CI run. Queries, expected rows, datatype unions and validation restrictions are unchanged.

Refs #1807 and #1757; both remain open for their full contracts. Principles 4 and 7: repair canonical inputs and preserve honest qualification.

Validation: source lint and normal signed hooks; source audits compare 30 migrated range terminators and review 49 recently changed Turtle files. These are source checks, not a successful native producer run. No tests mirror the punctuation/prefix changes, and no local corpus/fixture/optimized producer or full gate ran. Hosted acceptance is pending and ignored for sequential integration as instructed.
