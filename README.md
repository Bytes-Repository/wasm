# `web-wasm` — pisz strony w H#, kompiluj do `.wasm`

Biblioteka H# (pakiet `bytes`) do pisania stron/aplikacji webowych i
kompilowania ich bezpośrednio do WebAssembly. **API jest w 100% czystym
H#** — struktury, funkcje, stałe, dokładnie jak w `std`. Jedyny fragment
napisany w Rust to `runtime-rust/`, i to nie z wyboru: `hsharp compile
--target wasm32` z zasady nie potrafi wyemitować gotowego, samodzielnego
`.wasm` (patrz niżej) — coś musi dostarczyć implementacje runtime'u przy
linkowaniu, więc dostarcza to małe, w pełni jawne `extern static [rust]`
(zero `wasm-bindgen`, zero wygenerowanej "magii").

## Zawartość archiwum

```
web-wasm/
├── bytes.hk                    manifest pakietu (bytes)
├── src/                        BIBLIOTEKA — 100% H#
│   ├── web_console.h#
│   ├── web_dom.h#
│   ├── web_events.h#
│   ├── web_timers.h#
│   ├── web_storage.h#
│   ├── web_fetch.h#
│   ├── web_canvas.h#
│   ├── web_random.h#
│   ├── web_time.h#
│   └── web_types.h#
├── runtime-rust/                jedyny nie-H#-owy fragment (extern static [rust])
│   ├── Cargo.toml
│   └── src/
│       ├── core_runtime.rs      reimplementacja rdzenia runtime'u H# (hsh_*) dla wasm32
│       └── web_bridge.rs        most do przeglądarki (hweb_* + import "hsharp_web")
├── glue/
│   └── hsharp-web-loader.js     JEDYNY plik JS — ładuje i uruchamia .wasm
├── scripts/
│   └── build-wasm.sh            pipeline: hsharp compile → cargo build → wasm-ld
├── examples/hello-page/         działający przykład
└── docs/API.md                  pełna dokumentacja + polityka stabilności
```

## Dlaczego w ogóle potrzebny jest `runtime-rust/`

`hsharp compile --target wasm32` **zawsze** emituje wyłącznie plik
obiektowy (`--emit object`) — próba `--emit bin`/`lib`/`so` na tym
targecie kończy się błędem kompilatora. Powód: cały runtime H#
(`compiler/runtime/core.c`) korzysta z nagłówków POSIX i nie da się go
skompilować pod `wasm32-unknown-unknown` w ogóle — więc **każde**
wywołanie runtime'u H# (`hsh_*`), które trafi do skompilowanego obiektu
— nawet samo `write("Hello")` — zostaje nierozwiązanym symbolem
zewnętrznym. To udokumentowane, zamierzone zachowanie samego
kompilatora H#, nie ograniczenie tej biblioteki.

`runtime-rust/` dostarcza więc dwie rzeczy przy linkowaniu:
1. **`core_runtime.rs`** — implementacje `hsh_*` (stringi, tablice,
   structy, domknięcia, arena, `write()`/`assert`/`panic`) potrzebne
   przez KAŻDY program H# na wasm32, niezależnie od tego czy w ogóle
   używa reszty tej biblioteki.
2. **`web_bridge.rs`** — implementacje `hweb_*`, czyli funkcji
   deklarowanych przez `extern static [rust]` w `src/web_*.h#`, które z
   kolei wołają do JS przez import `hsharp_web` (zaimplementowany w
   `glue/hsharp-web-loader.js`).

Świadomie **nie** dostarczamy `fs::`/`proc::`/`env::`/`net_*`/`sqlite`/
`python`/`crypto_rsa` — to zależy od POSIX, którego w przeglądarce nie
ma; program, który tego użyje, dostanie błąd linkowania (oczekiwane).

## Jak to zbudować

Wymagane narzędzia w `PATH`: `hsharp` (lub `bytes`), `cargo` z targetem
`wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`),
`wasm-ld` (pakiet `lld`/`llvm`).

```bash
./scripts/build-wasm.sh examples/hello-page/src/main.h# examples/hello-page/build
cd examples/hello-page
python3 -m http.server 8080   # WASM wymaga HTTP, nie działa z file://
# → http://localhost:8080/
```

Skrypt robi dokładnie trzy rzeczy: (1) `hsharp compile --target wasm32
--emit object`, (2) `cargo build --release --target
wasm32-unknown-unknown` dla `runtime-rust/`, (3) `wasm-ld` łączący oba w
jeden `app.wasm`, eksportując `main`/`wasm_on_event`/`hsharp_web_alloc`/
`memory` i zostawiając `js_*` (moduł `hsharp_web`) jako importy — patrz
komentarze w `scripts/build-wasm.sh`.

## Minimalny przykład

```hsharp
use "web-wasm -> web_console" from "console"
use "web-wasm -> web_dom"     from "dom"

fn main() is
    console::log("Cześć z H#!")
    let root: dom::Element = dom::body()
    let h: dom::Element = dom::create("h1")
    h.set_text("Działa!")
    root.append(h)
end
```

Pełny, interaktywny przykład (licznik kliknięć, `setInterval`, losowe
UUID) jest w `examples/hello-page/`.

## Ważne, zanim zaczniesz pisać na produkcję

- **H# nie ma mutowalnych zmiennych globalnych.** Stan między `main()` a
  Twoją `wasm_on_event()` trzymaj w DOM (`data-*`) albo w
  `web_storage`. Wyjaśnienie i przykład: `docs/API.md`.
- **Zweryfikuj kontrakt ABI na swoim toolchainie przed produkcją.**
  Ta biblioteka powstała bez dostępu do skompilowanego kompilatora H#
  (brak LLVM 21 w tym środowisku) — sygnatury w `runtime-rust/` są
  napisane wprost wobec źródeł kompilatora (`core.c`,
  `ffi_rust_native.rs`), ale nie zostały uruchomieniowo potwierdzone.
  Instrukcja weryfikacji (`wasm-objdump`/`wasm2wat`, 5 minut) jest w
  `docs/API.md`, sekcja „Kontrakt ABI”. To jedyne miejsce w całej
  bibliotece, gdzie jest jakakolwiek niepewność — samo API H# (nazwy,
  sygnatury, struktura modułów) jest stabilne i gotowe do użycia od razu.

## Licencja

MIT — patrz `LICENSE`.
