Standpoint: add a frame relation between standpoints that does not narrow (Principles 9, 10)

Adds gmeow:madeWithinContextOf: a standpoint that exists for reading works is related to the frame whose
context its readings were made within. It is not a subproperty of gmeow:sharpens, declares no property
characteristic and entails no sharpening, so two standpoints may share a context without either being a
case of the other. Domain and range are authored as logic: axioms. Usage guidance, the slice guide and the
French and Chinese catalogs carry the same rule.

Verified: independent review PASS at c5053878f; rapper, msgfmt, the issue-reference lint and the producer
recipe exit 0 on the merged tree. Hosted helpers, lint, producer-build and rust-prebuild green; the three
producer jobs and the aggregate are red as on main, for lack of runner memory. No producer run was made.

Closes #2684.
