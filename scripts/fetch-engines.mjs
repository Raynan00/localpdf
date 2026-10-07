// Fetch the PDFium library LocalPDF bundles, verify it, and place it in
// src-tauri/engines/. Run once before `tauri build` (CI does this).
//
//   node scripts/fetch-engines.mjs            # for this machine
//   node scripts/fetch-engines.mjs mac-univ   # universal macOS build
//
// This is the only network access in the whole project, and it happens at
// build time on the build machine. The app itself never downloads anything.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const PDFIUM_TAG = "chromium/7881";
const SHA256 = {
  "win-x64": "73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08",
  "win-arm64": "d3035d4d2cacac6ecd1a2ece197a3d702a1b2a58466276b9f870b8cb278a9d84",
  "mac-arm64": "52e94ca5aa8847934330daf3f8150c190682c5ca93831468794f8b90d4392e40",
  "mac-x64": "6dedf83990e0e3d6b7c93c9e7589c5a126b0ae14b7464d76120cff7a26afb18b",
  "mac-univ": "df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3",
  "linux-x64": "1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d",
};

function hostTarget() {
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  if (process.platform === "win32") return `win-${arch}`;
  if (process.platform === "darwin") return `mac-${arch}`;
  return `linux-${arch}`;
}

const target = process.argv[2] ?? process.env.LOCALPDF_ENGINE_TARGET ?? hostTarget();
if (!SHA256[target]) {
  console.error(`Unknown target "${target}". Known: ${Object.keys(SHA256).join(", ")}`);
  process.exit(2);
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src-tauri", "engines");
const stamp = join(outDir, ".pdfium-version");
const lib = target.startsWith("win") ? "pdfium.dll" : target.startsWith("mac") ? "libpdfium.dylib" : "libpdfium.so";

if (existsSync(join(outDir, lib)) && existsSync(stamp) && readFileSync(stamp, "utf8") === `${PDFIUM_TAG} ${target}`) {
  console.log(`PDFium ${PDFIUM_TAG} (${target}) already in place.`);
  process.exit(0);
}

const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/${encodeURIComponent(PDFIUM_TAG)}/pdfium-${target}.tgz`;
console.log(`Downloading ${url}`);
const res = await fetch(url);
if (!res.ok) {
  console.error(`Download failed: HTTP ${res.status}`);
  process.exit(1);
}
const data = Buffer.from(await res.arrayBuffer());
const digest = createHash("sha256").update(data).digest("hex");
if (digest !== SHA256[target]) {
  console.error(`Checksum mismatch for pdfium-${target}.tgz\n  expected ${SHA256[target]}\n  got      ${digest}`);
  process.exit(1);
}

const work = mkdtempSync(join(tmpdir(), "localpdf-pdfium-"));
try {
  const archive = join(work, "pdfium.tgz");
  writeFileSync(archive, data);
  // bsdtar ships with Windows 10+, macOS and Linux.
  execFileSync("tar", ["-xzf", "pdfium.tgz"], { cwd: work, stdio: "inherit" });
  mkdirSync(outDir, { recursive: true });
  const src = join(work, target.startsWith("win") ? "bin" : "lib", lib);
  copyFileSync(src, join(outDir, lib));
  rmSync(join(outDir, "licenses"), { recursive: true, force: true });
  cpSync(join(work, "licenses"), join(outDir, "licenses"), { recursive: true });
  copyFileSync(join(work, "LICENSE"), join(outDir, "PDFIUM-LICENSE.txt"));
  writeFileSync(stamp, `${PDFIUM_TAG} ${target}`);
  console.log(`PDFium ${PDFIUM_TAG} (${target}) -> ${join(outDir, lib)}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
