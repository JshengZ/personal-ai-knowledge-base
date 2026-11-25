import { spawn } from "node:child_process";
import {
  mkdir,
  readFile,
  writeFile,
  copyFile,
  chmod,
  readdir,
  stat,
} from "node:fs/promises";
import { createReadStream, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, resolve, dirname } from "node:path";
import { createRequire } from "node:module";
import { gunzipSync } from "node:zlib";
import { unzipSync } from "fflate";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { BookOpenIcon } from "@phosphor-icons/react";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import pngToIco from "png-to-ico";

const require = createRequire(import.meta.url);
const cache = resolve(".local/downloads");
await mkdir(cache, { recursive: true });

async function digest(path) {
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  return hash.digest("hex");
}
async function run(command, args) {
  const child = spawn(command, args, { stdio: "inherit" });
  const code = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", resolve);
  });
  if (code) throw new Error(command + " failed with " + code);
}
async function download(url, path, checksum, headers = []) {
  if (existsSync(path) && checksum && (await digest(path)) === checksum) return;
  await mkdir(dirname(path), { recursive: true });
  await run("curl", [
    "--fail",
    "--silent",
    "--show-error",
    "--location",
    "--http1.1",
    "--continue-at",
    "-",
    "--retry",
    "8",
    "--retry-all-errors",
    "--connect-timeout",
    "15",
    "--speed-limit",
    "1024",
    "--speed-time",
    "40",
    ...headers.flatMap((value) => ["--header", value]),
    "--output",
    path,
    url,
  ]);
  if (checksum && (await digest(path)) !== checksum)
    throw new Error("Checksum mismatch: " + path);
}

const model = {
  file: "resources/runtime/models/Qwen3-Embedding-0.6B-Q8_0.gguf",
  sha256: "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439",
  url: "https://modelscope.cn/models/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/master/Qwen3-Embedding-0.6B-Q8_0.gguf",
};
if (!process.argv.includes("--small-only"))
  await download(model.url, resolve(model.file), model.sha256);

async function find(directory, name) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isFile() && entry.name === name) return path;
    if (entry.isDirectory()) {
      const found = await find(path, name);
      if (found) return found;
    }
  }
  return null;
}

for (const language of ["eng", "chi_sim"]) {
  const source = await find(
    dirname(
      require.resolve("@tesseract.js-data/" + language + "/package.json"),
    ),
    language + ".traineddata.gz",
  );
  if (!source) throw new Error("OCR package is incomplete: " + language);
  const target = resolve("resources/ocr", language + ".traineddata");
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, gunzipSync(await readFile(source)));
}

const svg = renderToStaticMarkup(
  React.createElement(BookOpenIcon, {
    size: 128,
    weight: "thin",
    color: "#292b25",
  }),
);
await mkdir("resources", { recursive: true });
await mkdir("build", { recursive: true });
for (const [name, size, background] of [
  ["tray.png", 32, false],
  ["icon.png", 1024, true],
]) {
  const canvas = createCanvas(size, size);
  const context = canvas.getContext("2d");
  if (background) {
    context.fillStyle = "#f8f6ef";
    context.fillRect(0, 0, size, size);
  }
  const image = await loadImage(Buffer.from(svg));
  const margin = background ? size * 0.22 : 0;
  context.drawImage(
    image,
    margin,
    margin,
    size - margin * 2,
    size - margin * 2,
  );
  await writeFile(join("resources", name), canvas.toBuffer("image/png"));
}
await writeFile(
  "build/icon.ico",
  await pngToIco(await readFile("resources/icon.png")),
);

if (process.platform === "darwin") {
  const iconset = resolve("build/icon.iconset");
  await mkdir(iconset, { recursive: true });
  for (const size of [16, 32, 128, 256, 512]) {
    for (const density of [1, 2]) {
      const canvas = createCanvas(size * density, size * density);
      const context = canvas.getContext("2d");
      const image = await loadImage(await readFile("resources/icon.png"));
      context.drawImage(image, 0, 0, size * density, size * density);
      await writeFile(
        join(
          iconset,
          "icon_" + size + "x" + size + (density === 2 ? "@2x" : "") + ".png",
        ),
        canvas.toBuffer("image/png"),
      );
    }
  }
  await run("iconutil", ["-c", "icns", iconset, "-o", "build/icon.icns"]);
}

