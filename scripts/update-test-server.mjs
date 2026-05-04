import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createHttpsServer } from "node:https";
import { extname, join, normalize, resolve, sep } from "node:path";
import process from "node:process";

const root = resolve(readArg("--root") ?? join(process.cwd(), "artifacts", "update-test", "server"));
const port = Number(readArg("--port") ?? process.env.UPDATE_TEST_PORT ?? 7531);
const pfxPath = resolve(
  readArg("--pfx") ?? join(process.cwd(), "artifacts", "update-test", "certs", "localhost.pfx")
);
const passphrase = readArg("--passphrase") ?? process.env.UPDATE_TEST_CERT_PASSWORD ?? "torch-overlay-update-test";

if (!existsSync(join(root, "latest.json"))) {
  throw new Error(`latest.json was not found under ${root}. Run npm run update:test:build first.`);
}

const listener = (request, response) => {
  const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "127.0.0.1"}`);
  const pathname = decodeURIComponent(url.pathname === "/" ? "/latest.json" : url.pathname);
  const filePath = resolve(join(root, pathname));
  console.log(`${new Date().toISOString()} ${request.method} ${pathname}`);

  if (!filePath.startsWith(root + sep) && filePath !== root) {
    response.writeHead(403).end("Forbidden");
    return;
  }

  if (!existsSync(filePath) || !statSync(filePath).isFile()) {
    response.writeHead(404).end("Not Found");
    return;
  }

  response.writeHead(200, {
    "Content-Type": contentType(filePath),
    "Cache-Control": "no-store",
    "Access-Control-Allow-Origin": "*"
  });
  createReadStream(filePath).pipe(response);
};

const useHttps = existsSync(pfxPath);
const server = useHttps
  ? createHttpsServer({ pfx: readFileSync(pfxPath), passphrase }, listener)
  : createHttpServer(listener);

server.listen(port, "127.0.0.1", () => {
  const protocol = useHttps ? "https" : "http";
  console.log(
    JSON.stringify(
      {
        ok: true,
        latest: `${protocol}://localhost:${port}/latest.json`,
        root: normalize(root)
      },
      null,
      2
    )
  );
});

function readArg(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function contentType(filePath) {
  if (extname(filePath) === ".json") {
    return "application/json; charset=utf-8";
  }

  if (extname(filePath) === ".sig") {
    return "text/plain; charset=utf-8";
  }

  return "application/octet-stream";
}
