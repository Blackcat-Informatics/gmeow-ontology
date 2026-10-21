# Independent source review: native shell template values

Disposition: no actionable finding in the frozen two-file change. Read Stage plan, exact renderer/test diff, embedded shell, actual navigation/context caller and pinned Minijinja APIs. No builds, checks, tests, host probes or source edits were run by this reviewer. Only this Stage artifact was written.

## Actual production boundary

The former private `NavItem` Serialize wrapper is removed, and its sole production caller now receives native Minijinja map values. The entire shell context is also built as a native map, with the same nine keys: site title/version, page title, CSS href, root href, navigation, body, page language and body scripts. No unused JSON codec, JSON intermediate, Serde bridge or dependency change was introduced. Docs fixture records continue to require Serde; this is not complete dependency removal.

Pinned Minijinja2.21.0 `value/argtypes.rs` maps `FromIterator<Value>` into an ordered vector-backed sequence and `FromIterator<(key,value)>` into its native value map. `From<&str>`, `From<&String>` and `From<String>` preserve text as ordinary strings, not safe strings. `Template::render` explicitly documents that an already-native `Value` does not undergo actual serialization. This is the intended template owner path despite its public render method retaining a generic Serialize signature.

## Preserved behavior

Navigation remains the same fixed vector order; individual maps retain exact `label` and `href` fields. The href expression still uses the same root plus `join(dir, "index.html")`; root calculation and all page-directory choices are untouched. Empty strings remain strings rather than undefined/null values. Map enumeration order is not part of this shell's behavior: it performs named attribute lookup, while navigation order comes from the preserved sequence.

The UI-label closure and its English/default fallback remain unchanged. The same language computation, Markdown-to-HTML body, link rewriting, controller-selection conditions, script string, template name/environment and render failure behavior feed the same shell. No safe-string constructor, escaping filter, autoescape setting, localization or HTML template changed. The native value path therefore preserves the former string safety classification and existing HTML behavior rather than introducing a new escaping policy.

## Coverage reviewed and limits

Three authored `navigation_template_native_` controls cover empty/Unicode/punctuation field values and href construction; the actual shell's complete nested navigation order, language fallback, root/CSS/title/body/footer contract; and the selected optional playground appended once and last. The old `render_site_lang_uses_translation_with_english_fallback` uses the same independently authored tiny model and retains translation/page-graph checks. These paths do not invoke discovery, authenticated fixture loaders, corpus builders or producers.

## Follow-up: template EOF expectation

The initial review incorrectly inferred rendered EOF behavior from the template file's final newline and missed the engine default. Root's first four-control execution passed three controls; the shell control reached and passed its navigation/title/CSS/language/body/footer assertions, then failed only the newly added final-LF expectation. The original failure is retained in `navigation-tests-initial.log`.

Reviewed the narrow correction: the test now expects `</footer>\n</body>\n</html>` without a final LF and explains why. Pinned Minijinja2.21.0 `environment.rs` explicitly documents the `keep_trailing_newline` default as false, stripping one final template newline; unchanged `shell_env` does not override it. The correction pins existing rendered output and changes no production source, template, environment setting or rendering policy. The corrected four-control rerun belongs to root and is not inferred to have passed by this review.

Source inspection does not establish test success or full rendered-site parity. Root owns focused execution, source-policy/helper qualification and normal hooks. Actual full-site/installed/corpus acceptance and complete issue1776 dependency migration remain outstanding. Review frozen.
