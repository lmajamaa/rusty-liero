# OpenLiero Web Port Plan

Last updated 2026-11-03.

## Goal and strategy

The goal is OpenLiero running in the browser, with tctool still available as a command-line converter. We get there in three phases, and each phase ends with a working build:

1. **Emscripten build of the existing C++ code.** No rewrite, just build changes and a few `#ifdef __EMSCRIPTEN__` blocks. This build becomes the reference that later phases are checked against.
2. **Gradual port to Rust inside the same Emscripten build.** Game logic moves first, one module at a time, with replays used to confirm nothing changed.
3. **Pure Rust on `wasm32-unknown-unknown`.** Once no C++ is left, Emscripten is dropped.

tctool runs alongside all three phases as a Rust command-line tool in `tools/tctool`. Later its file reader also becomes the importer inside the browser game.

## Current architecture

The codebase is about 70,000 lines of C++11 and C. SDL2 and SDL2\_image are its only system libraries, and zlib/miniz is bundled. The main loop, the threads and file access are what need work; graphics and game logic port as they are.

| Area | Current code | In the browser |
| --- | --- | --- |
| Rendering | 8-bit software renderer, presented through `SDL_Renderer` (`Gfx::draw`, `gfx.cpp`) | Works with Emscripten's SDL2 |
| Main loop | Blocking `do { process(); flip(); } while(true)` loops in every menu and in the game; `flip()` waits with `SDL_Delay` | **Main blocker**: the page can't block |
| Input | `SDL_PollEvent` / `SDL_WaitEvent`, scancodes | Works; some keys are taken by the browser |
| Audio | `SDL_OpenAudio` with a callback mixer (`sfx.cpp`) | Works after a user click or keypress |
| AI | `WorkQueue` with `SDL_CreateThread` and mutexes (`ai/work_queue.hpp`), but only under `AI_THREADS`, which is off | Already single-threaded |
| Files | `opendir` / `FindFirstFile` in `filesystem.cpp`; settings and replays saved to disk | Needs a virtual file system and persistent storage |
| Data | `TC/openliero`: about 100 `.cfg`, `.tga` and `.wav` files | Easy to bundle |
| tctool | 1,200 lines in `src/tc_tool`; reads `LIERO.EXE`, `LIERO.CHR` and `LIERO.SND`, writes a TC folder | Command-line file tool, no SDL window |

## Phase 1: Emscripten build of the C++ code

Phase 1 is done when local 2-player games, menus, sound and saved settings all work in the browser, built from the same sources as the desktop version.

**Toolchain.** The repo's `docker-compose.yml` provides both toolchains in Docker, so nothing else needs installing: `native` (Alpine 3.22, CMake, SDL2, SDL2\_image, Cargo) and `web` (Debian trixie with emsdk 4.0.0 and Bun 1.4.4; emsdk's prebuilt tools need glibc, so not Alpine).

- [x] **Build target.** An `if(EMSCRIPTEN)` branch in `CMakeLists.txt` links Emscripten's SDL2 ports (`-sUSE_SDL=2 -sUSE_SDL_IMAGE=2 -sSDL2_IMAGE_FORMATS=tga,png`) and builds only `openliero`, as `build/web/openliero.html`, `.js` and `.wasm`. `gvl` and `tl` now recognise wasm32 as a little-endian architecture. The build links but doesn't run yet: it needs the main loop and game data steps below.
- [x] **Main loop.** Linked with `-sASYNCIFY -sASYNCIFY_STACK_SIZE=65536` (the 4 KB default crashed at the first sleep). `Gfx::flip()` calls `emscripten_sleep` every frame, even when behind, because the browser only shows a frame once control returns to it. `SDL_WaitEvent` already yields, since SDL's `SDL_Delay` calls `emscripten_sleep` under ASYNCIFY. Menus, weapon select and 2-player games run. The build also needs `-fexceptions`, because the game reports errors with C++ exceptions. ASYNCIFY grows the `.wasm` from 1.3 MB to 1.9 MB.
- [x] **AI threads.** No change needed: `WorkQueue` is only used under `#if AI_THREADS`, which is never defined, so the predictive AI already runs on the main thread in every build. A 2-player game against a CPU bot holds 60 fps in the browser (median frame 16.7 ms, worst 32 ms after the start-up spike). GitHub Pages' missing COOP/COEP headers don't matter while it stays that way.
- [x] **Game data.** Bundled with `--preload-file TC/openliero@/TC/openliero` (600 KB `.data`). The POSIX branch of `filesystem.cpp` (now also chosen for `__EMSCRIPTEN__`) works on Emscripten's file system. The bundled sounds were renamed to lower case to match `tc.cfg`, which also fixes them on case-sensitive Linux. CMake doesn't track the data files, so delete `build/web/openliero.*` to repackage after changing them.
- [ ] **Saving.** Mount IDBFS on the folder for settings, profiles and replays. Load it with `FS.syncfs(true)` at startup and save with `FS.syncfs(false)` after each write.
- [ ] **Custom TCs and levels.** Add a file upload on the page that writes files into the virtual file system. The original Liero data is not hosted.
- [x] **Browser details.** Add a click-to-start screen so audio can start, call `preventDefault` on the game's keys, give player 2 default keys that exist on Mac keyboards (there is no Right Ctrl, and Ctrl+arrows switch Spaces), make the canvas fill the window, and turn off the spectator window.
- [x] **Hosting.** Publish a static page (`index.html`, `.js`, `.wasm`, `.data`) on GitHub Pages, built and deployed by a GitHub Actions workflow on every published release: <https://lmajamaa.github.io/rusty-liero/>. Bun is the web runtime: `web/serve.ts` (Bun.serve) serves the build locally with the right MIME types and no COOP/COEP headers, matching what Pages serves.

