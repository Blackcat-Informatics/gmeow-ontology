/* tslint:disable */
/* eslint-disable */

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
export function validate(data: string, format: string, gts: Uint8Array, namespace: string, origin: string): string;

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
    readonly validate: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number, number];
    readonly version: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
