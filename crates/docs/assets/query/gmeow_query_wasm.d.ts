/* tslint:disable */
/* eslint-disable */

/**
 * An in-memory RDF 1.2 dataset the browser can parse, serialize, and query.
 */
export class Dataset {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `Dataset.fromGts(bytes)` → read every named graph of a `gmeow.gts` bundle.
     *
     * This is the bundle-wide entry: the reader preserves the bundle's real graph
     * structure rather than folding it into a single union, so a query can select
     * a named graph and the RDF 1.2 statement layer stays addressable.
     *
     * # Errors
     *
     * Throws if the container cannot be read or its segments do not fold into a
     * dataset.
     */
    static fromGts(gts: Uint8Array): Dataset;
    /**
     * `Dataset.parse(text, format)` → parse an RDF document.
     *
     * `format` is any media type or short format id `purrdf` understands
     * (`turtle`/`ttl`, `trig`, `ntriples`/`nt`, `nquads`/`nq`, `rdfxml`,
     * `jsonld`, …).
     *
     * # Errors
     *
     * Throws if the format is unrecognized or the document does not parse.
     * There is no degraded fallback codec.
     */
    static parse(text: string, format: string): Dataset;
    /**
     * `query(sparql, base?)` → run a SPARQL query against this dataset, offline.
     *
     * Returns **SPARQL Results JSON** for SELECT / ASK and **Turtle** for
     * CONSTRUCT / DESCRIBE — the same contract `purrdf`'s own binding presents,
     * which is what the documentation playground is written against.
     *
     * # Errors
     *
     * A parse error, an evaluation error, or a `SERVICE` / `LOAD` clause
     * (unresolvable in-browser) throws — never a silent empty result.
     */
    query(sparql: string, base?: string | null): string;
    /**
     * `serialize(format)` → re-encode the dataset.
     *
     * Dataset-capable formats (TriG / N-Quads / TriX / JSON-LD / YAML-LD) carry
     * every named graph; single-graph syntaxes carry the default graph, which is
     * `purrdf`'s documented behaviour rather than a silent truncation here.
     *
     * # Errors
     *
     * Throws if the format is unrecognized or the dataset cannot be encoded.
     */
    serialize(format: string): string;
    /**
     * The number of quads in the dataset, across every graph.
     */
    readonly size: number;
}

/**
 * The engine version (this crate's SemVer), exposed to JS as `version()` — a
 * liveness probe proving the wasm module instantiated and the engine linked.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_dataset_free: (a: number, b: number) => void;
    readonly dataset_fromGts: (a: number, b: number, c: number) => void;
    readonly dataset_parse: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly dataset_query: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly dataset_serialize: (a: number, b: number, c: number, d: number) => void;
    readonly dataset_size: (a: number) => number;
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
