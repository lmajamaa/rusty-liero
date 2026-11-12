// Builds and runs the hash run (rust/liero-game/src/bin/liero-hashrun.rs)
// natively and compares the game-state and frame hashes with expected.txt,
// which was made from the original C++ game and drawing code.
//
//   bun tools/hashrun/run.ts             # from the repository's root
//   bun tools/hashrun/run.ts --update    # rewrite expected.txt
//   bun tools/hashrun/run.ts --only=app,menus
//   bun tools/hashrun/run.ts --wasm      # as wasm, with Bun
//   bun tools/hashrun/run.ts --tree      # with the files in memory
//
// --only runs just those sections (tc, games, menus, settings, replays, ai,
// input, app, levels, randomtc) and checks that each one's lines match a run
// of expected.txt's, for quick checks such as planted bugs. A full run is still
// the check for a commit.
//
// --wasm runs it on the web build instead (tools/web/build.ts --hashrun,
// into build/hashrun-wasm), with Bun: the game's wasm as it ships, with its
// 64 KB stack and its files in memory, plus the export that runs the hash
// run (tools/hashrun/wasm.ts). CI runs both.
//
// --tree keeps the game's files in a tree in memory instead of a folder on
// disk, as the web build does (rust/liero-game/src/disk.rs), to show the tree
// gives the same lines natively. The wasm run always does.
//
// Only regenerate expected.txt from a build whose game logic is known to be
// right, such as the commit before a port.

import { join } from "node:path";

const TICKS = 30000;
const INTERVAL = 100;

const args = process.argv.slice(2);
const update = args.includes("--update");
const wasm = args.includes("--wasm");
const tree = args.includes("--tree");
const only = args.find((a) => a.startsWith("--only="))?.slice("--only=".length) ?? "";
if (update && only) {
  console.error("--update needs a full run, without --only");
  process.exit(1);
}
const unknown = args.filter((a) => a !== "--update" && a !== "--wasm" && a !== "--tree" && !a.startsWith("--only="));
if (unknown.length) {
  console.error(`Unknown arguments: ${unknown.join(" ")}`);
  process.exit(1);
}
const expectedPath = join(import.meta.dir, "expected.txt");

const isHash = (l: string) => /^\S+ \d+ [0-9a-f]{16}( end| frame)?$/.test(l);

const runArgs = [...(tree ? ["--tree"] : []), String(TICKS), String(INTERVAL), ...(only ? [only] : [])];
const cargo = ["cargo", "build", "--quiet", "--release", "--manifest-path", "rust/liero-game/Cargo.toml", "--bin", "liero-hashrun"];
let run;
if (wasm) {
  const out = "build/hashrun-wasm";
  const build = Bun.spawnSync([process.execPath, "tools/web/build.ts", "--hashrun", `--out=${out}`], { stdout: "inherit", stderr: "inherit" });
  if (build.exitCode !== 0) process.exit(1);
  const wasmArgs = [String(TICKS), String(INTERVAL), only];
  run = Bun.spawnSync([process.execPath, join(import.meta.dir, "wasm.ts"), out, ...wasmArgs], { stdout: "pipe", stderr: "pipe" });
} else {
  const build = Bun.spawnSync(cargo, { stdout: "inherit", stderr: "inherit" });
  if (build.exitCode !== 0) process.exit(1);
  run = Bun.spawnSync(["rust/liero-game/target/release/liero-hashrun", ...runArgs], { stdout: "pipe", stderr: "pipe" });
}
if (run.exitCode !== 0) {
  // How each game and replay ended, then the error.
  console.error(run.stderr.toString().trim());
  console.error(`The hash run exited with ${run.exitCode}`);
  process.exit(1);
}
const printed = run.stdout.toString().split("\n");
for (const l of printed) if (l.includes(" summary: ")) console.log(l);

// The hash lines, and those of each section run.
const output = printed.filter((l) => isHash(l) || l.startsWith("hash-section "));

const lines = output.filter(isHash);
const sections: string[][] = [];
for (const l of output) {
  if (l.startsWith("hash-section ")) sections.push([]);
  else sections[sections.length - 1]?.push(l);
}

if (lines.length === 0) {
  console.error("No hashes printed");
  process.exit(1);
}

if (update) {
  await Bun.write(expectedPath, lines.join("\n") + "\n");
  console.log(`Wrote ${lines.length} hashes to ${expectedPath}`);
  process.exit(0);
}

const all = (await Bun.file(expectedPath).text()).trim().split("\n");
let expected = all;
if (only) {
  // Each section's lines are a run of expected.txt, starting where its first
  // line is.
  expected = [];
  for (const section of sections) {
    if (section.length === 0) continue;
    const start = all.indexOf(section[0]);
    if (start < 0) {
      console.error(`A section's first line isn't in expected.txt: ${section[0]}`);
      console.error("The game no longer matches the original C++.");
      process.exit(1);
    }
    expected.push(...all.slice(start, start + section.length));
  }
}
const count = Math.max(expected.length, lines.length);
for (let i = 0; i < count; ++i) {
  if (expected[i] !== lines[i]) {
    console.error(`First difference at line ${i + 1}:`);
    console.error(`  expected: ${expected[i] ?? "(nothing)"}`);
    console.error(`  actual:   ${lines[i] ?? "(nothing)"}`);
    console.error("The game no longer matches the original C++.");
    process.exit(1);
  }
}
console.log(`All ${lines.length} game-state and frame hashes match${only ? ` (only ${only})` : ""}${wasm ? " (wasm)" : ""}${tree ? " (files in memory)" : ""}`);
