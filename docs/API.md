# `web-wasm` — dokumentacja API i polityka stabilności (v0.1.0)

## Polityka stabilności

`web-wasm` istnieje po to, żeby dawało się na nim polegać. Zasady:

1. **Żadna publiczna funkcja/struktura/stała z `src/*.h#` nie zmieni nazwy,
   liczby parametrów ani ich typów bez zmiany wersji MAJOR** (semver:
   `MAJOR.MINOR.PATCH`, mimo że jesteśmy jeszcze przed 1.0.0 — dla tej
   biblioteki 0.x.y traktujemy zmianę pierwszej cyfry po zerze jak MAJOR).
2. Nowe funkcje mogą być dodawane w wersji MINOR. Nigdy nie usuwamy ani
   nie zmieniamy istniejącej sygnatury w MINOR/PATCH — najwyżej oznaczamy
   ją jako przestarzałą w komentarzu i utrzymujemy dalej.
3. Wartości liczbowe `KIND_*` (`web_events.h#`) są częścią zamrożonego
   ABI między H# a `glue/hsharp-web-loader.js` — traktowane jak MAJOR.
4. Nazwy eksportów wasm, których oczekuje loader (`main`, `wasm_on_event`,
   `hsharp_web_alloc`, `hsharp_runtime_install_panic_hook`, `memory`) są
   również częścią zamrożonego ABI.
5. Każda zmiana łamiąca którykolwiek z powyższych punktów trafia do
   `CHANGELOG.md` z jawnym nagłówkiem „BREAKING”.

## Kontrakt ABI (dlaczego to działa i gdzie leży ryzyko)

`web-wasm` jest w 100% czystym H# po stronie API. Jedyny "nie-H#"
fragment to `runtime-rust/` — i to nie z wyboru stylistycznego, tylko dlatego,
że `hsharp compile --target wasm32` **zawsze** zostawia każde wywołanie
runtime'u H# (`hsh_*`) jako nierozwiązany symbol (patrz README.md,
sekcja „Dlaczego potrzebny jest `runtime-rust/`”) — dotyczy to KAŻDEGO
programu H# kompilowanego na wasm32, nawet `write("Hello")`, niezależnie
od tego czy w ogóle korzysta z tej biblioteki webowej.

Konwencje przyjęte przy pisaniu `runtime-rust/` (potwierdzone bezpośrednio
w źródłach kompilatora: `compiler/runtime/core.c` i
`compiler/src/ffi_rust_native.rs`):

