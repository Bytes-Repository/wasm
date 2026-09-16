use std::alloc::{alloc, Layout};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

unsafe fn s(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

fn leak(s: &str) -> *mut c_char {
    CString::new(s.replace('\0', "")).unwrap_or_default().into_raw()
}

const B_TRUE: i64 = 1;
const B_FALSE: i64 = 0;
fn as_bool(v: i64) -> i64 {
    if v != 0 {
        B_TRUE
    } else {
        B_FALSE
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Alokator eksportowany dla JS — glue/hsharp-web-loader.js woła to, żeby
// zarezerwować miejsce w pamięci wasm PRZED zapisaniem tam stringa i
// zwróceniem jego wskaźnika do importu, który go zażądał (patrz
// glue/hsharp-web-loader.js, sekcja "writeStringIntoWasm"). Zaalokowany
// bufor jest celowo "przeciekający" (nigdy nie zwalniany pojedynczo) —
// dokładnie ta sama konwencja co reszta runtime'u H# (patrz
// core_runtime.rs). Frontowa aplikacja wasm nie powinna wołać tego
// eksportu bezpośrednio — to szczegół implementacyjny mostu JS.
#[no_mangle]
pub extern "C" fn hsharp_web_alloc(len: i32) -> i32 {
    let len = len.max(0) as usize;
    let layout = Layout::array::<u8>(len + 1).unwrap();
    unsafe {
        let ptr = alloc(layout);
        *ptr.add(len) = 0;
        ptr as i32
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Importy z JS — implementowane w glue/hsharp-web-loader.js. Konwencja:
// argumenty string wchodzą jako (ptr: i32, len: i32) do pamięci wasm
// (Rust już zna długość z CStr, więc JS nie musi jej liczyć); wyniki typu
// string wracają jako i32 = wskaźnik na już zaalokowany (przez
// hsharp_web_alloc, wołany przez JS) null-terminated bufor w pamięci wasm.
// Uchwyty (elementy DOM, timery, żądania fetch, ...) to zwykłe i32,
// -1 = brak/błąd.
#[link(wasm_import_module = "hsharp_web")]
extern "C" {
    fn js_write(is_newline: i32, ptr: i32, len: i32);
    fn js_console(level: i32, ptr: i32, len: i32);
    fn js_console_clear();
    fn js_console_group(ptr: i32, len: i32);
    fn js_console_group_end();

    fn js_dom_get_by_id(ptr: i32, len: i32) -> i32;
    fn js_dom_query_selector(ptr: i32, len: i32) -> i32;
    fn js_dom_query_selector_all(ptr: i32, len: i32) -> i32;
    fn js_dom_create_element(ptr: i32, len: i32) -> i32;
    fn js_dom_body() -> i32;
    fn js_dom_head() -> i32;
    fn js_dom_append_child(parent: i32, child: i32) -> i32;
    fn js_dom_prepend_child(parent: i32, child: i32) -> i32;
    fn js_dom_remove(handle: i32) -> i32;
    fn js_dom_set_inner_html(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_get_inner_html(handle: i32) -> i32;
    fn js_dom_set_text(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_get_text(handle: i32) -> i32;
    fn js_dom_set_attribute(handle: i32, np: i32, nl: i32, vp: i32, vl: i32) -> i32;
    fn js_dom_get_attribute(handle: i32, np: i32, nl: i32) -> i32;
    fn js_dom_remove_attribute(handle: i32, np: i32, nl: i32) -> i32;
    fn js_dom_add_class(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_remove_class(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_toggle_class(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_has_class(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_set_style(handle: i32, pp: i32, pl: i32, vp: i32, vl: i32) -> i32;
    fn js_dom_get_style(handle: i32, pp: i32, pl: i32) -> i32;
    fn js_dom_set_value(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_dom_get_value(handle: i32) -> i32;
    fn js_dom_is_valid(handle: i32) -> i32;
    fn js_dom_parent(handle: i32) -> i32;

    fn js_events_listen(handle: i32, tp: i32, tl: i32) -> i32;
    fn js_events_unlisten(listener_id: i32) -> i32;

    fn js_timers_set_timeout(delay_ms: i32) -> i32;
    fn js_timers_clear_timeout(id: i32);
    fn js_timers_set_interval(delay_ms: i32) -> i32;
    fn js_timers_clear_interval(id: i32);
    fn js_timers_request_animation_frame() -> i32;
    fn js_timers_cancel_animation_frame(id: i32);

    fn js_storage_get(area: i32, kp: i32, kl: i32) -> i32;
    fn js_storage_set(area: i32, kp: i32, kl: i32, vp: i32, vl: i32) -> i32;
    fn js_storage_remove(area: i32, kp: i32, kl: i32) -> i32;
    fn js_storage_clear(area: i32) -> i32;
    fn js_storage_has(area: i32, kp: i32, kl: i32) -> i32;

    fn js_fetch(method: i32, up: i32, ul: i32, bp: i32, bl: i32, cp: i32, cl: i32) -> i32;
    fn js_fetch_status(request_id: i32) -> i32;

    fn js_random_f64() -> f64;

    fn js_time_now_ms() -> f64;
    fn js_time_perf_now_ms() -> f64;

    fn js_canvas_set_size(handle: i32, w: i32, h: i32) -> i32;
    fn js_canvas_clear(handle: i32) -> i32;
    fn js_canvas_fill_style(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_canvas_stroke_style(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_canvas_line_width(handle: i32, w: f64) -> i32;
    fn js_canvas_fill_rect(handle: i32, x: f64, y: f64, w: f64, h: f64) -> i32;
    fn js_canvas_stroke_rect(handle: i32, x: f64, y: f64, w: f64, h: f64) -> i32;
    fn js_canvas_clear_rect(handle: i32, x: f64, y: f64, w: f64, h: f64) -> i32;
    fn js_canvas_begin_path(handle: i32) -> i32;
    fn js_canvas_move_to(handle: i32, x: f64, y: f64) -> i32;
    fn js_canvas_line_to(handle: i32, x: f64, y: f64) -> i32;
    fn js_canvas_arc(handle: i32, x: f64, y: f64, r: f64, a0: f64, a1: f64) -> i32;
    fn js_canvas_close_path(handle: i32) -> i32;
    fn js_canvas_fill(handle: i32) -> i32;
    fn js_canvas_stroke(handle: i32) -> i32;
    fn js_canvas_font(handle: i32, ptr: i32, len: i32) -> i32;
    fn js_canvas_fill_text(handle: i32, ptr: i32, len: i32, x: f64, y: f64) -> i32;
}

/// Wołane przez `core_runtime.rs` dla `hsh_print`/`hsh_println` (czyli
/// H#'owego `write()`). Wypisuje przez `console.log` po stronie JS.
pub(crate) fn host_write(msg: &str, newline: bool) {
    let bytes = msg.as_bytes();
    unsafe { js_write(newline as i32, bytes.as_ptr() as i32, bytes.len() as i32) };
}

unsafe fn read_back(ptr: i32) -> *mut c_char {
    ptr as *mut c_char
}

// ── console ─────────────────────────────────────────────────────────────

macro_rules! console_fn {
    ($name:ident, $level:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(msg: *const c_char) {
            let m = s(msg);
            js_console($level, m.as_ptr() as i32, m.len() as i32);
        }
    };
}
console_fn!(hweb_console_log, 0);
console_fn!(hweb_console_info, 1);
console_fn!(hweb_console_warn, 2);
console_fn!(hweb_console_error, 3);
console_fn!(hweb_console_debug, 4);

#[no_mangle]
pub unsafe extern "C" fn hweb_console_clear() {
    js_console_clear();
}
#[no_mangle]
pub unsafe extern "C" fn hweb_console_group(label: *const c_char) {
    let m = s(label);
    js_console_group(m.as_ptr() as i32, m.len() as i32);
}
#[no_mangle]
pub unsafe extern "C" fn hweb_console_group_end() {
    js_console_group_end();
}

// ── dom ──────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hweb_dom_get_by_id(id: *const c_char) -> i64 {
    let m = s(id);
    js_dom_get_by_id(m.as_ptr() as i32, m.len() as i32) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_query_selector(sel: *const c_char) -> i64 {
    let m = s(sel);
    js_dom_query_selector(m.as_ptr() as i32, m.len() as i32) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_query_selector_all(sel: *const c_char) -> *mut c_char {
    let m = s(sel);
    read_back(js_dom_query_selector_all(m.as_ptr() as i32, m.len() as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_create_element(tag: *const c_char) -> i64 {
    let m = s(tag);
    js_dom_create_element(m.as_ptr() as i32, m.len() as i32) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_body() -> i64 {
    js_dom_body() as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_head() -> i64 {
    js_dom_head() as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_append_child(parent: i64, child: i64) -> i64 {
    as_bool(js_dom_append_child(parent as i32, child as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_prepend_child(parent: i64, child: i64) -> i64 {
    as_bool(js_dom_prepend_child(parent as i32, child as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_remove(handle: i64) -> i64 {
    as_bool(js_dom_remove(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_set_inner_html(handle: i64, html: *const c_char) -> i64 {
    let m = s(html);
    as_bool(js_dom_set_inner_html(handle as i32, m.as_ptr() as i32, m.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_get_inner_html(handle: i64) -> *mut c_char {
    read_back(js_dom_get_inner_html(handle as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_set_text(handle: i64, text: *const c_char) -> i64 {
    let m = s(text);
    as_bool(js_dom_set_text(handle as i32, m.as_ptr() as i32, m.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_get_text(handle: i64) -> *mut c_char {
    read_back(js_dom_get_text(handle as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_set_attribute(handle: i64, name: *const c_char, value: *const c_char) -> i64 {
    let n = s(name);
    let v = s(value);
    as_bool(js_dom_set_attribute(handle as i32, n.as_ptr() as i32, n.len() as i32, v.as_ptr() as i32, v.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_get_attribute(handle: i64, name: *const c_char) -> *mut c_char {
    let n = s(name);
    read_back(js_dom_get_attribute(handle as i32, n.as_ptr() as i32, n.len() as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_remove_attribute(handle: i64, name: *const c_char) -> i64 {
    let n = s(name);
    as_bool(js_dom_remove_attribute(handle as i32, n.as_ptr() as i32, n.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_add_class(handle: i64, class_name: *const c_char) -> i64 {
    let n = s(class_name);
    as_bool(js_dom_add_class(handle as i32, n.as_ptr() as i32, n.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_remove_class(handle: i64, class_name: *const c_char) -> i64 {
    let n = s(class_name);
    as_bool(js_dom_remove_class(handle as i32, n.as_ptr() as i32, n.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_toggle_class(handle: i64, class_name: *const c_char) -> i64 {
    let n = s(class_name);
    as_bool(js_dom_toggle_class(handle as i32, n.as_ptr() as i32, n.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_has_class(handle: i64, class_name: *const c_char) -> i64 {
    let n = s(class_name);
    as_bool(js_dom_has_class(handle as i32, n.as_ptr() as i32, n.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_set_style(handle: i64, prop: *const c_char, value: *const c_char) -> i64 {
    let p = s(prop);
    let v = s(value);
    as_bool(js_dom_set_style(handle as i32, p.as_ptr() as i32, p.len() as i32, v.as_ptr() as i32, v.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_get_style(handle: i64, prop: *const c_char) -> *mut c_char {
    let p = s(prop);
    read_back(js_dom_get_style(handle as i32, p.as_ptr() as i32, p.len() as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_set_value(handle: i64, value: *const c_char) -> i64 {
    let v = s(value);
    as_bool(js_dom_set_value(handle as i32, v.as_ptr() as i32, v.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_get_value(handle: i64) -> *mut c_char {
    read_back(js_dom_get_value(handle as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_is_valid(handle: i64) -> i64 {
    as_bool(js_dom_is_valid(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_dom_parent(handle: i64) -> i64 {
    js_dom_parent(handle as i32) as i64
}

// ── events ───────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hweb_events_listen(handle: i64, event_type: *const c_char) -> i64 {
    let t = s(event_type);
    js_events_listen(handle as i32, t.as_ptr() as i32, t.len() as i32) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_events_unlisten(listener_id: i64) -> i64 {
    as_bool(js_events_unlisten(listener_id as i32) as i64)
}

// ── timers ───────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hweb_timers_set_timeout(delay_ms: i64) -> i64 {
    js_timers_set_timeout(delay_ms as i32) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_timers_clear_timeout(id: i64) {
    js_timers_clear_timeout(id as i32);
}
#[no_mangle]
pub unsafe extern "C" fn hweb_timers_set_interval(delay_ms: i64) -> i64 {
    js_timers_set_interval(delay_ms as i32) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_timers_clear_interval(id: i64) {
    js_timers_clear_interval(id as i32);
}
#[no_mangle]
pub unsafe extern "C" fn hweb_timers_request_animation_frame() -> i64 {
    js_timers_request_animation_frame() as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_timers_cancel_animation_frame(id: i64) {
    js_timers_cancel_animation_frame(id as i32);
}

// ── storage ──────────────────────────────────────────────────────────────

const AREA_LOCAL: i32 = 0;
const AREA_SESSION: i32 = 1;

#[no_mangle]
pub unsafe extern "C" fn hweb_storage_local_get(key: *const c_char) -> *mut c_char {
    let k = s(key);
    read_back(js_storage_get(AREA_LOCAL, k.as_ptr() as i32, k.len() as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_local_set(key: *const c_char, value: *const c_char) -> i64 {
    let k = s(key);
    let v = s(value);
    as_bool(js_storage_set(AREA_LOCAL, k.as_ptr() as i32, k.len() as i32, v.as_ptr() as i32, v.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_local_remove(key: *const c_char) -> i64 {
    let k = s(key);
    as_bool(js_storage_remove(AREA_LOCAL, k.as_ptr() as i32, k.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_local_clear() -> i64 {
    as_bool(js_storage_clear(AREA_LOCAL) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_local_has(key: *const c_char) -> i64 {
    let k = s(key);
    as_bool(js_storage_has(AREA_LOCAL, k.as_ptr() as i32, k.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_session_get(key: *const c_char) -> *mut c_char {
    let k = s(key);
    read_back(js_storage_get(AREA_SESSION, k.as_ptr() as i32, k.len() as i32))
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_session_set(key: *const c_char, value: *const c_char) -> i64 {
    let k = s(key);
    let v = s(value);
    as_bool(js_storage_set(AREA_SESSION, k.as_ptr() as i32, k.len() as i32, v.as_ptr() as i32, v.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_session_remove(key: *const c_char) -> i64 {
    let k = s(key);
    as_bool(js_storage_remove(AREA_SESSION, k.as_ptr() as i32, k.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_session_clear() -> i64 {
    as_bool(js_storage_clear(AREA_SESSION) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_storage_session_has(key: *const c_char) -> i64 {
    let k = s(key);
    as_bool(js_storage_has(AREA_SESSION, k.as_ptr() as i32, k.len() as i32) as i64)
}

// ── fetch ────────────────────────────────────────────────────────────────

const METHOD_GET: i32 = 0;
const METHOD_POST: i32 = 1;

#[no_mangle]
pub unsafe extern "C" fn hweb_fetch_get(url: *const c_char) -> i64 {
    let u = s(url);
    js_fetch(METHOD_GET, u.as_ptr() as i32, u.len() as i32, 0, 0, 0, 0) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_fetch_post(url: *const c_char, body: *const c_char, content_type: *const c_char) -> i64 {
    let u = s(url);
    let b = s(body);
    let c = s(content_type);
    js_fetch(
        METHOD_POST,
        u.as_ptr() as i32, u.len() as i32,
        b.as_ptr() as i32, b.len() as i32,
        c.as_ptr() as i32, c.len() as i32,
    ) as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_fetch_status(request_id: i64) -> i64 {
    js_fetch_status(request_id as i32) as i64
}

// ── random ───────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hweb_random_f64() -> f64 {
    js_random_f64()
}
#[no_mangle]
pub unsafe extern "C" fn hweb_random_int(min: i64, max: i64) -> i64 {
    if max <= min {
        return min;
    }
    let span = (max - min + 1) as f64;
    min + (js_random_f64() * span).floor() as i64
}
#[no_mangle]
pub unsafe extern "C" fn hweb_random_hex(n_bytes: i64) -> *mut c_char {
    let n = n_bytes.max(0);
    let mut out = String::with_capacity((n * 2) as usize);
    for _ in 0..n {
        let byte = (js_random_f64() * 256.0) as u8;
        out.push_str(&format!("{:02x}", byte));
    }
    leak(&out)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_random_uuid_v4() -> *mut c_char {
    let mut bytes = [0u8; 16];
    for b in bytes.iter_mut() {
        *b = (js_random_f64() * 256.0) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]
    );
    leak(&uuid)
}

// ── time ─────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hweb_time_now_ms() -> f64 {
    js_time_now_ms()
}
#[no_mangle]
pub unsafe extern "C" fn hweb_time_perf_now_ms() -> f64 {
    js_time_perf_now_ms()
}

// ── canvas ───────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_set_size(handle: i64, width: i64, height: i64) -> i64 {
    as_bool(js_canvas_set_size(handle as i32, width as i32, height as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_clear(handle: i64) -> i64 {
    as_bool(js_canvas_clear(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_fill_style(handle: i64, css_color: *const c_char) -> i64 {
    let m = s(css_color);
    as_bool(js_canvas_fill_style(handle as i32, m.as_ptr() as i32, m.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_stroke_style(handle: i64, css_color: *const c_char) -> i64 {
    let m = s(css_color);
    as_bool(js_canvas_stroke_style(handle as i32, m.as_ptr() as i32, m.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_line_width(handle: i64, width: f64) -> i64 {
    as_bool(js_canvas_line_width(handle as i32, width) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_fill_rect(handle: i64, x: f64, y: f64, w: f64, h: f64) -> i64 {
    as_bool(js_canvas_fill_rect(handle as i32, x, y, w, h) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_stroke_rect(handle: i64, x: f64, y: f64, w: f64, h: f64) -> i64 {
    as_bool(js_canvas_stroke_rect(handle as i32, x, y, w, h) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_clear_rect(handle: i64, x: f64, y: f64, w: f64, h: f64) -> i64 {
    as_bool(js_canvas_clear_rect(handle as i32, x, y, w, h) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_begin_path(handle: i64) -> i64 {
    as_bool(js_canvas_begin_path(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_move_to(handle: i64, x: f64, y: f64) -> i64 {
    as_bool(js_canvas_move_to(handle as i32, x, y) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_line_to(handle: i64, x: f64, y: f64) -> i64 {
    as_bool(js_canvas_line_to(handle as i32, x, y) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_arc(handle: i64, x: f64, y: f64, radius: f64, start_rad: f64, end_rad: f64) -> i64 {
    as_bool(js_canvas_arc(handle as i32, x, y, radius, start_rad, end_rad) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_close_path(handle: i64) -> i64 {
    as_bool(js_canvas_close_path(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_fill(handle: i64) -> i64 {
    as_bool(js_canvas_fill(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_stroke(handle: i64) -> i64 {
    as_bool(js_canvas_stroke(handle as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_font(handle: i64, css_font: *const c_char) -> i64 {
    let m = s(css_font);
    as_bool(js_canvas_font(handle as i32, m.as_ptr() as i32, m.len() as i32) as i64)
}
#[no_mangle]
pub unsafe extern "C" fn hweb_canvas_fill_text(handle: i64, text: *const c_char, x: f64, y: f64) -> i64 {
    let m = s(text);
    as_bool(js_canvas_fill_text(handle as i32, m.as_ptr() as i32, m.len() as i32, x, y) as i64)
}
