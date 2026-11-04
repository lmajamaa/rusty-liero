The game is only built for the web, with Docker (see below). The C++ tctool
and the optional videotool are native command-line tools.

How to build the native tools (Linux and Mac)
=============================================

* Make sure you have CMake, SDL2, SDL2_image and gcc installed
* Run cmake:
* $ cmake -DCMAKE_BUILD_TYPE=Release -G "Unix Makefiles"
* Run "make" to build tctool

(Optional) Enabling and building the video tool
-------------------------------
* Download latest libx264: git clone git://git.videolan.org/x264.git
* Build it: cd x264; ./configure --enable-shared --enable-pic && make -j8
* Download latest ffmpeg: git clone git://source.ffmpeg.org/ffmpeg.git ffmpeg
* Build it: cd ffmpeg; ./configure --enable-shared --enable-pic --enable-gpl --enable-libx264 --disable-programs --extra-ldflags=-L../x264 --extra-cflags=-I../x264 --extra-libs=-ldl && make -j8
* Run: make -j8 videotool

How to build (Docker)
=====================

With Docker installed, no other build tools are needed. The repository is mounted
into the containers and build output ends up in `build/`.

Native Linux build of the C++ tctool:
```bash
docker compose run --rm native
```

Running tctool from that build (writes to `TC/` in the repository):
```bash
docker compose run --rm native build/native/tctool --tc-name "Liero v1.33" liero-1.36-bundle
```

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

Building the Rust version of tctool (in `tools/tctool`):
```bash
docker compose run --rm native cargo build --release --manifest-path tools/tctool/Cargo.toml
```
```bash
docker compose run --rm native tools/tctool/target/release/tctool --tc-name "Liero v1.33" liero-1.36-bundle
```
With Rust installed locally, `cargo run --release --manifest-path tools/tctool/Cargo.toml -- <path-to-tc>` works too.

Game code ported to Rust lives in `rust/liero-game`. The web build compiles it
with cargo for `wasm32-unknown-emscripten` and links it into openliero. After
changing its `extern "C"` functions, regenerate the C header:
```bash
docker compose run --rm web sh -c "cd rust/liero-game && cbindgen --config cbindgen.toml --output include/liero_game.h"
```
Its tests:
```bash
docker compose run --rm native cargo test --manifest-path rust/liero-game/Cargo.toml
```

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
./tctool liero-1.36-bundle
mv TC/liero-1.36-bundle TC/"Liero v1.33"
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
* `src/gvl/zlib/miniz.c`: public domain, see the unlicense statement at the end of the file
* `src/gvl/support/type_info.hpp`: Boost Software License 1.0

The original Liero game data is not included. Convert your own copy with tctool.