| H# typ     | Reprezentacja przy `extern static [rust]`                     |
|------------|-----------------------------------------------------------------|
| `string`   | wskaźnik do C-stringa zakończonego zerem (`hsh_string`/`*const c_char`) — potwierdzone zarówno w `core.c`, jak i w generatorze shimów |
| `int`      | `i64` (H#'owy `int` jest 64-bitowy)                              |
| `bool`     | `i64` (0/1) — zgodnie z tym, jak `core.c` reprezentuje dziś wyniki logiczne (`hsh_str_contains` itd. zwracają `int64_t`, nie `int8_t`) |
| `f32`/`f64`| natywny `f32`/`f64`                                              |

**Uczciwie o ryzyku**: ta biblioteka została napisana i przejrzana wobec
źródeł kompilatora H#, ale nie została skompilowana i uruchomiona w tym
środowisku (brak LLVM 21 + pełnego toolchainu H# w tej sesji). Zanim
wejdziesz z tym na produkcję:

1. Zbuduj mały testowy `.h#` z jedną deklaracją `extern static [rust]`
   (np. `fn hweb_console_log(msg: string)`), skompiluj z
   `--target wasm32 --emit object`, i porównaj oczekiwaną sygnaturę importu
   przez `wasm-objdump -x app.o` (lub `wasm2wat`) z tym, co deklaruje
   `runtime-rust/src/web_bridge.rs`. Jeśli się różnią (np. `bool` faktycznie
   ląduje jako `i32`, nie `i64`) — popraw tylko typ w Rust (nazwy i logika
   zostają takie same), zbuduj ponownie `cargo build --target
   wasm32-unknown-unknown`, i wpisz różnicę do `CHANGELOG.md`.
2. To samo dla `hsh_*` w `core_runtime.rs` — porównaj z symbolami
   nierozwiązanymi w `.o` (`wasm-objdump -x app.o | grep hsh_`).

Ten rozdział jest tu świadomie, a nie w README, właśnie dlatego, że
"ogromna stabilność API" oznacza też: mówimy wprost, gdzie leży
niepewność, zamiast obiecywać coś, czego nie mogliśmy tu zweryfikować
uruchomieniowo.

## Moduły

| Moduł            | Import                                              | Zakres |
|-------------------|-----------------------------------------------------|--------|
| `web_console`      | `use "web-wasm -> web_console" from "console"`      | `console.*` |
| `web_dom`          | `use "web-wasm -> web_dom" from "dom"`              | DOM: `Element`, tworzenie/usuwanie, atrybuty, klasy, style, `value` |
| `web_events`       | `use "web-wasm -> web_events" from "events"`        | `listen`/`unlisten`, stałe `KIND_*`, punkt wejścia `wasm_on_event` |
| `web_timers`       | `use "web-wasm -> web_timers" from "timers"`        | `setTimeout`/`setInterval`/`requestAnimationFrame` |
| `web_storage`      | `use "web-wasm -> web_storage" from "storage"`      | `localStorage`/`sessionStorage` |
| `web_fetch`        | `use "web-wasm -> web_fetch" from "fetch"`          | `fetch()` (GET/POST), asynchronicznie przez `wasm_on_event` |
| `web_canvas`       | `use "web-wasm -> web_canvas" from "canvas"`        | Canvas 2D — prostokąty, ścieżki, tekst |
| `web_random`       | `use "web-wasm -> web_random" from "random"`        | `crypto.getRandomValues`-backed losowość |
| `web_time`         | `use "web-wasm -> web_time" from "time"`            | `Date.now()` / `performance.now()` (zamiennik `std -> time` na wasm32) |
| `web_types`        | `use "web-wasm -> web_types" from "types"`          | `Response` używany przez `web_fetch` |

Pełne sygnatury — patrz nagłówki poszczególnych plików `src/*.h#`, każdy
jest w pełni udokumentowany komentarzami i celowo krótki (jeden plik =
jedna odpowiedzialność), żeby dało się go przeczytać w całości w minutę.

## Model zdarzeń — dlaczego jeden punkt wejścia, nie domknięcia

H#'owy `extern static [rust]` ma dziś dobrze zdefiniowaną, stabilną
marshalling tylko dla typów płaskich (int/bool/float) i `string`/`bytes`
(patrz `ffi_rust_native.rs`: `RsTy::Opaque` dla czegokolwiek innego, w
tym typów funkcyjnych/domknięć, jest **odrzucane** przez generator
shimów — nie ma jeszcze stabilnego ABI dla przekazania domknięcia przez
tę granicę). Zamiast prowizorki opartej o niestabilny/niepotwierdzony
mechanizm, `web-wasm` celowo używa jednego, przewidywalnego wzorca:
zarejestruj coś (`events::listen`, `timers::set_timeout`, ...), odbierz
wynik w Twojej własnej `pub fn wasm_on_event(kind, id, num, text)`.
Jedna, stabilna granica ABI zamiast N niestabilnych.

## Stan aplikacji: H# nie ma mutowalnych zmiennych globalnych

Top-level `let`/`const` w H# jest zawsze **niezmienny** i liczony raz
przed `main()` (potwierdzone w AST kompilatora: `Item::ConstDef`, jedyny
top-level-binding, z komentarzem wprost mówiącym, że nie jest
przeliczany ponownie jak lokalny `let`). W praktyce oznacza to, że stanu
aplikacji między `main()` a `wasm_on_event()` **nie da się** trzymać w
zwykłej zmiennej modułu. Dwa poprawne miejsca na taki stan:

- **DOM** (atrybuty `data-*`, `value` pól formularzy) — stan nie musi
  przetrwać odświeżenia strony (patrz `examples/hello-page`).
- **`web_storage`** (`localStorage`/`sessionStorage`) — stan ma przetrwać
  odświeżenie/zamknięcie karty.

## Czego ta biblioteka NIE dostarcza (świadomie)

`fs::`, `proc::`, `env::`, `net_tcp`/`net_udp`, `sqlite`, `python`,
`crypto_rsa` (OpenSSL) — wszystko to, co w `std` zależy od POSIX/systemu
plików/procesów. Program H# kompilowany na `--target wasm32`, który tego
użyje, dostanie błąd linkowania (undefined symbol) — to zamierzone
zachowanie samego kompilatora (patrz jego `TargetTriple::wasm32()`/
`WasmIncompatible`), nie ograniczenie tej biblioteki. `web_random` i
`web_time` istnieją właśnie jako w pełni funkcjonalne, webowe zamienniki
dla tych dwóch akurat przypadków, gdzie użytkownik najczęściej i tak
sięgnąłby po `std`.
