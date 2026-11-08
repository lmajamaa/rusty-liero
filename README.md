The game is only built for the web, with Docker (see below). tctool, written
in Rust, and the optional videotool are native command-line tools.

Bots
====

Each player's menu (LEFT PLAYER or RIGHT PLAYER, then CONTROLLER) sets who
plays that worm: Human, CPU or AI. CPU and AI are different bots.

* **CPU** is the original Liero's bot. Each tick it heads for the nearest
  worm, turns to aim roughly at it and fires when it's in range of the
  current weapon. When it walks into a wall, it jumps or turns back. It also
  jumps, changes weapons and swings on the rope at random, with chances set by
  the TC. It doesn't look ahead or find paths, so it's cheap and easy to beat.
* **AI** is OpenLiero's predictive bot. It keeps a few plans, which are
  sequences of key presses, and plays each one out on copies of the game.
  Each tick it tries changed versions of the plans and keeps whichever scores
  best, then presses the first keys of the best plan. Plans are scored on
  health, ammo, the distance to the other worm along a path through the level
  (around rock, and through dirt at double cost), aiming along that path,
  steering missiles and the game mode's goals. On the copies, it models the
  other worm as a simple bot that aims and shoots. It plays much better than
  CPU, but costs a lot more: every tick it plays out about AI FRAMES x (AI
  MUTATIONS + 1) frames of the game per bot.

The AI's options are in OPTIONS (F2):

* **AI FRAMES** (default 140, 2 seconds): how far ahead each plan is played out.
* **AI MUTATIONS** (default 2): how many changed plans it tries each tick.
* **AI PARALLELS** (default 3): how many plans it keeps.
* **AI TRACES** (default off): draws the positions the plans pass through.

The first player's AI also tries switching to weapons that are still loading
in the idle parts of its plans; the second player's doesn't.

**BOT WEAPONS** in the same menu applies to both bots:

* **RANDOM**: bots get random weapons and skip the weapon screen.
* **PICK** (the default): you pick a bot's weapons on the weapon screen, with
  that player's keys, and choose DONE for it.
* **KEEP**: bots keep the weapons set in their player menu and skip the weapon
  screen.

How to build the native tools (Linux and Mac)
=============================================

tctool needs only Rust:
```bash
cargo build --release --manifest-path tools/tctool/Cargo.toml
```
It is written to `tools/tctool/target/release/tctool`.

(Optional) Enabling and building the video tool
-------------------------------
* Make sure you have CMake, SDL2, SDL2_image and gcc installed
* Run cmake: cmake -DCMAKE_BUILD_TYPE=Release -G "Unix Makefiles"
* Download latest libx264: git clone git://git.videolan.org/x264.git
* Build it: cd x264; ./configure --enable-shared --enable-pic && make -j8
* Download latest ffmpeg: git clone git://source.ffmpeg.org/ffmpeg.git ffmpeg
* Build it: cd ffmpeg; ./configure --enable-shared --enable-pic --enable-gpl --enable-libx264 --disable-programs --extra-ldflags=-L../x264 --extra-cflags=-I../x264 --extra-libs=-ldl && make -j8
* Run: make -j8 videotool

How to build (Docker)
=====================

With Docker installed, no other build tools are needed. The repository is mounted
into the containers and build output ends up in `build/`.

Native Linux build of tctool (writes `tools/tctool/target/release/tctool`):
```bash
docker compose run --rm native
```

Running tctool from that build (writes to `TC/` in the repository):
```bash
docker compose run --rm native tools/tctool/target/release/tctool --tc-name "Liero v1.33" liero-1.36-bundle
```

tctool's tests convert a generated installation and compare the result with
`tools/tctool/tests/synthetic.sha256`. With `LIERO_133_DIR` set to a folder with
Liero 1.33's `LIERO.EXE`, `LIERO.CHR` and `LIERO.SND`, they also check its
conversion against `tools/tctool/tests/liero133.sha256`:
```bash
docker compose run --rm native cargo test --manifest-path tools/tctool/Cargo.toml
```
After an intended change to the output, rewrite the listings with
`TCTOOL_UPDATE_LISTINGS=1` and say why in the commit.

Shell with the web build tools (Emscripten and Bun):
```bash
docker compose run --rm web
```

Web build of openliero (writes `build/web/openliero.html`, `.js`, `.wasm` and `.data`):
```bash
docker compose run --rm web sh -c "emcmake cmake -S . -B build/web -DCMAKE_BUILD_TYPE=Release && cmake --build build/web -j"
```

Serving the web build locally with Bun (then open http://localhost:8080):
```bash
bun web/serve.ts build/web
```

With Rust installed locally, `cargo run --release --manifest-path tools/tctool/Cargo.toml -- <path-to-tc>` works too.

Game code ported to Rust lives in `rust/liero-game`. The web build compiles it
with cargo for `wasm32-unknown-emscripten` and links it into openliero. After
changing its `extern "C"` functions, regenerate the C header:
```bash
docker compose run --rm web sh -c "cd rust/liero-game && cbindgen --config cbindgen.toml --output include/liero_game.h"
```
Its tests, natively and as wasm (run with Bun):
```bash
docker compose run --rm web sh -c "cd rust/liero-game && cargo test"
```
```bash
docker compose run --rm web sh -c "cd rust/liero-game && cargo test --target wasm32-unknown-emscripten"
```
Ported code is tested against the original C++: `rust/liero-game/reference/reference_values.cpp`
prints the values the tests expect, by running the same loops through the C++
code (see the comment at its top for how to run it).

Whole games are checked too: `openliero --hash-run` plays fixed-seed bot games
without a window and prints hashes of the game state and of the drawn frames,
and `tools/hashrun/run.ts`
compares them with `tools/hashrun/expected.txt`, made from the original C++.
After building the web game:
```bash
docker compose run --rm web bun tools/hashrun/run.ts
```
CI runs these tests and checks and builds on every pull request
(`.github/workflows/test.yml`).

Extracting game data for total conversions
======================

For copyright reasons, this repository does not contain the original Liero sound
files. Included instead is the original ruleset together with the lierolibre
sound effects.

In order to use the original data, or any total conversion, you need to run
the tctool on the game data. Example on how to do this is included below:

Linux/Mac
---------------------
```bash
curl https://www.liero.be/download/liero-1.36-bundle.zip -O
unzip liero-1.36-bundle.zip
cargo run --release --manifest-path tools/tctool/Cargo.toml -- --tc-name "Liero v1.33" liero-1.36-bundle
rm liero-1.36-bundle.zip
rm -rf liero-1.36-bundle
```

License
=======

The code is licensed under the BSD 2-Clause License, see [LICENSE](LICENSE).
It uses the same license and copyright holders as
[openliero/openliero](https://github.com/openliero/openliero), the continuation
of this project.

Some bundled parts keep their own licenses:
* `TC/openliero/sounds`: WTFPL, see `TC/openliero/sounds/sounds.txt`
* `src/gvl/support/type_info.hpp`: Boost Software License 1.0

The game also compiles in two Rust crates, under licenses of their choosing:
`miniz_oxide` (MIT, Zlib or Apache-2.0) and `adler2` (0BSD, MIT or Apache-2.0).

The original Liero game data is not included. Convert your own copy with tctool.
