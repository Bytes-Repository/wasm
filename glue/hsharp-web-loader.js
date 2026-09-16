export async function runHSharpWasm(wasmUrl, opts = {}) {
    let memory = null;
    let instance = null;

    // ── pamięć wasm <-> JS stringi ─────────────────────────────────────
    function readStr(ptr, len) {
        if (ptr === 0 || len === 0) return "";
        const bytes = new Uint8Array(memory.buffer, ptr, len);
        return new TextDecoder("utf-8").decode(bytes);
    }

    function readCStr(ptr) {
        if (ptr === 0) return "";
        const mem = new Uint8Array(memory.buffer);
        let end = ptr;
        while (mem[end] !== 0) end++;
        return new TextDecoder("utf-8").decode(mem.subarray(ptr, end));
    }

    // Zapisuje `str` do świeżo zaalokowanego (przez wasm) bufora i zwraca
    // jego wskaźnik — to jest "wynik typu string" oddawany do Rusta.
    function writeReturnStr(str) {
        const bytes = new TextEncoder().encode(str ?? "");
        const ptr = instance.exports.hsharp_web_alloc(bytes.length);
        new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
        return ptr;
    }

    // ── stan mostu (uchwyty) ────────────────────────────────────────────
    const domHandles = [];        // handle:int -> Element
    const listeners = [];         // listener_id:int -> {el, type, fn}
    const timeouts = new Map();   // id -> real setTimeout id
    const intervals = new Map();  // id -> real setInterval id
    const rafs = new Map();       // id -> real rAF id
    const fetchStatus = new Map();// request_id -> HTTP status
    let nextTimerId = 1;
    let nextFetchId = 1;

    function registerElement(el) {
        if (!el) return -1;
        const h = domHandles.length;
        domHandles.push(el);
        return h;
    }
    function elementOf(handle) {
        return handle >= 0 && handle < domHandles.length ? domHandles[handle] : null;
    }

    function callOnEvent(kind, id, num, text) {
        const ptr = writeReturnStr(text ?? "");
        instance.exports.wasm_on_event(kind, id, num, ptr);
    }
    const KIND_TIMEOUT = 1, KIND_INTERVAL = 2, KIND_FRAME = 3,
          KIND_DOM_EVENT = 4, KIND_FETCH_OK = 5, KIND_FETCH_ERR = 6;

    // ── most "hsharp_web" — patrz runtime-rust/src/web_bridge.rs ───────
    const imports = {
        hsharp_web: {
            js_write(isNewline, ptr, len) {
                const msg = readStr(ptr, len);
                if (opts.onStdout) opts.onStdout(msg, !!isNewline);
                else if (isNewline) console.log(msg);
                else console.log(msg);
            },
            js_console(level, ptr, len) {
                const msg = readStr(ptr, len);
                const fn = [console.log, console.info, console.warn, console.error, console.debug][level] || console.log;
                fn(msg);
            },
            js_console_clear() { console.clear(); },
            js_console_group(ptr, len) { console.group(readStr(ptr, len)); },
            js_console_group_end() { console.groupEnd(); },

            js_dom_get_by_id(ptr, len) {
                return registerElement(document.getElementById(readStr(ptr, len)));
            },
            js_dom_query_selector(ptr, len) {
                return registerElement(document.querySelector(readStr(ptr, len)));
            },
            js_dom_query_selector_all(ptr, len) {
                const found = document.querySelectorAll(readStr(ptr, len));
                const ids = Array.from(found).map((el) => registerElement(el));
                return writeReturnStr(ids.join(","));
            },
            js_dom_create_element(ptr, len) {
                return registerElement(document.createElement(readStr(ptr, len)));
            },
            js_dom_body() { return registerElement(document.body); },
            js_dom_head() { return registerElement(document.head); },
            js_dom_append_child(parent, child) {
                const p = elementOf(parent), c = elementOf(child);
                if (!p || !c) return 0;
                p.appendChild(c);
                return 1;
            },
            js_dom_prepend_child(parent, child) {
                const p = elementOf(parent), c = elementOf(child);
                if (!p || !c) return 0;
                p.prepend(c);
                return 1;
            },
            js_dom_remove(handle) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.remove();
                return 1;
            },
            js_dom_set_inner_html(handle, ptr, len) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.innerHTML = readStr(ptr, len);
                return 1;
            },
            js_dom_get_inner_html(handle) {
                const el = elementOf(handle);
                return writeReturnStr(el ? el.innerHTML : "");
            },
            js_dom_set_text(handle, ptr, len) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.textContent = readStr(ptr, len);
                return 1;
            },
            js_dom_get_text(handle) {
                const el = elementOf(handle);
                return writeReturnStr(el ? el.textContent : "");
            },
            js_dom_set_attribute(handle, np, nl, vp, vl) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.setAttribute(readStr(np, nl), readStr(vp, vl));
                return 1;
            },
            js_dom_get_attribute(handle, np, nl) {
                const el = elementOf(handle);
                return writeReturnStr(el ? (el.getAttribute(readStr(np, nl)) ?? "") : "");
            },
            js_dom_remove_attribute(handle, np, nl) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.removeAttribute(readStr(np, nl));
                return 1;
            },
            js_dom_add_class(handle, ptr, len) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.classList.add(readStr(ptr, len));
                return 1;
            },
            js_dom_remove_class(handle, ptr, len) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.classList.remove(readStr(ptr, len));
                return 1;
            },
            js_dom_toggle_class(handle, ptr, len) {
                const el = elementOf(handle);
                if (!el) return 0;
                return el.classList.toggle(readStr(ptr, len)) ? 1 : 0;
            },
            js_dom_has_class(handle, ptr, len) {
                const el = elementOf(handle);
                return el && el.classList.contains(readStr(ptr, len)) ? 1 : 0;
            },
            js_dom_set_style(handle, pp, pl, vp, vl) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.style.setProperty(readStr(pp, pl), readStr(vp, vl));
                return 1;
            },
            js_dom_get_style(handle, pp, pl) {
                const el = elementOf(handle);
                return writeReturnStr(el ? el.style.getPropertyValue(readStr(pp, pl)) : "");
            },
            js_dom_set_value(handle, ptr, len) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.value = readStr(ptr, len);
                return 1;
            },
            js_dom_get_value(handle) {
                const el = elementOf(handle);
                return writeReturnStr(el && "value" in el ? el.value : "");
            },
            js_dom_is_valid(handle) {
                const el = elementOf(handle);
                return el && el.isConnected !== false ? 1 : 0;
            },
            js_dom_parent(handle) {
                const el = elementOf(handle);
                return el && el.parentElement ? registerElement(el.parentElement) : -1;
            },

            js_events_listen(handle, tp, tl) {
                const el = elementOf(handle);
                if (!el) return -1;
                const type = readStr(tp, tl);
                const id = listeners.length;
                const fn = (ev) => callOnEvent(KIND_DOM_EVENT, id, 0, ev.type);
                listeners.push({ el, type, fn });
                el.addEventListener(type, fn);
                return id;
            },
            js_events_unlisten(listenerId) {
                const l = listeners[listenerId];
                if (!l) return 0;
                l.el.removeEventListener(l.type, l.fn);
                listeners[listenerId] = null;
                return 1;
            },

            js_timers_set_timeout(delayMs) {
                const id = nextTimerId++;
                const real = setTimeout(() => callOnEvent(KIND_TIMEOUT, id, 0, ""), delayMs);
                timeouts.set(id, real);
                return id;
            },
            js_timers_clear_timeout(id) {
                if (timeouts.has(id)) { clearTimeout(timeouts.get(id)); timeouts.delete(id); }
            },
            js_timers_set_interval(delayMs) {
                const id = nextTimerId++;
                const real = setInterval(() => callOnEvent(KIND_INTERVAL, id, 0, ""), delayMs);
                intervals.set(id, real);
                return id;
            },
            js_timers_clear_interval(id) {
                if (intervals.has(id)) { clearInterval(intervals.get(id)); intervals.delete(id); }
            },
            js_timers_request_animation_frame() {
                const id = nextTimerId++;
                const real = requestAnimationFrame((ts) => { rafs.delete(id); callOnEvent(KIND_FRAME, id, ts, ""); });
                rafs.set(id, real);
                return id;
            },
            js_timers_cancel_animation_frame(id) {
                if (rafs.has(id)) { cancelAnimationFrame(rafs.get(id)); rafs.delete(id); }
            },

            js_storage_get(area, kp, kl) {
                const store = area === 0 ? localStorage : sessionStorage;
                return writeReturnStr(store.getItem(readStr(kp, kl)) ?? "");
            },
            js_storage_set(area, kp, kl, vp, vl) {
                const store = area === 0 ? localStorage : sessionStorage;
                try { store.setItem(readStr(kp, kl), readStr(vp, vl)); return 1; }
                catch (_e) { return 0; }
            },
            js_storage_remove(area, kp, kl) {
                const store = area === 0 ? localStorage : sessionStorage;
                store.removeItem(readStr(kp, kl));
                return 1;
            },
            js_storage_clear(area) {
                (area === 0 ? localStorage : sessionStorage).clear();
                return 1;
            },
            js_storage_has(area, kp, kl) {
                const store = area === 0 ? localStorage : sessionStorage;
                return store.getItem(readStr(kp, kl)) !== null ? 1 : 0;
            },

            js_fetch(method, up, ul, bp, bl, cp, cl) {
                const url = readStr(up, ul);
                const id = nextFetchId++;
                const init = { method: method === 1 ? "POST" : "GET" };
                if (method === 1) {
                    init.body = readStr(bp, bl);
                    init.headers = { "Content-Type": readStr(cp, cl) || "text/plain" };
                }
                fetch(url, init)
                    .then(async (resp) => {
                        fetchStatus.set(id, resp.status);
                        const text = await resp.text();
                        callOnEvent(resp.ok ? KIND_FETCH_OK : KIND_FETCH_ERR, id, resp.status, text);
                    })
                    .catch((err) => {
                        fetchStatus.set(id, 0);
                        callOnEvent(KIND_FETCH_ERR, id, 0, String(err));
                    });
                return id;
            },
            js_fetch_status(id) { return fetchStatus.get(id) ?? 0; },

            js_random_f64() { return Math.random(); },

            js_time_now_ms() { return Date.now(); },
            js_time_perf_now_ms() { return performance.now(); },

            js_canvas_set_size(handle, w, h) {
                const el = elementOf(handle);
                if (!el) return 0;
                el.width = w; el.height = h;
                return 1;
            },
            js_canvas_clear(handle) {
                const ctx = ctxOf(handle);
                if (!ctx) return 0;
                ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
                return 1;
            },
            js_canvas_fill_style(handle, ptr, len) { const c = ctxOf(handle); if (!c) return 0; c.fillStyle = readStr(ptr, len); return 1; },
            js_canvas_stroke_style(handle, ptr, len) { const c = ctxOf(handle); if (!c) return 0; c.strokeStyle = readStr(ptr, len); return 1; },
            js_canvas_line_width(handle, w) { const c = ctxOf(handle); if (!c) return 0; c.lineWidth = w; return 1; },
            js_canvas_fill_rect(handle, x, y, w, h) { const c = ctxOf(handle); if (!c) return 0; c.fillRect(x, y, w, h); return 1; },
            js_canvas_stroke_rect(handle, x, y, w, h) { const c = ctxOf(handle); if (!c) return 0; c.strokeRect(x, y, w, h); return 1; },
            js_canvas_clear_rect(handle, x, y, w, h) { const c = ctxOf(handle); if (!c) return 0; c.clearRect(x, y, w, h); return 1; },
            js_canvas_begin_path(handle) { const c = ctxOf(handle); if (!c) return 0; c.beginPath(); return 1; },
            js_canvas_move_to(handle, x, y) { const c = ctxOf(handle); if (!c) return 0; c.moveTo(x, y); return 1; },
            js_canvas_line_to(handle, x, y) { const c = ctxOf(handle); if (!c) return 0; c.lineTo(x, y); return 1; },
            js_canvas_arc(handle, x, y, r, a0, a1) { const c = ctxOf(handle); if (!c) return 0; c.arc(x, y, r, a0, a1); return 1; },
            js_canvas_close_path(handle) { const c = ctxOf(handle); if (!c) return 0; c.closePath(); return 1; },
            js_canvas_fill(handle) { const c = ctxOf(handle); if (!c) return 0; c.fill(); return 1; },
            js_canvas_stroke(handle) { const c = ctxOf(handle); if (!c) return 0; c.stroke(); return 1; },
            js_canvas_font(handle, ptr, len) { const c = ctxOf(handle); if (!c) return 0; c.font = readStr(ptr, len); return 1; },
            js_canvas_fill_text(handle, ptr, len, x, y) { const c = ctxOf(handle); if (!c) return 0; c.fillText(readStr(ptr, len), x, y); return 1; },
        },
    };

    const ctxCache = new Map();
    function ctxOf(handle) {
        if (ctxCache.has(handle)) return ctxCache.get(handle);
        const el = elementOf(handle);
        if (!el || el.tagName !== "CANVAS") return null;
        const ctx = el.getContext("2d");
        ctxCache.set(handle, ctx);
        return ctx;
    }

    const { instance: inst } = await WebAssembly.instantiateStreaming(fetch(wasmUrl), imports);
    instance = inst;
    memory = instance.exports.memory;

    if (typeof instance.exports.hsharp_runtime_install_panic_hook === "function") {
        instance.exports.hsharp_runtime_install_panic_hook();
    }
    if (typeof instance.exports.main === "function") {
        instance.exports.main();
    } else {
        console.warn("[hsharp-web-loader] moduł .wasm nie eksportuje `main` — sprawdź flagi --export w kroku wasm-ld (patrz README.md).");
    }

    return instance;
}
