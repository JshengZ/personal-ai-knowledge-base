import { readFile, stat, access } from "node:fs/promises";
import { createRequire } from "node:module";
import { extname, join, dirname } from "node:path";
import { TextDecoder } from "node:util";
import { XMLParser } from "fast-xml-parser";
import { unzipSync } from "fflate";
import { parse as parseCsv } from "csv-parse/sync";
import ExcelJS from "exceljs";

const require = createRequire(import.meta.url);
const xml = new XMLParser({
  preserveOrder: true,
  ignoreAttributes: false,
  processEntities: false,
  trimValues: false,
});
const maximumBytes = 32 * 1024 * 1024;
const maximumCharacters = 4 * 1024 * 1024;

function blockList() {
  const blocks = [];
  let characters = 0;
  blocks.push = (...items) => {
    characters += items.reduce((sum, item) => sum + item.text.length, 0);
    if (characters > maximumCharacters || blocks.length + items.length > 5000)
      throw new Error("解析结果超过安全上限，请拆分文档后加入。");
    return Array.prototype.push.apply(blocks, items);
  };
  return blocks;
}

function xmlText(value) {
  const named = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
  return String(value).replace(
    /&(amp|lt|gt|quot|apos|#x[0-9a-f]+|#\d+);/gi,
    (whole, entity) => {
      if (Object.hasOwn(named, entity)) return named[entity];
      if (!entity.startsWith("#")) return whole;
      const point =
        entity[1].toLowerCase() === "x"
          ? parseInt(entity.slice(2), 16)
          : parseInt(entity.slice(1), 10);
      return point > 0 &&
        point <= 0x10ffff &&
        !(point >= 0xd800 && point <= 0xdfff)
        ? String.fromCodePoint(point)
        : whole;
    },
  );
}

function textOf(nodes) {
  if (!Array.isArray(nodes)) return "";
  return nodes
    .map((node) => {
      if (Object.hasOwn(node, "#text")) return xmlText(node["#text"]);
      return Object.entries(node)
        .filter(([key]) => key !== ":@")
        .map(([key, value]) => {
          if (/:(br|cr|tab)$/.test(key))
            return key.endsWith("tab") ? "\t" : "\n";
          return textOf(value);
        })
        .join("");
    })
    .join("");
}

function descendants(nodes, tag, found = []) {
  if (!Array.isArray(nodes)) return found;
  for (const node of nodes) {
    for (const [key, value] of Object.entries(node)) {
      if (key === tag) found.push(value);
      else if (key !== ":@") descendants(value, tag, found);
    }
  }
  return found;
}

function splitText(text, location) {
  text = text.trim();
  if (text.length > maximumCharacters)
    throw new Error("正文超过安全上限，请拆分文档后加入。");
  const blocks = blockList();
  for (let offset = 0; offset < text.length;) {
    let end = Math.min(text.length, offset + 2400);
    if (end < text.length && /[\uD800-\uDBFF]/.test(text[end - 1])) end--;
    const piece = text.slice(offset, end);
    if (piece.trim()) blocks.push({ text: piece, location });
    if (end >= text.length) break;
    offset += 2160;
    if (/[\uDC00-\uDFFF]/.test(text[offset])) offset++;
  }
  return blocks;
}

function decode(bytes) {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return new TextDecoder("gb18030").decode(bytes);
  }
}

