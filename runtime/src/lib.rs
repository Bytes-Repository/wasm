#![allow(clippy::missing_safety_doc)]

pub mod core_runtime;
pub mod web_bridge;

// Panik Rusta (z tego crate'a, NIE z H#'owego `hsh_panic` — ten ma
// własną ścieżkę w core_runtime.rs) też powinien wylądować w konsoli
// przeglądarki zamiast ginąć bez śladu.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn hsharp_runtime_install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        web_bridge::host_write(&format!("[hsharp-wasm-runtime panic] {}", info), true);
    }));
}
