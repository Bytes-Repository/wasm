# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/), wersjonowanie
opisane w `docs/API.md`.

## [0.1.0] — pierwsze wydanie

### Dodano
- `web_console`, `web_dom`, `web_events`, `web_timers`, `web_storage`,
  `web_fetch`, `web_canvas`, `web_random`, `web_time`, `web_types` —
  100% H#, zero zależności w `bytes.hk`.
- `runtime-rust/` — reimplementacja podzbioru `runtime/core.c` (`hsh_*`)
  na `wasm32-unknown-unknown` + most do przeglądarki (`hweb_*` / import
  `hsharp_web`), jedyny nie-H#-owy fragment pakietu.
- `glue/hsharp-web-loader.js` — jedyny plik JS, implementuje most
  `hsharp_web`, ładuje i uruchamia skompilowany moduł `.wasm`.
- `scripts/build-wasm.sh` — pipeline `hsharp compile --emit object
  --target wasm32` → `cargo build --target wasm32-unknown-unknown` →
  `wasm-ld`.
- `examples/hello-page` — działający przykład (licznik kliknięć + timer +
  losowy UUID), pokazuje wzorzec przechowywania stanu w DOM.

### Znane ograniczenia (patrz `docs/API.md`)
- Brak `fs::`/`proc::`/`env::`/`net_*`/`sqlite`/`python`/`crypto_rsa` —
  świadomie, zależą od POSIX.
- Brak przekazywania domknięć H# przez `extern static [rust]` — patrz
  „Model zdarzeń” w `docs/API.md`.
- Kontrakt ABI (typy parametrów `extern static [rust]`) opisany w
  `docs/API.md` nie został zweryfikowany uruchomieniowo w tym środowisku
  (brak toolchainu LLVM 21 + H# w sandboxie, w którym ta biblioteka
  powstała) — zweryfikuj przed produkcją zgodnie z instrukcją tamże.
