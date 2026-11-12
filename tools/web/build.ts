// Builds the web game without Emscripten: rust/liero-game for
// wasm32-unknown-unknown (its shell is src/web.rs), wasm-bindgen's JavaScript
// for it, and the page (web/index.html), into one folder to serve as is.
//
//   bun tools/web/build.ts               # from the repository's root
//   bun tools/web/build.ts --out=dir     # default build/www
//   bun tools/web/build.ts --hashrun     # with the hash run's export
//
// The game gets a 64 KB stack, as the Emscripten build had; the hash run
// (tools/hashrun/run.ts --wasm) runs this build, so it would find a game
// that needs more. Rust's wasm puts the stack first in memory, so running
// past it traps instead of overwriting data.
//
// The wasm-bindgen CLI must be the version of the wasm-bindgen crate, which
// Cargo.toml pins; Dockerfile.web and CI install it.

import { copyFileSync, mkdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";

const args = process.argv.slice(2);
const outArg = args.find((a) => a.startsWith("--out="));
const hashrun = args.includes("--hashrun");
const unknown = args.filter((a) => a !== outArg && a !== "--hashrun");
if (unknown.length) {
  console.error(`Unknown arguments: ${unknown.join(" ")}`);
  process.exit(1);
}
const out = outArg?.slice("--out=".length) ?? "build/www";
const crate = "rust/liero-game";
const target = "wasm32-unknown-unknown";

function run(cmd: string[]) {
  const r = Bun.spawnSync(cmd, { stdout: "inherit", stderr: "inherit" });
  if (r.exitCode !== 0) {
    console.error(`Failed: ${cmd.join(" ")}`);
    process.exit(1);
  }
}

const pinned = readFileSync(join(crate, "Cargo.toml"), "utf8").match(/^wasm-bindgen = "=([^"]+)"/m)?.[1];
const cli = Bun.spawnSync(["wasm-bindgen", "--version"], { stdout: "pipe" });
const installed = cli.stdout.toString().trim().split(" ")[1];
if (!pinned || installed !== pinned) {
  console.error(`wasm-bindgen ${installed ?? "isn't installed"}; Cargo.toml pins ${pinned}. Install that version (Dockerfile.web).`);
  process.exit(1);
}

// A cdylib only for this build, so native builds and tests don't make one.
run(["cargo", "rustc", "--quiet", "--release", "--lib", "--crate-type", "cdylib", "--target", target,
  "--manifest-path", join(crate, "Cargo.toml"), ...(hashrun ? ["--features", "hashrun-export"] : []), "--", "-C", "link-arg=-zstack-size=65536"]);

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const wasm = join(crate, "target", target, "release", "liero_game.wasm");
run(["wasm-bindgen", "--target", "web", "--no-typescript", "--out-dir", out, wasm]);
copyFileSync("web/index.html", join(out, "index.html"));

const size = (f: string) => (statSync(join(out, f)).size / 1e6).toFixed(2);
console.log(`Built ${out}: liero_game_bg.wasm ${size("liero_game_bg.wasm")} MB, liero_game.js ${size("liero_game.js")} MB`);
