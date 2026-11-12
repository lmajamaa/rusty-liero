// Runs the hash run on a web build (tools/web/build.ts's output) with Bun,
// printing what the native liero-hashrun prints. tools/hashrun/run.ts --wasm
// calls it:
//
//   bun tools/hashrun/wasm.ts <build folder> <ticks> <interval> [sections]

import { join, resolve } from "node:path";

const [dir, ticks, interval, only = ""] = process.argv.slice(2);
if (!dir || !ticks || !interval) {
  console.error("usage: wasm.ts <build folder> <ticks> <interval> [sections]");
  process.exit(1);
}
const game = await import(resolve(dir, "liero_game.js"));
await game.default({ module_or_path: await Bun.file(join(dir, "liero_game_bg.wasm")).arrayBuffer() });

const lines: string[] = [];
let failed = false;
try {
  game.liero_web_hash_run(Number(ticks), Number(interval), only, (line: string) => lines.push(line));
} catch (e) {
  // An error the run returns, or a trap such as a stack overflow; a panic's
  // message is printed by the game first.
  console.error(String(e));
  failed = true;
}
await Bun.write(Bun.stdout, lines.map((l) => l + "\n").join(""));
process.exit(failed ? 1 : 0);
