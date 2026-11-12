// Counts unsafe Rust and other patterns that come from talking to C++ (raw
// pointers, extern "C", #[no_mangle], #[repr(C)], ...) in two checkouts, and
// prints a Markdown table comparing them.
//
//   bun tools/rust-patterns/report.ts <base-dir> <head-dir>
//   bun tools/rust-patterns/report.ts --fail-on-rise <base-dir> <head-dir>
//
// With --fail-on-rise it exits with 1 if any pattern's count is higher in
// head than in base: in phase 4 of PLAN.md the counts only go down.
//
// Comments and string literals are skipped, so "unsafe" in a doc comment or a
// message doesn't count. Files under target/ are ignored.

import { Glob } from "bun";

export const MARKER = "<!-- rust-patterns-report -->";

type Pattern = { name: string; re: RegExp };

const PATTERNS: Pattern[] = [
  { name: "`unsafe` blocks", re: /\bunsafe\s*\{/g },
  { name: "`unsafe fn`", re: /\bunsafe\s+(?:extern\s+"[^"]*"\s+)?fn\b/g },
  { name: "`unsafe impl`", re: /\bunsafe\s+impl\b/g },
  { name: "`static mut`", re: /\bstatic\s+mut\b/g },
  { name: "`transmute`", re: /\btransmute\b/g },
  { name: "Raw pointer types (`*const` / `*mut`)", re: /\*\s*(?:const|mut)\b/g },
  { name: "`extern \"C\"` functions and function pointers", re: /\bextern\s+"C"\s+fn\b/g },
  { name: "`extern \"C\"` blocks", re: /\bextern\s+"C"\s*\{/g },
  { name: "`#[no_mangle]`", re: /#\[\s*(?:unsafe\s*\(\s*)?no_mangle/g },
  { name: "`#[repr(C)]`", re: /#\[\s*repr\s*\(\s*C\b/g },
  { name: "`c_void`", re: /\bc_void\b/g },
  { name: "`from_raw_parts`", re: /\bfrom_raw_parts(?:_mut)?\b/g },
  { name: "`Box::into_raw` / `Box::from_raw`", re: /\bBox::(?:into_raw|from_raw)\b/g },
  { name: "`UnsafeCell`", re: /\bUnsafeCell\b/g },
  { name: "`&'static mut`", re: /&\s*'static\s+mut\b/g },
];

// Replaces comments and string and char literals with spaces, keeping line
// breaks, so the patterns only see code.
export function stripRust(src: string): string {
  let out = "";
  let i = 0;
  const n = src.length;
  const blank = (s: string) => s.replace(/[^\n]/g, " ");
  while (i < n) {
    const c = src[i];
    const next = src[i + 1];
    if (c === "/" && next === "/") {
      const end = src.indexOf("\n", i);
      const stop = end < 0 ? n : end;
      out += blank(src.slice(i, stop));
      i = stop;
    } else if (c === "/" && next === "*") {
      // Block comments nest in Rust.
      let depth = 0;
      let j = i;
      while (j < n) {
        if (src[j] === "/" && src[j + 1] === "*") { depth++; j += 2; }
        else if (src[j] === "*" && src[j + 1] === "/") { depth--; j += 2; if (depth === 0) break; }
        else j++;
      }
      out += blank(src.slice(i, j));
      i = j;
    } else if ((c === "r" || (c === "b" && next === "r")) && /^b?r#*"/.test(src.slice(i, i + 260)) && !/\w/.test(src[i - 1] ?? "")) {
      const m = /^b?r(#*)"/.exec(src.slice(i))!;
      const close = '"' + m[1];
      const end = src.indexOf(close, i + m[0].length);
      const stop = end < 0 ? n : end + close.length;
      out += blank(src.slice(i, stop));
      i = stop;
    } else if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') j += src[j] === "\\" ? 2 : 1;
      // Keep the ABI in `extern "C"`, which is code.
      const literal = src.slice(i, j + 1);
      out += /\bextern\s*$/.test(out) ? literal : blank(literal);
      i = j + 1;
    } else if (c === "'") {
      // A char literal ('a', '\n', '"'), or else a lifetime ('a).
      const m = /^'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'/.exec(src.slice(i, i + 16));
      if (m) { out += blank(m[0]); i += m[0].length; }
      else { out += c; i++; }
    } else {
      out += c;
      i++;
    }
  }
  return out;
}

export type Counts = { files: number; lines: number; cppLines: number; patterns: number[] };

function nonBlankLines(src: string): number {
  return src.split("\n").filter((l) => l.trim() !== "").length;
}

async function scan(dir: string, pattern: string): Promise<string[]> {
  const paths: string[] = [];
  for await (const p of new Glob(pattern).scan({ cwd: dir, onlyFiles: true })) {
    if (!p.split("/").some((part) => part === "target" || part === "build" || part === "node_modules" || part.startsWith(".")))
      paths.push(p);
  }
  return paths.sort();
}

export async function count(dir: string): Promise<Counts> {
  const counts: Counts = { files: 0, lines: 0, cppLines: 0, patterns: PATTERNS.map(() => 0) };
  for (const p of await scan(dir, "**/*.rs")) {
    const code = stripRust(await Bun.file(`${dir}/${p}`).text());
    counts.files++;
    counts.lines += nonBlankLines(code);
    PATTERNS.forEach((pat, k) => { counts.patterns[k] += code.match(pat.re)?.length ?? 0; });
  }
  for (const p of await scan(dir, "src/**/*.{c,cc,cpp,h,hpp}"))
    counts.cppLines += nonBlankLines(await Bun.file(`${dir}/${p}`).text());
  return counts;
}

function change(base: number, head: number): [string, string] {
  const d = head - base;
  const diff = d === 0 ? "0" : d > 0 ? `+${d}` : `${d}`;
  const pct = base === 0 ? (head === 0 ? "0%" : "new") : `${d > 0 ? "+" : ""}${((d / base) * 100).toFixed(1)}%`;
  return [diff, pct];
}

export function table(base: Counts, head: Counts): string {
  const rows: [string, number, number][] = [
    ...PATTERNS.map((p, k): [string, number, number] => [p.name, base.patterns[k], head.patterns[k]]),
    ["Rust files", base.files, head.files],
    ["Rust lines of code", base.lines, head.lines],
    ["C/C++ lines in `src/`", base.cppLines, head.cppLines],
  ];
  const per1k = (c: Counts) => (c.lines === 0 ? 0 : (c.patterns[0] + c.patterns[1]) * 1000 / c.lines);
  const lines = [
    MARKER,
    "### Unsafe and FFI patterns in Rust",
    "",
    "| Pattern | Base | This PR | Change | % |",
    "|:--|--:|--:|--:|--:|",
    ...rows.map(([name, b, h]) => `| ${name} | ${b} | ${h} | ${change(b, h).join(" | ")} |`),
    "",
    `\`unsafe\` blocks and functions per 1,000 lines of Rust: ${per1k(base).toFixed(1)} → ${per1k(head).toFixed(1)}.`,
    "",
    "<sub>Counted by `tools/rust-patterns/report.ts`, skipping comments and strings. Lines are non-blank lines.</sub>",
  ];
  return lines.join("\n") + "\n";
}

/// The patterns whose count is higher in head than in base.
export function risen(base: Counts, head: Counts): string[] {
  return PATTERNS.filter((_, k) => head.patterns[k] > base.patterns[k]).map((p) => p.name);
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const failOnRise = args[0] === "--fail-on-rise";
  const [baseDir, headDir] = failOnRise ? args.slice(1) : args;
  if (!baseDir || !headDir) {
    console.error("usage: bun tools/rust-patterns/report.ts [--fail-on-rise] <base-dir> <head-dir>");
    process.exit(2);
  }
  const [base, head] = [await count(baseDir), await count(headDir)];
  if (failOnRise) {
    const up = risen(base, head);
    if (up.length) {
      console.error(`These counts went up, and in phase 4 they only go down: ${up.join(", ")}`);
      process.exit(1);
    }
    console.log("No count went up.");
  } else {
    process.stdout.write(table(base, head));
  }
}
