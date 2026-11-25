import { cpSync, createReadStream, existsSync, statSync } from "node:fs";
import { extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const here = fileURLToPath(new URL(".", import.meta.url));
const repo = resolve(here, "..");
/** Repository folders served at /data and /assets. */
const SERVED = ["data", "assets"];
const TYPES: Record<string, string> = { ".json": "application/json", ".png": "image/png", ".md": "text/markdown" };
/** Never shipped: raw DEM/OSM downloads and external archives. */
const skip = (path: string) => path.includes(`${sep}raw`) || path.endsWith(".zip") || path.includes("contact_sheet");

function repoFiles(): Plugin {
  return {
    name: "ceasefire-repo-files",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = decodeURIComponent((req.url ?? "").split("?")[0] ?? "");
        const top = url.split("/")[1] ?? "";
        if (!SERVED.includes(top)) return next();
        const base = resolve(repo, top);
        const file = resolve(repo, `.${url}`);
        if (!file.startsWith(base + sep) || skip(file) || !existsSync(file) || !statSync(file).isFile()) {
          res.statusCode = 404;
          res.end();
          return;
        }
        res.setHeader("Content-Type", TYPES[extname(file)] ?? "application/octet-stream");
        createReadStream(file).pipe(res);
      });
    },
    closeBundle() {
      for (const dir of SERVED) {
        cpSync(resolve(repo, dir), resolve(here, "dist", dir), { recursive: true, filter: (src) => !skip(src) });
      }
    },
  };
}

export default defineConfig({
  plugins: [repoFiles()],
  build: { target: "es2022" },
});