const runtimes = [
  {
    platform: "mac-x64",
    archive: "llama-mac-x64.tar.gz",
    url: "https://api.github.com/repos/ggml-org/llama.cpp/releases/assets/789989365?download=1",
    sha256: "407ba426f33a63bd189116e296a939adf55e12c2ad3244abd27860aef0276400",
    binary: "llama-server",
  },
  {
    platform: "win-x64",
    archive: "llama-win-x64.zip",
    url: "https://api.github.com/repos/ggml-org/llama.cpp/releases/assets/789991286?download=1",
    sha256: "fd2408e502e1416de55f2759cc252dee23ce8d034cc7e4682a8cb6e5e2ddf1b8",
    binary: "llama-server.exe",
  },
];
if (!process.argv.includes("--small-only"))
  for (const runtime of runtimes) {
    const directory = resolve("resources/runtime", runtime.platform);
    await mkdir(directory, { recursive: true });
    const custom = join(directory, "build-manifest.json");
    if (existsSync(custom)) {
      const build = JSON.parse(await readFile(custom, "utf8"));
      if (
        build.version === "b12711" &&
        build.binary_sha256 === (await digest(join(directory, runtime.binary)))
      )
        continue;
      throw new Error(
        "Custom runtime integrity check failed: " + runtime.platform,
      );
    }
    const marker = join(directory, ".verified-sha256");
    if (
      existsSync(marker) &&
      (await readFile(marker, "utf8")).trim() === runtime.sha256
    )
      continue;
    const archive = join(cache, runtime.archive);
    await download(runtime.url, archive, runtime.sha256, [
      "Accept: application/octet-stream",
    ]);
    const staging = join(cache, runtime.platform);
    await mkdir(staging, { recursive: true });
    if (runtime.archive.endsWith(".zip")) {
      for (const [name, bytes] of Object.entries(
        unzipSync(await readFile(archive)),
      )) {
        if (name.startsWith("/") || name.split("/").includes(".."))
          throw new Error("Invalid archive path");
        if (name.endsWith("/")) continue;
        const path = join(staging, name);
        await mkdir(dirname(path), { recursive: true });
        await writeFile(path, bytes);
      }
    } else await run("tar", ["-xzf", archive, "-C", staging]);
    const binary = await find(staging, runtime.binary);
    if (!binary) throw new Error("Runtime binary missing");
    for (const entry of await readdir(dirname(binary), {
      withFileTypes: true,
    })) {
      if (!entry.isFile() && !entry.isSymbolicLink()) continue;
      if (entry.name === runtime.binary || /\.(dylib|dll)$/.test(entry.name)) {
        const target = join(directory, entry.name);
        await copyFile(join(dirname(binary), entry.name), target);
        if (!entry.name.endsWith(".dll")) await chmod(target, 0o755);
      }
    }
    const license = join(dirname(binary), "LICENSE");
    if (existsSync(license))
      await copyFile(license, join(directory, "LICENSE"));
    await writeFile(marker, runtime.sha256 + "\n");
  }
for (const runtime of runtimes) {
  const custom = resolve(
    "resources/runtime",
    runtime.platform,
    "build-manifest.json",
  );
  if (existsSync(custom))
    runtime.custom_build = JSON.parse(await readFile(custom, "utf8"));
}
await writeFile(
  "resources/assets-manifest.json",
  JSON.stringify(
    {
      schema: 1,
      model,
      llama_version: "b12711",
      runtimes,
      ocr: { languages: ["eng", "chi_sim"], source: "@tesseract.js-data" },
      font: "Noto Serif SC",
      icon: "Phosphor BookOpen thin",
    },
    null,
    2,
  ),
);
console.log("Local resources prepared.");
