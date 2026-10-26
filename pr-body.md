Cold generation exposed a stack overflow. Static inspection of its authenticated producer identifies three hashing paths with one-megabyte stack frames; the scheduler frame is live across an input discovery callback that can enter nested Rayon work.

Move the streaming scratch buffers in action-cache verification, pipeline input hashing and slice-test worker hashing directly onto the heap. Preserve chunk size, SHA-256 framing, byte limits and existing error paths. Extend existing synthetic multi-chunk cache inspection and worker-copy identity controls. Principles 4 and 7 apply.

Independent source review passed. Four focused synthetic controls passed: action-key framing across chunks, authenticated cache inspection/read identity, blob-growth refusal, and copied worker executable digest identity. Scoped all-target strict diagnostics passed for action-cache, pipeline, slice-test and the developer CLI, with warnings treated as errors. Normal commit hooks passed after applying the formatter-required line wrap. Hosted checks and automated review are pending; the recorded user instruction permits merging without CI completion. No local optimized producer or corpus generation is performed under the recorded user exception.

This removes demonstrated oversized frames; without the crashing call stack, it does not establish that the complete hosted abort is resolved. Both cold generations, exact parity, authenticated fixtures and downstream acceptance remain open under #1962.

Closes #1966.