function parseText(bytes, extension) {
  if (bytes.length > 8 * 1024 * 1024)
    throw new Error("文本文件超过 8 MB 的安全上限，请拆分后加入。");
  let text = decode(bytes).replace(/^\uFEFF/, "");
  if (text.includes("\u0000"))
    throw new Error("文件包含二进制内容，无法作为文本读取。");
  if ([".html", ".htm"].includes(extension)) {
    text = text
      .replace(/<(script|style)[\s\S]*?<\/\1>/gi, "")
      .replace(/<br\s*\/?\s*>/gi, "\n")
      .replace(/<\/(p|div|h[1-6]|li)>/gi, "\n")
      .replace(/<[^>]+>/g, "")
      .replace(/&nbsp;/g, " ")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&amp;/g, "&");
  }
  if (extension === ".ipynb") {
    const notebook = JSON.parse(text);
    const blocks = blockList();
    for (const [index, cell] of (notebook.cells || []).entries()) {
      if (!["code", "markdown"].includes(cell.cell_type)) continue;
      const content = Array.isArray(cell.source)
        ? cell.source.join("")
        : String(cell.source || "");
      blocks.push(
        ...splitText(content, { label: "第 " + (index + 1) + " 个单元格" }),
      );
    }
    return { blocks, ocr: false };
  }
  const lines = text.replace(/\r\n/g, "\n").split("\n", 100002);
  if (lines.length > 100000)
    throw new Error("文本超过 10 万行，请拆分后加入。");
  const blocks = blockList();
  let start = 0;
  while (start < lines.length) {
    let end = start;
    let size = 0;
    while (
      end < lines.length &&
      end - start < 80 &&
      (size < 2400 || end === start)
    ) {
      size += lines[end].length + 1;
      end++;
    }
    const content = lines.slice(start, end).join("\n").trim();
    const location = {
      label: "第 " + (start + 1) + "–" + end + " 行",
      line_start: start + 1,
      line_end: end,
    };
    blocks.push(...splitText(content, location));
    if (end === lines.length) break;
    // Bound overlap by characters, not a fixed five lines. Long lines used to
    // repeat most of the previous block and multiply the embedding workload.
    const budget = Math.min(240, Math.floor(size * 0.1));
    let overlap = 0;
    let next = end;
    while (next > start + 1 && overlap + lines[next - 1].length + 1 <= budget) {
      overlap += lines[next - 1].length + 1;
      next--;
    }
    start = Math.max(start + 1, next);
  }
  return { blocks, ocr: false };
}

function officeEntries(bytes, pattern) {
  let total = 0;
  return unzipSync(bytes, {
    filter(entry) {
      if (!pattern.test(entry.name)) return false;
      total += entry.originalSize;
      if (entry.originalSize > 8 * 1024 * 1024 || total > 32 * 1024 * 1024)
        throw new Error("文档解压后的内容过大。");
      return true;
    },
  });
}

function parseDocx(bytes) {
  const entries = officeEntries(
    bytes,
    /^word\/(document|footnotes|endnotes)\.xml$/,
  );
  if (!entries["word/document.xml"])
    throw new Error("DOCX 正文不存在，文件可能已损坏或加密。");
  const paragraphs = descendants(
    xml.parse(decode(entries["word/document.xml"])),
    "w:p",
  );
  const blocks = blockList();
  let group = [];
  let groupStart = 1;
  let groupEnd = 1;
  let groupSize = 0;
  const flush = () => {
    if (!group.length) return;
    blocks.push(
      ...splitText(group.join("\n"), {
        label:
          groupStart === groupEnd
            ? "第 " + groupStart + " 段"
            : "第 " + groupStart + "–" + groupEnd + " 段",
      }),
    );
    group = [];
    groupSize = 0;
  };
  for (const [index, paragraph] of paragraphs.entries()) {
    const text = textOf(paragraph).trim();
    if (!text) continue;
    if (groupSize + text.length + 1 > 2400) flush();
    if (!group.length) groupStart = index + 1;
    group.push(text);
    groupEnd = index + 1;
    groupSize += text.length + 1;
    if (groupSize >= 2160) flush();
  }
  flush();
  for (const name of ["word/footnotes.xml", "word/endnotes.xml"]) {
    if (!entries[name]) continue;
    for (const paragraph of descendants(
      xml.parse(decode(entries[name])),
      "w:p",
    )) {
      blocks.push(
        ...splitText(textOf(paragraph), {
          label: name.includes("footnotes") ? "脚注" : "尾注",
        }),
      );
    }
  }
  return { blocks, ocr: false };
}

function parsePptx(bytes) {
  const entries = officeEntries(
    bytes,
    /^ppt\/(slides\/slide\d+|notesSlides\/notesSlide\d+)\.xml$/,
  );
  const names = Object.keys(entries)
    .filter((name) => name.startsWith("ppt/slides/"))
    .sort(
      (a, b) =>
        Number(a.match(/slide(\d+)/)[1]) - Number(b.match(/slide(\d+)/)[1]),
    );
  const blocks = blockList();
  for (const name of names) {
    const number = Number(name.match(/slide(\d+)/)[1]);
    const text = descendants(xml.parse(decode(entries[name])), "a:p")
      .map(textOf)
      .join("\n");
    blocks.push(
      ...splitText(text, { label: "第 " + number + " 张幻灯片", page: number }),
    );
    const notes = entries["ppt/notesSlides/notesSlide" + number + ".xml"];
    if (notes)
      blocks.push(
        ...splitText(
          descendants(xml.parse(decode(notes)), "a:p")
            .map(textOf)
            .join("\n"),
          { label: "第 " + number + " 张幻灯片的备注", page: number },
        ),
      );
  }
  return { blocks, ocr: false };
}

