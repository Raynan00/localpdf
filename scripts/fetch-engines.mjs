// Fetch the engines LocalPDF bundles, verify them, and stage them for the build.
//
//   node scripts/fetch-engines.mjs win-x64     # Windows installer
//   node scripts/fetch-engines.mjs mac-arm64   # Apple Silicon app
//   node scripts/fetch-engines.mjs mac-x64     # Intel Mac app
//   node scripts/fetch-engines.mjs linux-x64   # dev only (PDFium; LibreOffice comes from the system)
//
// PDFium      -> src-tauri/engines/                     (bundled as Tauri resources)
// LibreOffice -> src-tauri/engines/libreoffice/          on Windows (bundled as resources)
//                src-tauri/engines-mac/LibreOffice.app   on macOS (copied into the .app by
//                                                         scripts/macos-package.sh, keeping symlinks)
//
// These downloads are the only network access in the project, and they happen on
// the build machine. The app itself never downloads anything.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  copyFileSync, cpSync, createReadStream, createWriteStream, existsSync, mkdirSync, mkdtempSync,
  readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------- pins
const PDFIUM_TAG = "chromium/7881";
const PDFIUM_SHA256 = {
  "win-x64": "73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08",
  "win-arm64": "d3035d4d2cacac6ecd1a2ece197a3d702a1b2a58466276b9f870b8cb278a9d84",
  "mac-arm64": "52e94ca5aa8847934330daf3f8150c190682c5ca93831468794f8b90d4392e40",
  "mac-x64": "6dedf83990e0e3d6b7c93c9e7589c5a126b0ae14b7464d76120cff7a26afb18b",
  "mac-univ": "df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3",
  "linux-x64": "1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d",
};

// LibreOffice: pinned version, verified against the SHA-256 that The Document
// Foundation publishes next to each file on its primary download server.
const LO_VERSION = process.env.LOCALPDF_LO_VERSION ?? "26.8.1";
const LO_FILES = {
  "win-x64": `win/x86_64/LibreOffice_${LO_VERSION}_Win_x86-64.msi`,
  "mac-arm64": `mac/aarch64/LibreOffice_${LO_VERSION}_MacOS_aarch64.dmg`,
  "mac-x64": `mac/x86_64/LibreOffice_${LO_VERSION}_MacOS_x86-64.dmg`,
};
const LO_BASES = [
  `https://download.documentfoundation.org/libreoffice/stable/${LO_VERSION}/`,
  `https://downloadarchive.documentfoundation.org/libreoffice/old/${LO_VERSION}/`,
];

// Parts of LibreOffice that headless conversion to PDF never touches. Licence
// files stay. CI converts .docx/.xlsx/.pptx with the trimmed copy to prove it.
const LO_PRUNE_WIN = [
  "help", "readmes", "share/gallery", "share/template", "share/wordbook",
  "share/extensions/wiki-publisher", "share/extensions/nlpsolver",
  "program/classes", "program/python-core-*", "program/python.exe", "program/pythonw.exe",
  "share/extensions/dict-*",
];
const LO_PRUNE_MAC = [
  "Contents/Resources/help", "Contents/Resources/gallery", "Contents/Resources/template",
  "Contents/Resources/wordbook", "Contents/Resources/java",
  "Contents/Resources/extensions/wiki-publisher", "Contents/Resources/extensions/nlpsolver",
  "Contents/Resources/extensions/dict-*",
  "Contents/Frameworks/LibreOfficePython.framework",
];

// ---------------------------------------------------------------- setup
function hostTarget() {
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  if (process.platform === "win32") return `win-${arch}`;
  if (process.platform === "darwin") return `mac-${arch}`;
  return `linux-${arch}`;
}
const target = process.argv[2] ?? process.env.LOCALPDF_ENGINE_TARGET ?? hostTarget();
if (!PDFIUM_SHA256[target]) {
  console.error(`Unknown target "${target}". Known: ${Object.keys(PDFIUM_SHA256).join(", ")}`);
  process.exit(2);
}
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const engines = join(root, "src-tauri", "engines");
const enginesMac = join(root, "src-tauri", "engines-mac");
const cache = process.env.LOCALPDF_ENGINE_CACHE ?? join(homedir(), ".cache", "localpdf-engines");
mkdirSync(engines, { recursive: true });
mkdirSync(cache, { recursive: true });

