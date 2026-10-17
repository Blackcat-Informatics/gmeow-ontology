# Diagnostic macro source-input ownership

Hosted producer-recipe rejected the new shared impl_diag_kind macro before
compilation, because the authenticated source-input traversal only recognized
define_diag_kind. The failure is retained in producer-build-failure.log.

Register the syntax-only shared diagnostic implementation macro alongside its
definition macro. Both forward their complete argument tokens to the existing
recursive traversal. No unknown macro, source-producing declaration, included
file or compile-time input is silently accepted or skipped.

The focused control calls the real InputInventory::collect with a synthetic
source file. It verifies nested included bytes are retained and changing them
changes the inventory; an unknown nested input capability must still fail. The
existing lexical-scope, standard-output nested-input and negated-keyword controls
are selected alongside it. No producer recipe, host probe or corpus production
is invoked by these tests.

All four selected ownership controls passed (macro-ownership-tests.log), including
lexical scope, output-macro inputs and keyword negation. The refreshed real
helpers-hygiene gate passed (macro-ownership-helpers.log). Strict producer-independent
workspace/all-target Clippy passed with warnings denied in 4m50s
(macro-ownership-clippy.log). Commit 425eb65de passed all normal hooks and is pushed
normally to PR 1835. Hosted producer-recipe admission must qualify the new source.
The earlier 156 runtime tests and
helper gate retain their original coverage; they did not exercise producer source
capability admission and are not evidence that the failed hosted job passed.