**Later, if needed:** replace ASYNCIFY with `emscripten_set_main_loop` by turning each menu and the game into a state with a `tick()` function. That gives a smaller, faster binary.

## tctool as a command-line utility

tctool is rewritten in Rust as a standalone command-line tool (`tools/tctool`, crate `liero-tctool`, binary `tctool`). It has no dependencies, takes the same arguments as the C++ tool and is meant to write byte-identical output. The C++ tctool stays for now so the two can be compared.

| Form | How it's built | How it's run |
| --- | --- | --- |
| Rust CLI | `cargo build --release` in `tools/tctool`, locally or in the `native` Docker image | `tctool [--tc-name NAME] [--config-root DIR] <liero-dir>` |
| C++ CLI (reference) | Existing CMake `tctool` target | Same arguments |
| Browser importer (later) | The Rust reader compiled to wasm with the game | Drop `LIERO.EXE`, `LIERO.CHR` and `LIERO.SND` on the page |

Remaining steps:

- [x] **Compile and fix.** Builds cleanly in the `native` image (no fixes needed).
- [ ] **Compare outputs.** Convert the 1.36 bundle with both tools and diff the TC folders; they should be identical.
- [ ] **Remove the C++ tctool** once the outputs match.
- [ ] **Test in CI.** Run the conversion and compare against a checked-in file listing.

## Graphics in the browser

Rendering needs almost no changes. The game draws each frame on the CPU, and SDL only uploads the finished frame and shows it, which Emscripten's SDL2 does with WebGL.

| Step | Current code | In the browser |
| --- | --- | --- |
| 1. Draw | Game writes into an 8-bit palette bitmap, 320×200 or 640×400 | Unchanged, runs as WebAssembly |
| 2. Convert | `scaleDraw` turns it into 32-bit ARGB using the palette, scaled by a whole number | Unchanged, runs as WebAssembly |
| 3. Upload | `SDL_UpdateTexture` | Uploads into a WebGL texture (`texSubImage2D`) |
| 4. Show | `SDL_RenderCopy`, `SDL_RenderPresent` | One textured rectangle drawn on the `<canvas>`; the browser shows it when control returns |

Changes needed:

- **Spectator window:** the browser has one canvas, so turn it off in the web build.
- **Scaling filter:** `SDL_HINT_RENDER_SCALE_QUALITY` is `"linear"`, which blurs pixel art. Use `"nearest"`, or CSS `image-rendering: pixelated`.
- **Canvas size:** resize the canvas to the window; `fitScreen` and `SDL_RenderSetLogicalSize` handle the rest.
- **Frame timing:** the game ticks every 14 ms (about 70 Hz) and most screens refresh at 60 Hz. Use a fixed-step loop that runs as many 14 ms ticks as real time requires and draws once per refresh. This also keeps game speed exact for replays.

Optional: move the palette conversion into a WebGL shader. It isn't needed for performance.

## Phase 2: gradual Rust port inside the Emscripten build

Rust code is built as a static library for `wasm32-unknown-emscripten`, and `emcc` links it with the remaining C++ into one `.wasm`. Every step is checked against replays, so the game stays playable throughout.

**How the two sides connect**

- Plain C functions (`extern "C"`, `#[no_mangle]`) with simple types and `#[repr(C)]` structs. `cbindgen` generates the C header.
- C++ holds an opaque handle to state owned by Rust, or whole subsystems move across at once. Rust can't use C++ classes like `Game` or `Worm` directly.
- Rust's `std::fs` works on Emscripten's file system, so the preloaded TC data and the IDBFS saves are readable from Rust too.
- Build with `panic = "abort"` and never let a panic cross into C++.
- Keep the `rustc` and `emcc` LLVM versions close to avoid link errors.
- ASYNCIFY runs after linking, so it covers Rust functions too.

