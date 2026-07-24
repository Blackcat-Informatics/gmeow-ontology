/* tslint:disable */
/* eslint-disable */

/**
 * Test a candidate `logic:` formula against a KB with the native SYMMETRIC conjecture
 * engine and return the **deterministic verdict** as N-Triples text — the SAME projection
 * the on-gate MCP / CLI surface emits (proven byte-identical by the native≡wasm conjecture
 * witness). Powers the live documentation conjecture playground (issue #1406 W4).
 *
 * - `kb` — the knowledge base to test against (RDF text in `kb_format`).
 * - `kb_format` — a media type / short id purrdf understands (`turtle`/`ttl`,
 *   `n-triples`/`nt`, `n-quads`/`nq`, `trig`, `rdf+xml`, `json-ld`).
 * - `formula` — the candidate `logic:` document naming exactly one `logic:Formula` / axiom.
 * - `standpoint` — the reified standpoint IRI the verdict is scoped to (REQUIRED; a
 *   conjecture verdict is always standpoint-scoped, never global — Principle 9).
 *
 * The symmetric two legs (proof `KB ⊨ φ` and counterproof `KB ∪ {φ} ⊨ ⊥`) and the Belnap
 * classification are all readable from the returned N-Triples; the JS controller renders
 * them side-by-side.
 *
 * # Errors
 *
 * Throws a JS exception if the candidate does not name exactly one formula, if the KB
 * cannot be parsed, or if the native conjecture engine fails.
 */
export function conjecture(kb: string, kb_format: string, formula: string, standpoint: string): string;

/**
 * Run the structured-DL chase over `data` (RDF text in `format`) and return the
 * **reasoned closure** — the inferred triples — as N-Quads text.
 *
 * - `data` — the RDF document to reason over (UTF-8 text).
 * - `format` — a media type / short id purrdf understands (`turtle`/`ttl`,
 *   `n-triples`/`nt`, `n-quads`/`nq`, `trig`, `rdf+xml`, `json-ld`).
 *
 * # Errors
 *
 * Throws a JS exception if the data cannot be parsed, reasoning fails, or the
 * closure cannot be serialized.
 */
export function reason(data: string, format: string): string;

/**
 * The reasoner version (the crate's SemVer), exposed to JS as `version()` — a
 * liveness probe proving the wasm module instantiated and the reasoner core linked.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly conjecture: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly reason: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly version: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
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