const sha256File = (path) =>
  new Promise((resolve, reject) => {
    const h = createHash("sha256");
    createReadStream(path).on("data", (d) => h.update(d)).on("end", () => resolve(h.digest("hex"))).on("error", reject);
  });

async function download(url, dest) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(dest + ".part"));
  renameSync(dest + ".part", dest);
}

function sizeOf(path) {
  if (!existsSync(path)) return 0;
  const st = statSync(path);
  if (!st.isDirectory()) return st.size;
  return readdirSync(path).reduce((n, f) => n + sizeOf(join(path, f)), 0);
}
const mb = (bytes) => `${(bytes / 1024 / 1024).toFixed(0)} MB`;

function prune(base, patterns) {
  let freed = 0;
  for (const pat of patterns) {
    const dir = join(base, dirname(pat));
    const leaf = pat.split("/").pop();
    if (!existsSync(dir)) continue;
    const re = new RegExp("^" + leaf.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*") + "$");
    for (const name of readdirSync(dir).filter((n) => re.test(n))) {
      const p = join(dir, name);
      freed += sizeOf(p);
      rmSync(p, { recursive: true, force: true });
    }
  }
  return freed;
}

// ---------------------------------------------------------------- PDFium
async function fetchPdfium() {
  const lib = target.startsWith("win") ? "pdfium.dll" : target.startsWith("mac") ? "libpdfium.dylib" : "libpdfium.so";
  const stamp = join(engines, ".pdfium-version");
  if (existsSync(join(engines, lib)) && existsSync(stamp) && readFileSync(stamp, "utf8") === `${PDFIUM_TAG} ${target}`) {
    console.log(`PDFium ${PDFIUM_TAG} (${target}) already in place.`);
    return;
  }
  const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/${encodeURIComponent(PDFIUM_TAG)}/pdfium-${target}.tgz`;
  const work = mkdtempSync(join(tmpdir(), "localpdf-pdfium-"));
  try {
    const archive = join(work, "pdfium.tgz");
    console.log(`Downloading ${url}`);
    await download(url, archive);
    const digest = await sha256File(archive);
    if (digest !== PDFIUM_SHA256[target]) throw new Error(`PDFium checksum mismatch: expected ${PDFIUM_SHA256[target]}, got ${digest}`);
    execFileSync("tar", ["-xzf", "pdfium.tgz"], { cwd: work, stdio: "inherit" });
    copyFileSync(join(work, target.startsWith("win") ? "bin" : "lib", lib), join(engines, lib));
    rmSync(join(engines, "licenses"), { recursive: true, force: true });
    cpSync(join(work, "licenses"), join(engines, "licenses"), { recursive: true });
    copyFileSync(join(work, "LICENSE"), join(engines, "PDFIUM-LICENSE.txt"));
    writeFileSync(stamp, `${PDFIUM_TAG} ${target}`);
    console.log(`PDFium ${PDFIUM_TAG} (${target}) -> ${join(engines, lib)}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------- LibreOffice
async function listVersions(url) {
  try {
    const res = await fetch(url);
    if (!res.ok) return [];
    const html = await res.text();
    return [...new Set([...html.matchAll(/href="(\d+\.\d+\.\d+(?:\.\d+)?)\/"/g)].map((m) => m[1]))];
  } catch {
    return [];
  }
}

// TDF's download server (MirrorBrain) publishes hashes in a few forms; accept any.
async function publishedSha256(fileUrl) {
  const tries = [fileUrl + ".sha256", fileUrl + "?sha256", fileUrl + ".mirrorlist"];
  for (const u of tries) {
    try {
      const res = await fetch(u);
      const text = res.ok ? await res.text() : "";
      const m = text.match(/\b([0-9a-f]{64})\b/i);
      console.log(`  ${res.status} ${u}${m ? "  -> sha256 found" : ""}`);
      if (m) return m[1].toLowerCase();
    } catch (e) {
      console.log(`  ERR ${u}: ${e.message}`);
    }
  }
  return null;
}

async function fetchLibreOfficeArchive() {
  const rel = LO_FILES[target];
  const name = rel.split("/").pop();
  for (const base of LO_BASES) {
    console.log(`Looking for ${base + rel}`);
    const expected = await publishedSha256(base + rel);
    if (!expected) continue;
    const local = join(cache, name);
    if (!existsSync(local) || (await sha256File(local)) !== expected) {
      console.log(`Downloading ${base + rel}`);
      await download(base + rel, local);
    }
    const digest = await sha256File(local);
    if (digest !== expected) throw new Error(`LibreOffice checksum mismatch for ${name}: expected ${expected}, got ${digest}`);
    console.log(`Verified ${name} (sha256 ${digest.slice(0, 16)}…)`);
    return local;
  }
  const stable = await listVersions("https://download.documentfoundation.org/libreoffice/stable/");
  throw new Error(
    `LibreOffice ${LO_VERSION} (${rel}) not found with a published checksum.\n` +
    `Versions on the stable server: ${stable.join(", ") || "(listing unavailable)"}\n` +
    `Set LO_VERSION in scripts/fetch-engines.mjs (or LOCALPDF_LO_VERSION) to one of them.`);
}

async function fetchLibreOffice() {
  if (!LO_FILES[target]) {
    console.log(`LibreOffice is not bundled for ${target}; a system install is used in development.`);
    return;
  }
  const stampDir = target.startsWith("win") ? join(engines, "libreoffice") : join(enginesMac, "LibreOffice.app");
  const stamp = join(stampDir, ".localpdf-version");
  if (existsSync(stamp) && readFileSync(stamp, "utf8") === `${LO_VERSION} ${target}`) {
    console.log(`LibreOffice ${LO_VERSION} (${target}) already in place.`);
    return;
  }
  const archive = await fetchLibreOfficeArchive();
  rmSync(stampDir, { recursive: true, force: true });
  const work = mkdtempSync(join(tmpdir(), "localpdf-lo-"));
  try {
    if (target.startsWith("win")) {
      // Administrative install: unpacks the MSI into a folder, installs nothing.
      execFileSync("msiexec", ["/a", archive, "/qn", `TARGETDIR=${work}`], { stdio: "inherit" });
      const unpacked = join(work, "LibreOffice");
      const before = sizeOf(unpacked);
      const freed = prune(unpacked, LO_PRUNE_WIN);
      cpSync(unpacked, stampDir, { recursive: true });
      console.log(`LibreOffice ${LO_VERSION}: ${mb(before)} unpacked, trimmed ${mb(freed)} -> ${mb(sizeOf(stampDir))}`);
    } else {
      const mnt = join(work, "mnt");
      mkdirSync(mnt);
      execFileSync("hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mnt, archive], { stdio: "inherit" });
      try {
        mkdirSync(enginesMac, { recursive: true });
        // ditto keeps the bundle's symlinks and metadata intact.
        execFileSync("ditto", [join(mnt, "LibreOffice.app"), stampDir], { stdio: "inherit" });
      } finally {
        execFileSync("hdiutil", ["detach", mnt, "-force"], { stdio: "inherit" });
      }
      const before = sizeOf(stampDir);
      const freed = prune(stampDir, LO_PRUNE_MAC);
      // Trimming breaks The Document Foundation's signature; seal it again
      // (ad-hoc, like the app itself) so macOS will run it.
      execFileSync("xattr", ["-cr", stampDir]);
      execFileSync("codesign", ["--force", "--deep", "--sign", "-", stampDir], { stdio: "inherit" });
      console.log(`LibreOffice ${LO_VERSION}: ${mb(before)} unpacked, trimmed ${mb(freed)} -> ${mb(sizeOf(stampDir))}`);
    }
    writeFileSync(stamp, `${LO_VERSION} ${target}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

await fetchPdfium();
await fetchLibreOffice();
