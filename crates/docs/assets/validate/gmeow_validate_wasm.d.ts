/* tslint:disable */
/* eslint-disable */

/**
 * Extract a `gmeow.gts` bundle's RDF as **graph-preserving N-Quads text**, so an
 * in-browser RDF engine (gmeow-query-wasm) can parse and query the SAME
 * bundle the pipeline shipped — the browser source of truth for the documentation
 * playground and bundle explorer, replacing any second curated data path.
 *
 * - `gts` — the `gmeow.gts` bundle bytes (the single canonical browser-query
 *   bundle; the container is read, not re-embedded).
 *
 * Returns N-Quads (`application/n-quads`) covering every named graph in the bundle
 * (the graph component of each quad is retained — the query surface sees the
 * bundle's real graph structure, not a flattened union).
 *
 * # Errors
 *
 * Throws a JS exception if the container cannot be read, the statement layer cannot
 * be folded, or the dataset cannot be serialized.
 */
export function bundle_dataset(gts: Uint8Array | number[]): string;

/**
 * The BLAKE3 digest of the original GMN-1 source document (`module.ttl`), as
 * lowercase hex. The prepared native packet preserves this original-byte identity,
 * so a JS caller can pin the source their document was validated against.
 */
export function gmn_codebook_digest(): string;

/**
 * Validate a GMN-1 document against the EMBEDDED codebook, returning a canonical JSON
 * verdict.
 *
 * The `bytes` are the raw GMN-1 surface text (the `@gmn{…}` header plus one record per
 * line). They are read through [`gmeow_lang_bridge::gmn1_read`] — the production codec's reader — against the
 * dictionary/glyph registry hydrated from the embedded native packet. Because the
 * codebook is embedded, glyphs, dictionary aliases, and prefixed terms are actually
 * RESOLVED: a document whose grammar is well-formed but which names a term the codebook
 * does not cover is rejected as `lang:GmnUncoveredTerm`, and every other codec-tier
 * violation resolves to its one typed `lang:LangConformanceFailure` class.
 *
 * # Returns
 *
 * A JSON object:
 * - conformant: `{ "conformant": true }` — the document read back cleanly.
 * - non-conformant: `{ "conformant": false, "failureClass":
 *   "https://blackcatinformatics.ca/lang/Gmn…", "detail": "…" }` — `failureClass` is the
 *   full `lang:` IRI from [`gmeow_lang_bridge::Gmn1Error::failure_class`] (the ONE
 *   canonical classifier), `detail` its human-readable rendering.
 *
 * # Errors
 *
 * Throws a JS exception only if the document text is not valid UTF-8. A build-integrity
 * failure of the embedded native codebook is not a runtime condition — its complete
 * tables are a pinned build artifact (see [`embedded_codebook`])
 * — so it hard-fails as a panic / wasm trap, never a document defect and never a silent
 * degradation to a syntax-only check.
 */
export function gmn_validate(bytes: Uint8Array | number[]): string;

/**
 * Run Tier-1 conformance of `data` (RDF text in `format`) against the SHACL shapes
 * and OntoUML disciplines carried in the `gts` bundle bytes, returning the
 * diagnostics `Report` as a JSON string.
 *
 * - `data` — the RDF document to validate (UTF-8 text).
 * - `format` — a media type or short id understood by the validator
 *   (`turtle`/`ttl`, `trig`, `n-triples`/`nt`, `n-quads`/`nq`, `rdf+xml`, or the
 *   JSON-LD ids `json-ld`/`jsonld`).
 * - `gts` — the `gmeow.gts` bundle bytes (carrying the `shapes-archive`).
 * - `namespace` — the GMEOW IRI prefix the discipline checks key on.
 * - `origin` — the data file's display path, recorded as each finding's location.
 *
 * The returned JSON is the canonical `Report`: `{ "tool": "validate", "findings":
 * [ { "severity": "error"|"warning"|"note", "code": ..., ... } ] }`, with `findings`
 * omitted when the graph conforms.
 *
 * # Errors
 *
 * Throws a JS exception if the bundle carries no `shapes-archive`, the archive or
 * shapes are malformed, or the data graph fails to parse.
 */
export function validate(data: string, format: string, gts: Uint8Array | number[], namespace: string, origin: string): string;

/**
 * The validator version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build + the npm package: importing the module and
 * calling `version()` proves it instantiated and the validator core linked.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly bundle_dataset: (a: number, b: number, c: number) => void;
    readonly gmn_codebook_digest: (a: number) => void;
    readonly gmn_validate: (a: number, b: number, c: number) => void;
    readonly validate: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly version: (a: number) => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_export4: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
