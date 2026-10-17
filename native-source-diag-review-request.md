@coderabbitai full review

Please review the current 1c3bc726a head, especially the removal of the
string-only frontend error across actual compiler/dialect/pipeline/prover callers,
preserved native grades and source evidence, original parser/I/O causes, and the
consumer failure report. The source-mapping, parser, FOF/dialect and conformance
controls passed 138/138; three affected final pipeline round-trip controls,
strict producer-independent Clippy, helper hygiene and normal hooks also passed.
The authenticated corpus observation and embedded CLI regression remain locally
unqualified, and the full canon/prepared-base/installed-consumer plan is still
open. Please assess the actual source delta rather than infer full acceptance
from these focused checks. Principles 4, 7, 12 and 18.
