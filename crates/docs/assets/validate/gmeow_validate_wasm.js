/* @ts-self-types="./gmeow_validate_wasm.d.ts" */

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
 * @param {Uint8Array | number[]} gts
 * @returns {string}
 */
export function bundle_dataset(gts) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passArray8ToWasm0(gts, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        wasm.bundle_dataset(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export2(deferred3_0, deferred3_1, 1);
    }
}

/**
 * The BLAKE3 digest of the original GMN-1 source document (`module.ttl`), as
 * lowercase hex. The prepared native packet preserves this original-byte identity,
 * so a JS caller can pin the source their document was validated against.
 * @returns {string}
 */
export function gmn_codebook_digest() {
    let deferred1_0;
    let deferred1_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        wasm.gmn_codebook_digest(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export2(deferred1_0, deferred1_1, 1);
    }
}

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
 * @param {Uint8Array | number[]} bytes
 * @returns {string}
 */
export function gmn_validate(bytes) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        wasm.gmn_validate(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export2(deferred3_0, deferred3_1, 1);
    }
}

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
 * @param {string} data
 * @param {string} format
 * @param {Uint8Array | number[]} gts
 * @param {string} namespace
 * @param {string} origin
 * @returns {string}
 */
export function validate(data, format, gts, namespace, origin) {
    let deferred7_0;
    let deferred7_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4;
        try {
            ptr0 = passStringToWasm0(data, wasm.__wbindgen_export, wasm.__wbindgen_export4);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(format, wasm.__wbindgen_export, wasm.__wbindgen_export4);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passArray8ToWasm0(gts, wasm.__wbindgen_export);
            len2 = WASM_VECTOR_LEN;
            ptr3 = passStringToWasm0(namespace, wasm.__wbindgen_export, wasm.__wbindgen_export4);
            len3 = WASM_VECTOR_LEN;
            ptr4 = passStringToWasm0(origin, wasm.__wbindgen_export, wasm.__wbindgen_export4);
            len4 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr4) { wasm.__wbindgen_export2(ptr4, len4 * 1, 1); }
            if (ptr3) { wasm.__wbindgen_export2(ptr3, len3 * 1, 1); }
            if (ptr2) { wasm.__wbindgen_export2(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_export2(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export2(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.validate(retptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr6 = r0;
        var len6 = r1;
        if (r3) {
            ptr6 = 0; len6 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred7_0 = ptr6;
        deferred7_1 = len6;
        return getStringFromWasm0(ptr6, len6);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export2(deferred7_0, deferred7_1, 1);
    }
}

/**
 * The validator version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build + the npm package: importing the module and
 * calling `version()` proves it instantiated and the validator core linked.
 * @returns {string}
 */
export function version() {
    let deferred1_0;
    let deferred1_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        wasm.version(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export2(deferred1_0, deferred1_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_now_80faebe4b1fd98e9: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_random_f755c57ee5c03213: function() {
            const ret = Math.random();
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        },
    };
    return {
        __proto__: null,
        "./gmeow_validate_wasm_bg.js": import0,
    };
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__wbindgen_export3(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('gmeow_validate_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
