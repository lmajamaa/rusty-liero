# Rusty Liero

This is a Rust port of OpenLiero, the open-source remake of Liero 1.33 by
Erik Lindroos (gliptic) and others, made so that it runs in the browser. The port keeps the game exactly as it was: the same seed gives the
same game, frame for frame, and replays recorded by OpenLiero play back the
same. `tools/hashrun` checks that against hashes from the original C++.
[How the port was done](docs/002-rust-port-results.md) describes the port, [the port plan](docs/001-rust-port.md) has its step-by-step plan and notes, and [online multiplayer](docs/003-multiplayer.md) is the plan for what comes next.

The game is only built for the web, with Docker (see below), from Rust. tctool, written
in Rust, is a native command-line tool.

## The game

Liero is a two-player action game from 1998, made for DOS by Joosa
Riekkinen. Two worms fight on a level of dirt and rock that they dig through
and blow apart, with 40 weapons, a ninja rope to swing around on, and bonus
boxes of health and weapons. Two players share one keyboard (or use
joysticks) on a split screen, or play against bots. The game modes are
Kill'em All, Game of Tag, Holdazone and Scales of Justice.

# Bots

Each player's menu (LEFT PLAYER or RIGHT PLAYER, then CONTROLLER) sets who
plays that worm: Human, CPU or AI. CPU and AI are different bots.

- **CPU** is the original Liero's bot. Each tick it heads for the nearest
  worm, turns to aim roughly at it and fires when it's in range of the
  current weapon. When it walks into a wall, it jumps or turns back. It also
  jumps, changes weapons and swings on the rope at random, with chances set by
  the TC. It doesn't look ahead or find paths, so it's cheap and easy to beat.
- **AI** is OpenLiero's predictive bot. It keeps a few plans, which are
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

- **AI FRAMES** (default 140, 2 seconds): how far ahead each plan is played out.
- **AI MUTATIONS** (default 2): how many changed plans it tries each tick.
- **AI PARALLELS** (default 3): how many plans it keeps.
- **AI TRACES** (default off): draws the positions the plans pass through.

The first player's AI also tries switching to weapons that are still loading
in the idle parts of its plans; the second player's doesn't.

**BOT WEAPONS** in the same menu applies to both bots:

- **RANDOM**: bots get random weapons and skip the weapon screen.
- **PICK** (the default): you pick a bot's weapons on the weapon screen, with
  that player's keys, and choose DONE for it.
- **KEEP**: bots keep the weapons set in their player menu and skip the weapon
  screen.

# How to build the native tools (Linux and Mac)

tctool needs only Rust:

```bash
cargo build --release --manifest-path tools/tctool/Cargo.toml
```

It is written to `tools/tctool/target/release/tctool`.

# How to build (Docker)

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

Shell with the web build tools (Rust, wasm-bindgen and Bun):

```bash
docker compose run --rm web
```

Web build of the game (writes `build/www/index.html`, `liero_game.js` and
`liero_game_bg.wasm`, with the TC inside the wasm):

```bash
docker compose run --rm web bun tools/web/build.ts
```

The game is `rust/liero-game`, built for `wasm32-unknown-unknown`; its shell
in the browser is `rust/liero-game/src/web.rs` and its page `web/index.html`.
Saves are kept in IndexedDB, and the first start copies those of the earlier
Emscripten build.

Serving the web build locally with Bun (then open http://localhost:8080):

```bash
bun web/serve.ts build/www
```

With Rust installed locally, `cargo run --release --manifest-path tools/tctool/Cargo.toml -- <path-to-tc>` works too.

The game's tests run natively:

```bash
docker compose run --rm web sh -c "cd rust/liero-game && cargo test"
```

CI also runs clippy on every crate and fails on any warning:

```bash
docker compose run --rm web sh -c "cd rust/liero-game && cargo clippy --all-targets -- -D warnings"
```

Ported code is tested against the original C++: the values the tests expect
were printed by running the same loops through the C++ code
(`rust/liero-game/reference/reference_values.cpp`, removed in phase 4 once
nothing could build it; it is in git history).

Whole games are checked too: the hash run (`rust/liero-game/src/bin/liero-hashrun.rs`)
plays fixed-seed bot games, replays, the menus and a scripted run of the whole
app without a window and prints hashes of the game state and of the drawn
frames, and `tools/hashrun/run.ts` builds it, runs it and compares them with
`tools/hashrun/expected.txt`, made from the original C++:

```bash
docker compose run --rm web bun tools/hashrun/run.ts
```

`--wasm` runs it on the web build instead, with Bun: the same wasm, with its
64 KB stack and its files in memory. `--tree` keeps the game's files in
memory natively too, as the web build does (`rust/liero-game/src/disk.rs`),
instead of a folder on disk. For a quick check of one part, such as a planted
bug, `--only=` runs just those sections (`tc`, `games`, `menus`, `settings`,
`replays`, `ai`, `input`, `app`, `levels`, `randomtc`); a full run is still the
check for a commit.

```bash
docker compose run --rm web bun tools/hashrun/run.ts --only=app,menus
```

CI runs these tests and checks and builds on every pull request
(`.github/workflows/test.yml`).

# Extracting game data for total conversions

For copyright reasons, this repository does not contain the original Liero sound
files. Included instead is the original ruleset together with the lierolibre
sound effects.

In order to use the original data, or any total conversion, you need to run
the tctool on the game data. Example on how to do this is included below:

## Linux/Mac

```bash
curl https://www.liero.be/download/liero-1.36-bundle.zip -O
unzip liero-1.36-bundle.zip
cargo run --release --manifest-path tools/tctool/Cargo.toml -- --tc-name "Liero v1.33" liero-1.36-bundle
rm liero-1.36-bundle.zip
rm -rf liero-1.36-bundle
```

# License

The code is licensed under the BSD 2-Clause License, see [LICENSE](LICENSE).
It uses the same license and copyright holders as
[openliero/openliero](https://github.com/openliero/openliero), the C++
OpenLiero this is ported from.

Some bundled parts keep their own licenses:

- `TC/openliero/sounds`: WTFPL, see `TC/openliero/sounds/sounds.txt`
- `rust/liero-game/src/exp.rs`: ported from musl's `exp`, MIT, Copyright (c)
  2018 Arm Limited (see the file)

The game also compiles in Rust crates, under licenses of their choosing:
`miniz_oxide` (MIT, Zlib or Apache-2.0) and `adler2` (0BSD, MIT or
Apache-2.0), and in the web build `wasm-bindgen`, `js-sys` and `web-sys`
(MIT or Apache-2.0).

The original Liero game data is not included. Convert your own copy with tctool.
