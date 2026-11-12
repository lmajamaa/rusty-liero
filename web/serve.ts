// Serves a web build locally, like GitHub Pages would: static files only, with
// Bun picking the MIME type from the extension (application/wasm for .wasm) and
// no COOP/COEP headers, so anything that needs threads fails here as it would there.
//
//   bun web/serve.ts [dir] [--port N]     # dir defaults to build/web
//
// A folder's page is its index.html, or the Emscripten build's openliero.html.

import { resolve, join, sep } from "node:path";

const args = process.argv.slice(2);
const portFlag = args.indexOf("--port");
const port = portFlag >= 0 ? Number(args.splice(portFlag, 2)[1]) : 8080;
const root = resolve(args[0] ?? "build/web");

const server = Bun.serve({
  port,
  async fetch(req) {
    let path = decodeURIComponent(new URL(req.url).pathname);
    if (path.endsWith("/")) {
      path += (await Bun.file(join(root, path, "index.html")).exists()) ? "index.html" : "openliero.html";
    }

    const full = join(root, path);
    if (full !== root && !full.startsWith(root + sep)) {
      return new Response("Forbidden", { status: 403 });
    }

    const file = Bun.file(full);
    if (!(await file.exists())) {
      return new Response("Not found", { status: 404 });
    }
    // Always serve the latest build while developing.
    return new Response(file, { headers: { "Cache-Control": "no-store" } });
  },
});

console.log(`Serving ${root} at ${server.url}`);
