// Runs the web build's headless hash mode (`--hash-run`, see
// src/game/hashrun.cpp) with Bun and compares the game-state hashes with
// expected.txt, which was made from the original C++ game logic.
//
//   bun tools/hashrun/run.ts [dir]            # dir defaults to build/web
//   bun tools/hashrun/run.ts [dir] --update   # rewrite expected.txt
//
// Only regenerate expected.txt from a build whose game logic is known to be
// right, such as the commit before a port.

import { resolve, join } from "node:path";

const TICKS = 30000;
const INTERVAL = 100;

const args = process.argv.slice(2);
const update = args.includes("--update");
const dir = resolve(args.find((a) => !a.startsWith("--")) ?? "build/web");
const expectedPath = join(import.meta.dir, "expected.txt");

const lines: string[] = await new Promise(async (done, fail) => {
  const out: string[] = [];
  // Emscripten's generated script uses a global Module object if one exists.
  (globalThis as any).Module = {
    locateFile: (path: string) => join(dir, path),
    print: (line: string) => out.push(line),
    printErr: (line: string) => console.error(line),
    onRuntimeInitialized() {
      try {
        const status = (globalThis as any).Module.callMain(["--hash-run", String(TICKS), String(INTERVAL)]);
        if (status) fail(new Error(`--hash-run exited with ${status}`));
        // The first line is the game's version banner on normal runs; hash
        // lines have three or four fields.
        else {
          for (const l of out) if (l.includes(" summary: ")) console.log(l);
          done(out.filter((l) => /^\S+ \d+ [0-9a-f]{16}( end)?$/.test(l)));
        }
      } catch (e) {
        fail(e);
      }
    },
  };
  // Emscripten's script starts with `var Module = typeof Module != "undefined"
  // ? Module : {}`. Under require() that `var` is local and hides the global,
  // so run the script in global scope instead, with the Node globals it uses.
  const script = join(dir, "openliero.js");
  Object.assign(globalThis, { require, __dirname: dir, __filename: script });
  (0, eval)(await Bun.file(script).text());
});

if (lines.length === 0) {
  console.error("No hashes printed");
  process.exit(1);
}

if (update) {
  await Bun.write(expectedPath, lines.join("\n") + "\n");
  console.log(`Wrote ${lines.length} hashes to ${expectedPath}`);
  process.exit(0);
}

const expected = (await Bun.file(expectedPath).text()).trim().split("\n");
const count = Math.max(expected.length, lines.length);
for (let i = 0; i < count; ++i) {
  if (expected[i] !== lines[i]) {
    console.error(`First difference at line ${i + 1}:`);
    console.error(`  expected: ${expected[i] ?? "(nothing)"}`);
    console.error(`  actual:   ${lines[i] ?? "(nothing)"}`);
    console.error("The game logic no longer matches the original C++.");
    process.exit(1);
  }
}
console.log(`All ${lines.length} game-state hashes match`);