function valueText(value) {
  if (value === null || value === undefined) return "";
  if (value instanceof Date) return value.toISOString();
  if (typeof value !== "object") return String(value);
  if (Array.isArray(value.richText))
    return value.richText.map((part) => part.text).join("");
  if (
    Object.hasOwn(value, "formula") ||
    Object.hasOwn(value, "sharedFormula")
  ) {
    return value.result === undefined
      ? "[公式，未保存计算结果]"
      : valueText(value.result);
  }
  if (value.text !== undefined) return String(value.text);
  if (value.error) return String(value.error);
  return "";
}

async function parseXlsx(bytes) {
  officeEntries(bytes, /\.xml$/);
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(bytes);
  const blocks = blockList();
  for (const sheet of workbook.worksheets) {
    const rows = [];
    sheet.eachRow({ includeEmpty: false }, (row, number) => {
      const cells = [];
      row.eachCell({ includeEmpty: false }, (cell) => {
        const text = valueText(cell.value).trim();
        if (text) cells.push(cell.address + " " + text);
      });
      if (cells.length) rows.push({ number, text: cells.join(" | ") });
    });
    for (let index = 0; index < rows.length; index += 12) {
      const group = rows.slice(index, index + 12);
      const range =
        "A" +
        group[0].number +
        ":" +
        sheet.getColumn(Math.max(1, sheet.columnCount)).letter +
        group.at(-1).number;
      const heading = index ? "表头参考：" + rows[0].text + "\n" : "";
      blocks.push(
        ...splitText(heading + group.map((row) => row.text).join("\n"), {
          label: sheet.name + " · " + range,
          sheet: sheet.name,
          cells: range,
        }),
      );
    }
  }
  return { blocks, ocr: false };
}

function parseCsvFile(bytes, delimiter) {
  const rows = parseCsv(decode(bytes), {
    bom: true,
    relax_column_count: true,
    skip_empty_lines: true,
    delimiter,
    max_record_size: 2 * 1024 * 1024,
  });
  const blocks = blockList();
  for (let index = 0; index < rows.length; index += 16) {
    const group = rows.slice(index, index + 16);
    const heading = index ? "表头：" + rows[0].join(" | ") + "\n" : "";
    blocks.push(
      ...splitText(heading + group.map((row) => row.join(" | ")).join("\n"), {
        label: "第 " + (index + 1) + "–" + (index + group.length) + " 行",
        line_start: index + 1,
        line_end: index + group.length,
      }),
    );
  }
  return { blocks, ocr: false };
}

async function createOcr(resources) {
  const directory = join(resources, "ocr");
  for (const language of ["eng", "chi_sim"]) {
    try {
      await access(join(directory, language + ".traineddata"));
    } catch {
      throw new Error("缺少本地 OCR 语言资源，请使用完整安装包。");
    }
  }
  const { createWorker } = await import("tesseract.js");
  return createWorker(["chi_sim", "eng"], 1, {
    langPath: directory,
    gzip: false,
    cacheMethod: "none",
    corePath: dirname(require.resolve("tesseract.js-core/package.json")),
    workerPath: require.resolve("tesseract.js/src/worker-script/node/index.js"),
    logger: () => {},
  });
}

