# Question preparation review repair

CodeRabbit finding 4300064991 on 10c6e175f identified a useful missing boundary:
proof checking should prepare the exact question without executing another service
just to intern its terms. Verified against the pinned PurRDF 7734b013 implementation.

Commit 816a604cd exposes the existing native prepare operation through the typed
GMEOW diagnostic adapter. The native owner performs all preparation; no solver
session, duplicate encoding, fallback or new dependency is introduced. The proof
context documentation now names the exact preparation step.

All ten service controls passed in 0.056s. The added control records a fresh-class
question, prepares an independent consumer context without another decision,
verifies the retained full proof/certificate and rejects a different question.
Strict producer-independent workspace/all-target Clippy passed with warnings denied
in 4m34s. Every normal signed commit hook passed. Logs are
dl-question-preparation-tests.log, dl-question-preparation-clippy.log and
dl-question-preparation-commit.log. Push output: dl-question-preparation-push.log.

The full canon/prepared-base/installed proof requirements remain active under the
original plan. Four authenticated consumer packages remain excluded locally;
current-head hosted qualification and updated review remain required. No local
optimized producer, full gate, corpus construction or machine probe ran.
