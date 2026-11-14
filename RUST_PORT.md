# How the port was done

This repository took OpenLiero, a C++ game built on SDL, and turned it into a Rust game that runs in the browser. It took 60 merged pull requests, between 2026-11-03 and 2026-11-14. At the end, the two game crates have `#![forbid(unsafe_code)]`, no C++ is left, and every one of the 26,349 hashes recorded from the original C++ still matches.

This is how it was done, for anyone porting another C++ codebase to Rust. It's built from the merged pull requests (their descriptions and commits), read in order, and each point names the PRs it comes from by number. Nothing here is new. Apart from the pattern report a workflow posts, the PRs have no comments, so their descriptions and commit messages are the record.

[PLAN.md](PLAN.md) has the step-by-step detail, including every planted bug that got through and why.

## The shape of it

| Phase | What | PRs |
| --- | --- | --- |
| 1 | The C++ game builds with Emscripten and runs in the browser, unchanged | #3, #7, #8, #23 |
| 2 | Module by module (with the last of phase 1 and the tctool alongside), the game's code moves into a Rust library linked into the Emscripten build, behind a C boundary | #4–#25 |
| 3 | The main loop becomes a stack of screens, then everything left (data, screens, files, the hash run itself, the shell) moves to Rust, and Emscripten goes | #26–#49 |
| 4 | The C boundary and the `unsafe` are removed, then the code is made idiomatic | #51–#64 |

All the way through, there was a running game at every merge, and the same check: the hash run.

## Start from a build of the original that you can trust