**The bridge** (in place):

- [x] `rust/liero-game` is a `staticlib` crate (`liero_game`). CMake runs `cargo build --release --target wasm32-unknown-emscripten` for it on every web build, into `build/web/rust`, and links it into `openliero`.
- [x] The C header `rust/liero-game/include/liero_game.h` is generated by cbindgen and checked in.
- [x] Pinned versions: Rust 1.87.0 (LLVM 20.1) in the `web` image, matching emsdk 4.0.0's clang 20. Alpine 3.22's `cargo` is also 1.87.0. Change Rust and emsdk together.
- [x] Smoke test: `liero_game_version()` is called from `main.cpp`, which prints `liero-game 0.1.0` at startup in the browser.
- [x] Decided: the game is only built for wasm. The native CMake build now makes only the command-line tools (`tctool`, and `videotool` on request). Files only the desktop game used are removed: the macOS bundle and Windows icon files, `pkg/`, `CMakeSettings.json`, and about 200 sources and headers no target builds, mostly unused parts of `src/tl` and the original zlib (`miniz.c` replaces it).
- [x] CI (`.github/workflows/test.yml`, on pushes to main and on pull requests) runs the Rust tests natively and as wasm, checks the header against cbindgen's output, builds the game, and builds both tctools.

**Porting order**, starting with code that depends on nothing else:

1. [x] `math`, `rand`, fixed-point helpers: in `rust/liero-game/src/{rand,math}.rs`. C++'s `Rand` is now a thin wrapper that inherits the Rust state struct (`LieroRand`, the `gvl::mwc` multiply-with-carry generator, not CMWC), so games still copy and archive it as a plain value. `vectorLength` and the cos/sin table call Rust. Tests compare long output streams with hashes printed by `rust/liero-game/reference/reference_values.cpp`, which runs the original C++ code through em++. They pass natively and as wasm. Two findings: the cos/sin table relies on signed 64-bit overflow wrapping around, and native g++ on arm64 only matches wasm with `-ffp-contract=off` (no fused multiply-add). The C++ tctool keeps using `gvl::mwc`, and its build drops unused functions so it links without the Rust library.
2. `level` and materials
3. Object physics: `nobject`, `sobject`, `bobject`, `bonus`, `ninjarope`
4. `weapon`, `worm`, then the game tick in `game`
5. tctool's reader and writer (now in `tools/tctool`), as the `liero-data` crate shared by the game and `liero-tctool`
6. Last: `gfx`, menus, `sfx` and input

**Checking with replays.** Run each recorded replay through both the C++ and the Rust tick and compare a hash of the game state every frame. The first frame that differs points at the bug. Keep a set of replays in the repo that covers every weapon.

## Phase 3: pure Rust without Emscripten

Once no C++ is left, the web build moves to `wasm32-unknown-unknown` with `wasm-bindgen` and `trunk`, and native builds use the same code.

| Area | Web build | Native build |
| --- | --- | --- |
| Graphics | `pixels` crate (framebuffer on `wgpu` / WebGL2) | `winit` + `pixels` |
| Main loop | `tick()` called from `requestAnimationFrame`, fixed 14 ms steps | Same `tick()`, driven by `winit` |
| Files | TC data embedded with `include_bytes!` or fetched; saves in IndexedDB | `std::fs` |
| Audio | Web Audio through `cpal` or `kira` | `cpal` or `kira` |
| tctool | Browser importer calls `liero-data` | `liero-tctool` CLI calls `liero-data` |

## Later: online multiplayer

The game logic is deterministic and replays already record only inputs, which makes it a good fit for rollback netcode. Players send each other inputs over WebRTC DataChannels, and a small Bun server (Bun.serve with WebSockets) only connects them. GitHub Pages only serves static files, so that server needs a separate host. The old unused `src/tl/socket.c` has been removed; it couldn't run in a browser anyway.

## Risks and open questions

| Risk | Effect | Fallback |
| --- | --- | --- |
| ASYNCIFY makes the binary too big or slow | Slow loading or dropped frames | Move to `emscripten_set_main_loop` with per-state `tick()` functions |
| AI too slow on one thread | Bots slow down the game | `-pthread`, with coi-serviceworker adding the COOP/COEP headers GitHub Pages lacks |
| Rust port changes game behaviour | Old replays and TCs stop matching | Per-frame state hash compared against the C++ build |
| `rustc` and `emcc` LLVM versions drift apart | Link errors in phase 2 | Pin both versions in CI |
| Browser takes over keys the game uses | Some controls don't work | `preventDefault` on game keys; let players rebind |

Open questions:

- [x] Decided: the web build is hosted on GitHub Pages.
- [x] Decided: tctool is a Rust command-line tool, with no TypeScript or Emscripten version.
- [ ] Which replays make up the reference set for phase 2?
