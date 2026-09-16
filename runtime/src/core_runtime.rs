use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// ─────────────────────────────────────────────────────────────────────────
// Pomocnicze konwersje C-string <-> Rust
// ─────────────────────────────────────────────────────────────────────────

/// Odczytuje `hsh_string` (`const char*`, może być null) jako `&str`.
/// Zgodnie z resztą runtime'u H#, nieprawidłowy UTF-8 jest tolerowany
/// stratnie (`to_string_lossy`), a `null` traktowany jak `""`.
unsafe fn read_str<'a>(s: *const c_char) -> std::borrow::Cow<'a, str> {
    if s.is_null() {
        return std::borrow::Cow::Borrowed("");
    }
    CStr::from_ptr(s).to_string_lossy()
}

/// Zwraca nowo zaalokowany, "przeciekający" (leaked) C-string — dokładnie
/// jak `core.c`, które też nigdy nie zwalnia pojedynczych zwracanych
/// stringów (patrz komentarz przy `hsh_alloc` w oryginale: "today's
/// codegen never calls those per-object frees at all").
fn leak_cstring(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        // s zawierał bajt NUL w środku — obetnij tam, tak jak zrobiłoby
        // to zapisanie przez strcpy/snprintf w oryginalnym C. Bajt 0x00
        // nigdy nie występuje jako bajt kontynuacji w poprawnym UTF-8,
        // więc cięcie w tym miejscu zawsze trafia na granicę znaku.
        Err(_) => {
            let bytes = s.as_bytes();
            let cut_at = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            CString::new(&bytes[..cut_at]).unwrap_or_default().into_raw()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Core I/O — write() / print
// ─────────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hsh_print(s: *const c_char) {
    crate::web_bridge::host_write(&read_str(s), false);
}

#[no_mangle]
pub unsafe extern "C" fn hsh_println(s: *const c_char) {
    crate::web_bridge::host_write(&read_str(s), true);
}

#[no_mangle]
pub unsafe extern "C" fn hsh_atoll(s: *const c_char) -> i64 {
    read_str(s).trim().parse::<i64>().unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_atof(s: *const c_char) -> f64 {
    read_str(s).trim().parse::<f64>().unwrap_or(0.0)
}

#[no_mangle]
pub extern "C" fn hsh_int_to_string(n: i64) -> *mut c_char {
    leak_cstring(&n.to_string())
}

#[no_mangle]
pub extern "C" fn hsh_float_to_string(n: f64) -> *mut c_char {
    // %g z printf: skrócona reprezentacja, bez zer wiodących po przecinku.
    leak_cstring(&format_g(n))
}

fn format_g(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{}", n);
        s
    }
}

#[no_mangle]
pub unsafe extern "C" fn hsh_strlen(s: *const c_char) -> i64 {
    if s.is_null() {
        return 0;
    }
    CStr::from_ptr(s).to_bytes().len() as i64
}

#[no_mangle]
pub unsafe extern "C" fn hsh_strcat(a: *const c_char, b: *const c_char) -> *mut c_char {
    let a = read_str(a);
    let b = read_str(b);
    leak_cstring(&format!("{}{}", a, b))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_assert(cond: i8, msg: *const c_char) {
    if cond == 0 {
        let m = read_str(msg);
        crate::web_bridge::host_write(&format!("assertion failed: {}", m), true);
        std::process::abort();
    }
}

#[no_mangle]
pub unsafe extern "C" fn hsh_panic(msg: *const c_char) {
    let m = read_str(msg);
    crate::web_bridge::host_write(&format!("panic: {}", m), true);
    std::process::abort();
}

// RAII drop stuby — jak w oryginale: świadomie nic nie robią (H#'owy
// codegen dziś nigdy nie zwalnia pojedynczych obiektów, patrz wyżej).
#[no_mangle] pub extern "C" fn hsh_string_free(_s: *const c_char) {}
#[no_mangle] pub extern "C" fn hsh_bytes_free(_b: *mut u8) {}
#[no_mangle] pub extern "C" fn hsh_array_free(_a: *mut HshArray) {}
#[no_mangle] pub extern "C" fn hsh_struct_free(_p: *mut i64) {}

// ─────────────────────────────────────────────────────────────────────────
// Arena — bump allocator, jeden-do-jednego z core.c (te same kind'y:
// 0=General 1=Fixed 2=Pool 3=Page 4=Ring)
// ─────────────────────────────────────────────────────────────────────────

#[repr(C)]
pub struct HshArena {
    base: *mut u8,
    cap: u64,
    used: u64,
    kind: i64,
}

const ARENA_KIND_FIXED: i64 = 1;
const ARENA_KIND_POOL: i64 = 2;
const ARENA_KIND_PAGE: i64 = 3;
const ARENA_KIND_RING: i64 = 4;

thread_local! {
    static ARENA_STACK: RefCell<Vec<*mut HshArena>> = RefCell::new(Vec::new());
}

#[no_mangle]
pub extern "C" fn hsh_arena_new_kind(cap: u64, kind: i64) -> *mut HshArena {
    let cap = cap.max(1);
    let layout = Layout::array::<u8>(cap as usize).unwrap();
    let base = unsafe { alloc(layout) };
    Box::into_raw(Box::new(HshArena { base, cap, used: 0, kind }))
}

#[no_mangle]
pub extern "C" fn hsh_arena_new(cap: u64) -> *mut HshArena {
    hsh_arena_new_kind(cap, 0)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_arena_alloc(a: *mut HshArena, n: u64) -> *mut u8 {
    if a.is_null() {
        return alloc(Layout::array::<u8>(n.max(1) as usize).unwrap());
    }
    let arena = &mut *a;
    let align: u64 = match arena.kind {
        k if k == ARENA_KIND_POOL => 64,
        k if k == ARENA_KIND_PAGE => 4096,
        _ => 8,
    };
    let aligned = (n + (align - 1)) & !(align - 1);
    if arena.used + aligned > arena.cap {
        if arena.kind == ARENA_KIND_FIXED {
            hsh_panic(leak_cstring(
                "arena(fixed) capacity exceeded — allocation would overflow the fixed-size arena",
            ));
        }
        if arena.kind == ARENA_KIND_RING && aligned <= arena.cap {
            arena.used = 0;
        } else {
            return alloc(Layout::array::<u8>(n.max(1) as usize).unwrap());
        }
    }
    let p = arena.base.add(arena.used as usize);
    arena.used += aligned;
    p
}

#[no_mangle]
pub unsafe extern "C" fn hsh_arena_free(a: *mut HshArena) {
    if a.is_null() {
        return;
    }
    let arena = Box::from_raw(a);
    if !arena.base.is_null() {
        dealloc(arena.base, Layout::array::<u8>(arena.cap as usize).unwrap());
    }
}

#[no_mangle]
pub extern "C" fn hsh_arena_push_current(a: *mut HshArena) {
    ARENA_STACK.with(|s| s.borrow_mut().push(a));
}

#[no_mangle]
pub extern "C" fn hsh_arena_pop_current() -> *mut HshArena {
    ARENA_STACK.with(|s| s.borrow_mut().pop().unwrap_or(std::ptr::null_mut()))
}

fn arena_current() -> *mut HshArena {
    ARENA_STACK.with(|s| s.borrow().last().copied().unwrap_or(std::ptr::null_mut()))
}

#[no_mangle]
pub extern "C" fn hsh_arena_checkpoint() -> i64 {
    let a = arena_current();
    if a.is_null() {
        return -1;
    }
    unsafe { (*a).used as i64 }
}

#[no_mangle]
pub extern "C" fn hsh_arena_rewind(mark: i64) {
    let a = arena_current();
    if a.is_null() || mark < 0 {
        return;
    }
    unsafe { (*a).used = mark as u64 };
}

#[no_mangle]
pub extern "C" fn hsh_arena_used() -> i64 {
    let a = arena_current();
    if a.is_null() {
        return 0;
    }
    unsafe { (*a).used as i64 }
}

#[no_mangle]
pub extern "C" fn hsh_arena_capacity() -> i64 {
    let a = arena_current();
    if a.is_null() {
        return 0;
    }
    unsafe { (*a).cap as i64 }
}

// Ogólny alokator "arena-aware", odpowiednik core.c `hsh_alloc`. Nie jest
// eksportowany (`hsh_alloc` w oryginale też jest `static`) — korzystają
// z niego tylko funkcje w tym pliku.
unsafe fn hsh_alloc(n: usize) -> *mut u8 {
    let a = arena_current();
    if a.is_null() {
        alloc(Layout::array::<u8>(n.max(1)).unwrap())
    } else {
        hsh_arena_alloc(a, n as u64)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Stringi
// ─────────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hsh_trim(s: *const c_char) -> *mut c_char {
    leak_cstring(read_str(s).trim_matches(|c| c == ' ' || c == '\t' || c == '\n' || c == '\r'))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_contains(h: *const c_char, n: *const c_char) -> i64 {
    (!h.is_null() && !n.is_null() && read_str(h).contains(read_str(n).as_ref())) as i64
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_index_of(s: *const c_char, sub: *const c_char) -> i64 {
    if s.is_null() || sub.is_null() {
        return -1;
    }
    // Bajtowy (nie znakowy) offset — zgodnie z oryginałem opartym o strstr.
    let hay = CStr::from_ptr(s).to_bytes();
    let needle = CStr::from_ptr(sub).to_bytes();
    if needle.is_empty() {
        return 0;
    }
    hay.windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p as i64)
        .unwrap_or(-1)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_to_upper(s: *const c_char) -> *mut c_char {
    leak_cstring(&read_str(s).to_uppercase())
}

#[no_mangle]
pub unsafe extern "C" fn hsh_to_lower(s: *const c_char) -> *mut c_char {
    leak_cstring(&read_str(s).to_lowercase())
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_replace(
    s: *const c_char,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    let s = read_str(s);
    let from = read_str(from);
    let to = read_str(to);
    if from.is_empty() {
        return leak_cstring(&s);
    }
    leak_cstring(&s.replace(from.as_ref(), to.as_ref()))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_starts_with(s: *const c_char, prefix: *const c_char) -> i64 {
    (!s.is_null() && !prefix.is_null() && read_str(s).starts_with(read_str(prefix).as_ref())) as i64
}

#[no_mangle]
pub unsafe extern "C" fn hsh_ends_with(s: *const c_char, suffix: *const c_char) -> i64 {
    (!s.is_null() && !suffix.is_null() && read_str(s).ends_with(read_str(suffix).as_ref())) as i64
}

#[no_mangle]
pub unsafe extern "C" fn hsh_substr(s: *const c_char, start: i64, end_idx: i64) -> *mut c_char {
    if s.is_null() {
        return leak_cstring("");
    }
    let bytes = CStr::from_ptr(s).to_bytes();
    let len = bytes.len() as i64;
    let start = start.max(0);
    let end = if end_idx < 0 || end_idx > len { len } else { end_idx };
    if start >= end {
        return leak_cstring("");
    }
    let slice = &bytes[start as usize..end as usize];
    leak_cstring(&String::from_utf8_lossy(slice))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_int_to_str(n: i64) -> *mut c_char {
    hsh_int_to_string(n)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_to_int(s: *const c_char) -> i64 {
    hsh_atoll(s)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_to_int(s: *const c_char) -> i64 {
    hsh_atoll(s)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_to_int_from_hex(s: *const c_char) -> i64 {
    let s = read_str(s);
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    i64::from_str_radix(s, 16).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_to_float(s: *const c_char) -> f64 {
    hsh_atof(s)
}

#[no_mangle]
pub extern "C" fn hsh_conv_int_to_hex(n: i64) -> *mut c_char {
    leak_cstring(&format!("{:x}", n))
}

#[no_mangle]
pub extern "C" fn hsh_conv_float_to_int(f: f64) -> i64 {
    f as i64
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_lower(s: *const c_char) -> *const c_char {
    hsh_to_lower(s)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_upper(s: *const c_char) -> *const c_char {
    hsh_to_upper(s)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_trim_right(s: *const c_char) -> *const c_char {
    leak_cstring(read_str(s).trim_end_matches(|c| c == ' ' || c == '\t' || c == '\n' || c == '\r'))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_at(s: *const c_char, idx: i64) -> *const c_char {
    if s.is_null() || idx < 0 {
        return leak_cstring("");
    }
    let bytes = CStr::from_ptr(s).to_bytes();
    match bytes.get(idx as usize) {
        Some(&b) => leak_cstring(&(b as char).to_string()),
        None => leak_cstring(""),
    }
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_slice(s: *const c_char, start: i64, end: i64) -> *const c_char {
    hsh_substr(s, start, end)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_find(haystack: *const c_char, needle: *const c_char) -> i64 {
    hsh_str_index_of(haystack, needle)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_rfind(haystack: *const c_char, needle: *const c_char) -> i64 {
    if haystack.is_null() || needle.is_null() {
        return -1;
    }
    let hay = CStr::from_ptr(haystack).to_bytes();
    let needle = CStr::from_ptr(needle).to_bytes();
    if needle.is_empty() {
        return hay.len() as i64;
    }
    if needle.len() > hay.len() {
        return -1;
    }
    for start in (0..=hay.len() - needle.len()).rev() {
        if &hay[start..start + needle.len()] == needle {
            return start as i64;
        }
    }
    -1
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_pad_right(s: *const c_char, width: i64) -> *const c_char {
    let s = read_str(s);
    let width = width.max(0) as usize;
    if s.chars().count() >= width {
        return leak_cstring(&s);
    }
    let pad = width - s.chars().count();
    leak_cstring(&format!("{}{}", s, " ".repeat(pad)))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_repeat(s: *const c_char, n: i64) -> *const c_char {
    let s = read_str(s);
    leak_cstring(&s.repeat(n.max(0) as usize))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_to_char_code(s: *const c_char) -> i64 {
    read_str(s).chars().next().map(|c| c as i64).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn hsh_char_code_to_str(code: i64) -> *mut c_char {
    let c = char::from_u32(code.max(0) as u32).unwrap_or('\u{FFFD}');
    leak_cstring(&c.to_string())
}

#[no_mangle]
pub unsafe extern "C" fn hsh_val_to_str(v: i64) -> *mut c_char {
    hsh_int_to_string(v)
}

// ─────────────────────────────────────────────────────────────────────────
// Tablice — HshArray { len: i64, cap: i64, data: [i64; N] }
// ─────────────────────────────────────────────────────────────────────────

#[repr(C)]
pub struct HshArray {
    len: i64,
    cap: i64,
}

unsafe fn arr_data_ptr(a: *mut HshArray) -> *mut i64 {
    (a as *mut u8).add(std::mem::size_of::<HshArray>()) as *mut i64
}

unsafe fn arr_alloc(cap: i64) -> *mut HshArray {
    let cap = cap.max(4);
    let total = std::mem::size_of::<HshArray>() + (cap as usize) * 8;
    let layout = Layout::from_size_align(total, 8).unwrap();
    let raw = alloc(layout) as *mut HshArray;
    (*raw).len = 0;
    (*raw).cap = cap;
    raw
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_new() -> *mut HshArray {
    arr_alloc(4)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_push(a: *mut HshArray, val: i64) -> *mut HshArray {
    let mut a = if a.is_null() { hsh_array_new() } else { a };
    if (*a).len >= (*a).cap {
        let new_cap = (*a).cap * 2;
        let b = arr_alloc(new_cap);
        (*b).len = (*a).len;
        let src = arr_data_ptr(a);
        let dst = arr_data_ptr(b);
        std::ptr::copy_nonoverlapping(src, dst, (*a).len as usize);
        a = b;
    }
    let data = arr_data_ptr(a);
    *data.add((*a).len as usize) = val;
    (*a).len += 1;
    a
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_len(a: *mut HshArray) -> i64 {
    if a.is_null() {
        0
    } else {
        (*a).len
    }
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_get(a: *mut HshArray, idx: i64) -> i64 {
    if a.is_null() || idx < 0 || idx >= (*a).len {
        return 0;
    }
    *arr_data_ptr(a).add(idx as usize)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_set(a: *mut HshArray, idx: i64, val: i64) -> *mut HshArray {
    if a.is_null() || idx < 0 || idx >= (*a).len {
        return a;
    }
    *arr_data_ptr(a).add(idx as usize) = val;
    a
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_concat(a: *mut HshArray, b: *mut HshArray) -> *mut HshArray {
    if a.is_null() {
        return if b.is_null() { hsh_array_new() } else { b };
    }
    if b.is_null() {
        return a;
    }
    let r = arr_alloc((*a).len + (*b).len);
    (*r).len = (*a).len + (*b).len;
    std::ptr::copy_nonoverlapping(arr_data_ptr(a), arr_data_ptr(r), (*a).len as usize);
    std::ptr::copy_nonoverlapping(
        arr_data_ptr(b),
        arr_data_ptr(r).add((*a).len as usize),
        (*b).len as usize,
    );
    r
}

#[no_mangle]
pub unsafe extern "C" fn hsh_array_contains(a: *mut HshArray, val: i64) -> i64 {
    if a.is_null() {
        return 0;
    }
    let data = arr_data_ptr(a);
    for i in 0..(*a).len {
        if *data.add(i as usize) == val {
            return 1;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn hsh_sort_strings(a: *mut HshArray) -> *mut HshArray {
    if a.is_null() {
        return hsh_array_new();
    }
    let r = arr_alloc((*a).len);
    (*r).len = (*a).len;
    std::ptr::copy_nonoverlapping(arr_data_ptr(a), arr_data_ptr(r), (*a).len as usize);
    let data = std::slice::from_raw_parts_mut(arr_data_ptr(r), (*r).len as usize);
    data.sort_by(|&x, &y| {
        let sx = read_str(x as *const c_char);
        let sy = read_str(y as *const c_char);
        sx.as_ref().cmp(sy.as_ref())
    });
    r
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_split_whitespace(s: *const c_char) -> *mut HshArray {
    let out = hsh_array_new();
    let mut out = out;
    let s = read_str(s);
    for tok in s.split_whitespace() {
        let p = leak_cstring(tok) as i64;
        out = hsh_array_push(out, p);
    }
    out
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_split(s: *const c_char, sep: *const c_char) -> *mut HshArray {
    let mut out = hsh_array_new();
    if s.is_null() || sep.is_null() {
        return out;
    }
    let s = read_str(s);
    let sep = read_str(sep);
    if sep.is_empty() {
        let p = leak_cstring(&s) as i64;
        return hsh_array_push(out, p);
    }
    for part in s.split(sep.as_ref()) {
        let p = leak_cstring(part) as i64;
        out = hsh_array_push(out, p);
    }
    out
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_split_count(s: *const c_char, sep: *const c_char) -> i64 {
    hsh_array_len(hsh_string_split(s, sep))
}

#[no_mangle]
pub unsafe extern "C" fn hsh_str_split_part(s: *const c_char, sep: *const c_char, index: i64) -> *mut c_char {
    let arr = hsh_string_split(s, sep);
    let p = hsh_array_get(arr, index);
    if p == 0 {
        leak_cstring("")
    } else {
        p as *mut c_char
    }
}

#[no_mangle]
pub unsafe extern "C" fn hsh_string_chars(s: *const c_char) -> *mut HshArray {
    let mut out = hsh_array_new();
    let s = read_str(s);
    for c in s.chars() {
        let p = leak_cstring(&c.to_string()) as i64;
        out = hsh_array_push(out, p);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Structy — heap-allocated array of i64 fields, w kolejności deklaracji
// ─────────────────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn hsh_struct_new(n: i64) -> *mut i64 {
    let n = n.max(0) as usize;
    let p = hsh_alloc(n * 8) as *mut i64;
    std::ptr::write_bytes(p, 0, n);
    p
}

#[no_mangle]
pub unsafe extern "C" fn hsh_struct_get(s: *const i64, idx: i64) -> i64 {
    if s.is_null() {
        return 0;
    }
    *s.add(idx as usize)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_struct_set(s: *mut i64, idx: i64, val: i64) -> *mut i64 {
    if !s.is_null() {
        *s.add(idx as usize) = val;
    }
    s
}

// ─────────────────────────────────────────────────────────────────────────
// Domknięcia (closures)
// ─────────────────────────────────────────────────────────────────────────

#[repr(C)]
pub struct HshClosure {
    fn_ptr: i64,
    n_caps: i64,
    // przechwycone wartości następują bezpośrednio po nagłówku
}

unsafe fn closure_caps_ptr(c: *mut HshClosure) -> *mut i64 {
    (c as *mut u8).add(std::mem::size_of::<HshClosure>()) as *mut i64
}

#[no_mangle]
pub unsafe extern "C" fn hsh_closure_create(
    fn_ptr: i64,
    n_caps: i64,
    caps: *const i64,
) -> *mut HshClosure {
    let n = n_caps.max(0) as usize;
    let total = std::mem::size_of::<HshClosure>() + n * 8;
    let layout = Layout::from_size_align(total, 8).unwrap();
    let raw = alloc(layout) as *mut HshClosure;
    (*raw).fn_ptr = fn_ptr;
    (*raw).n_caps = n_caps;
    if n > 0 && !caps.is_null() {
        std::ptr::copy_nonoverlapping(caps, closure_caps_ptr(raw), n);
    }
    raw
}

type Closure1 = unsafe extern "C" fn(i64, i64) -> i64;
type Closure2 = unsafe extern "C" fn(i64, i64, i64) -> i64;

#[no_mangle]
pub unsafe extern "C" fn hsh_closure_call1(c: *mut HshClosure, a0: i64) -> i64 {
    if c.is_null() {
        return 0;
    }
    let f: Closure1 = std::mem::transmute((*c).fn_ptr as usize);
    f(closure_caps_ptr(c) as i64, a0)
}

#[no_mangle]
pub unsafe extern "C" fn hsh_closure_call2(c: *mut HshClosure, a0: i64, a1: i64) -> i64 {
    if c.is_null() {
        return 0;
    }
    let f: Closure2 = std::mem::transmute((*c).fn_ptr as usize);
    f(closure_caps_ptr(c) as i64, a0, a1)
}