**Get the original running on the new platform before porting anything.** Phase 1 built the unchanged C++ with Emscripten (#3), deployed it (#7), and fixed what the browser needed: a click to start audio, keys kept away from the page, Mac-friendly defaults (#8), saves in IndexedDB (#23). That build was then three things at once: the product, the host the Rust code was linked into, and the reference the Rust code was compared against.

Building somewhere new found real bugs before a line was ported. The game's sound files never loaded on a case-sensitive file system, because the loader upper-cased the whole path. That affected native Linux builds too (#3).

**Remove what nothing builds before you start.** #5 deleted 208 files that no target compiled or included. It found them by following every `#include` from the built files, counting those inside `#if` blocks too.

**Match the toolchains.** Rust 1.87 was chosen because it uses LLVM 20, the same major version as emsdk 4.0.0's clang, so their object files could link together (#4). The library was built with `panic = "abort"`, so a panic never unwinds into C++ (#4).

## Reference values from the original, by the same compiler

The first module ported was the random number generator and the fixed-point maths (#6). A small C++ program, `reference_values.cpp`, ran the same loops as the Rust tests through the original C++ code. It printed hashes of millions of outputs, and the Rust tests checked against them. Later ports added to it: levels (#9), `std::sort` (#20) and the stats screen's percentages (#20).

The reference was built with `em++` and run with Bun, the same compiler the game used. That mattered. Native g++ on arm64 gave different doubles unless it was told not to fuse multiply-adds (`-ffp-contract=off`), because wasm has no fused multiply-add (#6). The cos/sin table relied on a 64-bit sum wrapping around, which a Rust debug build turns into a panic, so the port wraps explicitly (#6).

Once nothing in the repository could build the reference any more, it was removed. The values it printed stay in the tests (#64).

## The hash run

Unit tests against reference values cover small functions. For everything else, the check was a headless run of the whole game that prints hashes of its state. Every PR had to match all of them.

**It started small.** #10 added `--hash-run`: eight bot-against-bot games from fixed seeds, with no window or sound, hashing the whole game state every 100 ticks. `tools/hashrun/run.ts` compared that with `expected.txt` and printed the first line that differed. The run took about a second, and CI ran it.

**It grew with each port, and the baseline for a new part was always recorded from the C++ first.** Each port that the existing hashes didn't cover began with a commit recording new hash lines from the original code, followed by the port itself:

| PR | Added to the hash run | Lines |
| --- | --- | --- |
| #10 | 8 bot games, whole game state | 2,408 |
| #11 | random weapons that actually were random (the bots had kept their default bazookas), and loadouts covering all 40 weapons | 4,153 |
| #12, #13 | game modes the earlier games never reached | 5,056 |
| #14 | the drawn frames, both views, every 100 ticks | 10,112 |
| #16 | menus and weapon selection driven by random keys | 12,768 |
| #18 | everything loaded from the TC | 12,880 |
| #19 | every settings format, including truncated and corrupted inputs | 13,713 |
| #20 | the stats recorder and the stats screen | 19,194 |
| #21 | replays recorded, played back and broken | 20,104 |
| #22 | games between the predictive bots, with each bot's whole state | 20,864 |
| #25 | the sound mixer and key handling | 21,314 |
| #26 | the whole app, driven by a fixed script of key presses through every loop | 25,906 |
| #38, #39, #40 | a replay rewind, one-letter names, menu paths that planted bugs had got past, level files | 26,349 |

**Record the whole app before changing its structure.** Before the nested loops were turned into screens, #26 ran the whole app with a scripted driver in place of the window. It hashed every frame shown, fed in key presses from a timeline fixed in advance, and controlled the clock. Every later change to the loops (#27–#33) matched those 4,592 frames without re-recording.

**Make the hash run independent of the machine.** A second commit in #26 made the app hashes independent of the machine and time zone.

**Make it fast to run part of it.** `run.ts --only=app,menus` (#42) checks only some sections, against their own lines. Checking a planted bug dropped from minutes to about 6 seconds. Full runs stayed the check for a commit.

**Port the check too, one section at a time.** The hash run itself was C++. #44 moved its ten sections to Rust one at a time, each checked on its own with `--only`, and #46 made it a native binary. In the end, CI runs the same hashes three ways: natively, with the files in memory (#48), and as the wasm the site ships (#49).

## Show that the check catches changes

**Plant bugs.** From #10 on, almost every port PR planted deliberate bugs at its risky spots, usually 5 to 10, and reported how many the hashes caught. The misses were as useful as the catches:

- **A miss can show a gap in the run's inputs.** In #16, shortening a 1.5 s search timeout went uncaught, because the scripted typing never left a gap of that length; a unit test covers it. In #36, four of eight plants were missed because the hash runs never used profiles, copied settings or edited names. #38 then added inputs that catch one of them (and a miss from #37), and replaced the code behind two others with an explicit `clone()`. #39 extended the app script wherever plants got through.
- **A miss can be unobservable.** In #25, `>> 1` instead of `/ 2` couldn't change anything, because the value was always even. #26 found the game loop's `++menuCycles` was dead code. PLAN.md lists each unobservable plant with its reason.
- **Some code is outside the hash run's reach, so it needs unit tests.** The browser's frame callback isn't run by the hash run, so #33 says it had no planted-bug check. Plants in the C++ host of #34 weren't caught by anything. The shell got 7 unit tests because the hash run doesn't reach it (#46).
- **Check that the plant was actually built.** #35 found that earlier plant runs could build stale code when a plant and its revert landed in the same second. Re-run with fresh timestamps, #34's hashes caught 3 of its 9 plants, not 2.

**Measure coverage.** In #10–#16, temporary counters on each ported branch showed which ones the hash games reached. For example, 52 of 59 in the worm port (#12). Every branch that wasn't reached got a reason (a TC hack the bundled TC turns off, an edge the bots never reach), and usually a unit test.

**Stop planting once it's refactoring.** Phase 4 changed no behaviour, so #52 and the PRs after it planted no bugs, and relied on the existing hashes matching after every commit.

## Keep the original's quirks, bit for bit

The goal was the same game, not a better one, so behaviour that a fix would change was kept and written down:

- **Unstable sort order.** `std::sort` isn't stable, and its tie order decides what the stats screen shows. `cpp_sort.rs` is a port of libc++ 19's introsort (#20). The bot's path finding ports gvl's pairing heap link for link, because which of two tied cells closes first decides where the bot aims (#22).
- **Parser quirks.** The TOML reader copies gvl's exactly, quirks included (#18). Settings archives keep their quirks, down to an old archive's dig control being read from one past the end of an array (#19).
- **Display quirks.** A timer's text was drawn with the wrong length because of an extra argument, and Rust draws that length (#15).
- **Floating point.** On `wasm32-unknown-unknown`, the AI's `exp` gave different last bits than musl or glibc did, which changed hashes from line 20,163. `exp.rs` is a port of musl's `exp`, so it gives the same bits on every target (#49).
- **Mixer quirks.** A stopped sound counts as playing until its channel is reused, and a volume of 2.0 is silent (#25).

Where the C++ read out of bounds or crashed, the port guards that case instead of copying it, and the PR says so. Examples are spawn selection on an impossible width (#9), replays naming a worm that doesn't exist (#21), bots with no plans (#22), and joystick buttons past the array (#25).

## Uninitialised memory shows up in the hashes

Hashing everything means hashing fields the C++ never set. That happened three times:

- #10 zeroed `Worm::reacts` and the rope lengths in the harness, because the constructor left them uninitialised.
- In #18, a health bonus never set `Bonus::weapon`, so the field held memory left behind by the C++ loader. The Rust loader left different memory, so the baseline couldn't match. The fix was to set it in C++ and re-record the baseline from `main` with only that change.
- In #29, moving the selectors to the heap changed what the unset `Worm::reacts` held, and from app frame 995 on the game played differently. It was found by zeroing every `operator new`, and fixed by zeroing those fields in the constructor and re-recording.

In each case, the fix went into the C++ and the baseline was re-recorded from it, before any Rust depended on it.

## Bugs the port found

Doing the port this way exposed bugs in the original and in the port itself:

- **Crashes in the original.** The C++ tctool crashed on a font character wider than 7 and on a reference past the last object (#24). An "AI" player crashed the web game: each copy of the 74 KB `Game` overflowed the 64 KB wasm stack, and the copies' worms were never freed (#22).
- **A leak.** `gvl::shared` had no copy constructor, so each rewind in the replay viewer leaked 1.8 MB (#38).
- **A hang.** A `liero.cfg` ending inside a string made the C++ reader loop until it ran out of memory (#19).
- **A port bug the hashes caught.** An implicit conversion built a temporary string that was then read after being freed. The hashes caught it at app frame 1887, and the conversion was made explicit (#36).
- **A port bug the hashes missed.** Setting a non-extended dig key crashed, because the port wrote to both control arrays where the C++ wrote to one (#45). No hash run sets that key.

## Crossing the C boundary

In phase 2, C++ owned all the data and called into Rust. Each port did it the same way:

- **Pointer views.** C++ passed views of its objects, sometimes structs of pointers to its fields.
- **Hook tables.** A table of C++ function pointers ("hooks") for anything Rust needed back from C++: making objects, playing sounds, recording stats (#10–#13).
- **Layout checks.** C++ structs used in place got `static_assert`s on their layout (#10).
- **A generated header.** The C header was generated by cbindgen and checked in CI (#4, #6).

Rust read and wrote fields through pointers at the same points the C++ did, because a hook could push or reuse the very object being processed (#10). Exceptions can't unwind through Rust, so a hook that threw stored the exception and C++ rethrew it once Rust returned (#19, #35).

In phase 3, ownership flipped. Rust owned the data, and the C++ structs became references into it, so the C++ code read and wrote fields as before. That went one type at a time: settings (#36), then the worms, game, viewports and controllers (#37), then the object lists and level (#38).

The cost of all this showed in phase 4. The first step of phase 4 made every unsafe operation explicit with `deny(unsafe_op_in_unsafe_fn)`, which raised the `unsafe` block count from 590 to 772 (#51). Then it deleted the boundary: 385 `#[no_mangle]`, 601 `extern "C"` functions and function pointers, and `repr(C)` on 122 types.

## The main loop

The original game was 14 nested loops that never returned to the browser. They slept through Emscripten's ASYNCIFY (#26). The loops were replaced in four steps, each against the #26 hashes:

1. A screen stack in Rust, with the whole old main loop as its single screen (#27).
2. Each loop turned into a screen, one group at a time: the main loop and main menu, the selectors, the boxes, then the game and stats screens (#28–#31).
3. The stack run from the browser's own frame loop, which removed ASYNCIFY and took the wasm from 2.74 MB to 1.88 MB (#33).
4. The screens moved to Rust (#34, #35, #39).

## The 64 KB stack

The web build's stack is 64 KB, and the game state is larger. That caused the AI crash (#22) and a bad indirect call when a temporary copy overflowed it (#37). #38 made CI's hash run build with `-sSTACK_OVERFLOW_CHECK=2`, so an overflow stops the run instead of silently corrupting data. When Emscripten went, the wasm hash run kept the same 64 KB stack and its checks (#46).

## Off Emscripten

Once no C++ was left, #49 moved the web build to `wasm32-unknown-unknown`:
- **The shell is Rust:** canvas drawing, keys mapped to the same SDL scancodes, gamepads, Web Audio and `requestAnimationFrame`.
- **The TC is built into the wasm**, and saves stay in IndexedDB, with the old build's saves copied on first start.
- **New crates, agreed first:** `wasm-bindgen`, `js-sys` and `web-sys`.

All hashes matched natively, with the files in memory, and on the build that ships. The only difference was the `exp` described above.

## Phase 4: safe and idiomatic

#43 wrote the plan before any of it started:
- **When it's done:** `forbid(unsafe_code)` holds, clippy is clean, and `expected.txt` is unchanged.
- **Twelve steps,** leaves first.
- **A rule:** the pattern counts only go down.

**Count what you want to remove, and make the count fail CI when it rises.** #17 added a workflow that posts a table of `unsafe`, raw pointers, `extern "C"`, `#[no_mangle]`, `repr(C)` and so on, comparing each pull request with its base. #51 added `--fail-on-rise`, so a count that goes up fails the check.

The steps, in the order they were done:

| Step | What | PRs |
| --- | --- | --- |
| 1 | Guardrails: clippy and rustfmt in CI, `forbid(unsafe_code)` on the data crates | #47, #51 |
| 2–4 | Delete the C boundary; leaf modules; views become borrows | #51 |
| 5 | Worms by id instead of pointer | #52 |
| 6 | The game tick by index, with no hook tables | #52, #53 |
| 7 | Ownership: borrows, `Rc<RefCell>` for what C++ shared | #55, #57 |
| 8 | Viewports, the scene, stats and replays | #58 |
| 9 | Screens as trait objects, the platform as a trait, no globals | #59 |
| 10 | One data model, shared with the data crate | #61 |
| 11 | Idioms: names, methods, enums, `Option`, fixed-point newtype | #62 |
| 12 | The last `unsafe`, then `forbid(unsafe_code)` | #64 |

Some things that came out of it:

- **Indexes instead of pointers.** Objects were walked through pointers to their slots, and a hook could change any object, including the one being processed. In #52 and #53, a step reads a copy of the slot, re-reads it after every call that can change the lists, and writes changes back at once, at the same points where the C++ re-read it. The hash run got about 2% slower (30.4 s to 30.9 s), probably from bounds checks (#52).
- **Shared ownership.** What C++ shared with a `shared_ptr` became `Rc<RefCell<…>>` (#55, #57). The tests found two double borrows the pointers had hidden (#57).
- **Slow parts measured.** Each PR that touched a hot loop measured the hash run's time. Most changes were within noise (#53, #55, #57, #58).
- **No platform crate needed.** The plan allowed for a small crate to hold any `unsafe` the browser glue needed. None was needed: the unsafe code `#[wasm_bindgen]` generates comes from another crate's macro, which the lint doesn't count (#64).
- **Miri not run.** Step 1 planned a Miri run to find code that was unsound rather than merely unsafe. It needed a nightly toolchain that was never agreed, and after step 12 no unsafe code of ours was left for it to check, so it was dropped (#64).
- **Kept on purpose.** `cpp_sort.rs`, the pairing heap, wrapping arithmetic and the object lists' slot reuse order stay as they are, because the hashes depend on them (#43).

Over phase 4, `unsafe` blocks went from 772 (once step 1 had made them all explicit) to 0, `unsafe fn` from 581 to 0, and raw pointer types from 773 to 11 (#51, #64). The 11 left are addresses that tests compare.
