# Online multiplayer

Players send each other inputs over WebRTC DataChannels and run the game with rollback netcode. A small Bun server (`Bun.serve` with WebSockets) only connects them. GitHub Pages only serves static files, so that server needs a separate host.

## Why rollback fits

- **The game is deterministic, and that's tested.** The game logic uses no floats, the random number generator is the game's own `Rand`, the hash run checks it against the original C++, and replays store a checksum every 15 seconds.
- **The game can already be copied and restored.** The predictive AI copies the whole game, plays it ahead and throws the copy away (`rust/liero-game/src/ai/sim.rs`), with no sounds, no stats, no viewports and `quick_sim` on. Rollback needs the same thing: a copy of `GameData`, the level's pixels and materials, and the worms. That's about 180 KB per frame, so a ring of around 10 copies at 71 Hz is cheap.
- **Inputs are tiny.** Each worm's controls change by fewer than 8 bits per frame (`write_frame` in `replay.rs`). Each packet can carry the last several frames of inputs, so a lost packet doesn't matter.
- **Network games come with replays.** The confirmed input stream is a replay, so saving a game and letting others watch need almost no extra code.

## What has to be solved

1. **Side effects during re-simulation.** The game step plays sounds, records stats and moves viewports (screen shake, death banners). On a rollback:
   - Viewports become part of the saved state.
   - Stats are recorded only for confirmed frames.
   - Sounds play only for frames that won't be re-simulated, or are skipped if they already played. Otherwise explosions play twice.

   This is the biggest piece of new work.
2. **Keeping the two machines in step.** The loop runs fixed 14 ms steps, and two browsers' clocks drift apart. Rollback needs:
   - An input delay of 1–3 frames.
   - A way to measure how far ahead each side is and slow the faster one down slightly.
   - An agreed pause, since `paused` is part of the game state.
3. **Setting up identical games.** Both sides have to start from exactly the same state: the same TC, settings, level (or the seed for a random one) and build version. Settings are locked during the game; replays allow them to change mid-game. The weapon selection screen also runs before each game and takes input, so it needs to be synced or moved into the lobby.
4. **AI worms.** The predictive AI runs many simulated games per frame. If it runs again on every re-simulated frame, the CPU cost explodes. Either leave AI out of online games, or have one side compute the AI's moves and send them as inputs.
5. **Detecting desyncs.** The replay checksum covers only worm positions and the random number generator, every 15 seconds. Online games send a hash of the full state every second or so along with the inputs, so a desync is caught at the frame where it starts.
6. **NAT traversal.** WebRTC needs STUN, and a TURN relay for players behind strict NATs, where direct connections often fail. TURN carries real traffic, so it needs real hosting. Decide whether to run one or accept that some pairs can't connect.
7. **The native build.** As written this is browser-only. Native ↔ web play would need a WebRTC library in Rust. Decide whether it's in scope.

## Build it or use a library

`ggrs` (rollback logic, including the input delay and keeping the machines in step) together with `matchbox` (WebRTC sockets for wasm and native, plus a small signaling server) covers items 2, 6 and 7. They're new dependencies, so they need a decision first. Writing it here is also reasonable, because copying and restoring the game already exists.

## Order

1. **Offline rollback test.** Run a game with fake input delays and forced rollbacks, and check its hashes match a straight run, as the hash run does. No network is involved. This step also solves item 1.
2. **Two tabs over `BroadcastChannel`.** Tests timing and setup without any server.
3. **WebRTC with the Bun signaling server, then TURN.**