async function parsePdf(bytes, options) {
  const canvas = await import("@napi-rs/canvas");
  globalThis.DOMMatrix = canvas.DOMMatrix;
  globalThis.Path2D = canvas.Path2D;
  globalThis.ImageData = canvas.ImageData;
  const pdfjs = await import("pdfjs-dist/legacy/build/pdf.mjs");
  const distribution = dirname(require.resolve("pdfjs-dist/package.json"));
  const loading = pdfjs.getDocument({
    data: new Uint8Array(bytes),
    isEvalSupported: false,
    useSystemFonts: true,
    verbosity: 0,
    standardFontDataUrl:
      join(distribution, "standard_fonts").replaceAll("\\", "/") + "/",
    wasmUrl: join(distribution, "wasm").replaceAll("\\", "/") + "/",
    useWorkerFetch: false,
  });
  let worker;
  const blocks = blockList();
  let ocr = false;
  try {
    const document = await loading.promise;
    if (document.numPages > 1000)
      throw new Error("PDF 超过 1000 页，请拆分后加入。");
    for (let pageNumber = 1; pageNumber <= document.numPages; pageNumber++) {
      const page = await document.getPage(pageNumber);
      const content = await page.getTextContent();
      let text = content.items
        .map((item) =>
          "str" in item ? item.str + (item.hasEOL ? "\n" : " ") : "",
        )
        .join("")
        .trim();
      if (text.length < 30 && options.ocr_enabled) {
        worker ||= await createOcr(options.resources);
        const viewport = page.getViewport({ scale: 1.6 });
        if (viewport.width * viewport.height > 30_000_000)
          throw new Error("PDF 页面尺寸过大，无法完成 OCR。");
        const surface = canvas.createCanvas(
          Math.ceil(viewport.width),
          Math.ceil(viewport.height),
        );
        await page.render({
          canvas: surface,
          canvasContext: surface.getContext("2d"),
          viewport,
        }).promise;
        const result = await worker.recognize(surface.toBuffer("image/png"));
        text = result.data.text.trim();
        ocr = true;
      }
      blocks.push(
        ...splitText(text, {
          label: "第 " + pageNumber + " 页" + (ocr ? " · 含 OCR" : ""),
          page: pageNumber,
        }),
      );
      page.cleanup();
    }
  } finally {
    if (worker) await worker.terminate();
    await loading.destroy();
  }
  return { blocks, ocr };
}

export async function parseFile(options) {
  const metadata = await stat(options.path);
  if (metadata.size > maximumBytes)
    throw new Error("文件超过解析器的 32 MB 安全上限，请拆分后加入。");
  const bytes = await readFile(options.path);
  const extension = extname(options.path).toLowerCase();
  switch (extension) {
    case ".docx":
      return parseDocx(bytes);
    case ".pptx":
      return parsePptx(bytes);
    case ".xlsx":
      return parseXlsx(bytes);
    case ".csv":
      return parseCsvFile(bytes, ",");
    case ".tsv":
      return parseCsvFile(bytes, "\t");
    case ".pdf":
      return parsePdf(bytes, options);
    case ".png":
    case ".jpg":
    case ".jpeg":
    case ".tif":
    case ".tiff": {
      if (!options.ocr_enabled)
        throw new Error("请在设置中启用 OCR 以读取图片。");
      const worker = await createOcr(options.resources);
      try {
        const result = await worker.recognize(bytes);
        return {
          blocks: splitText(result.data.text, { label: "图片文字 · OCR" }),
          ocr: true,
        };
      } finally {
        await worker.terminate();
      }
    }
    default:
      return parseText(bytes, extension);
  }
}

if (process.argv.includes("--server")) {
  const { createInterface } = await import("node:readline");
  console.log = (...values) => console.error(...values);
  for await (const line of createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  })) {
    if (!line.trim()) continue;
    try {
      process.stdout.write(
        JSON.stringify({ ok: true, ...(await parseFile(JSON.parse(line))) }) +
          "\n",
      );
    } catch (error) {
      process.stdout.write(
        JSON.stringify({
          ok: false,
          error: error instanceof Error ? error.message : "文件解析失败",
        }) + "\n",
      );
    }
  }
}

if (process.argv.includes("--stdio")) {
  const originalLog = console.log;
  console.log = (...values) => console.error(...values);
  try {
    const chunks = [];
    for await (const chunk of process.stdin) chunks.push(chunk);
    const options = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    const result = await parseFile(options);
    process.stdout.write(JSON.stringify({ ok: true, ...result }));
  } catch (error) {
    process.stdout.write(
      JSON.stringify({
        ok: false,
        error: error instanceof Error ? error.message : "文件解析失败",
      }),
    );
    process.exitCode = 1;
  } finally {
    console.log = originalLog;
  }
}
