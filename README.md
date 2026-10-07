# LocalPDF

Common PDF actions in the Explorer and Finder right-click menu. Every file is processed on your computer: no uploads, no account, no telemetry.

Right-click one or more PDFs and choose **Convert**, **Compress**, **Merge selected files**, **Split / extract pages**, **Rotate pages**, **Unlock** or **Protect with password**. Images and Office documents get **Convert to PDF**. Results are written next to the originals, which are never modified.

## Install

### Windows 10 / 11

1. Run `LocalPDF_x.y.z_x64-setup.exe`. It installs for the current user, so it doesn't need admin rights.
2. Right-click a PDF. On Windows 10 the **LocalPDF** submenu is in the main menu. On Windows 11 it's under **Show more options** (or Shift+F10), because Windows 11 only puts signed, packaged shell extensions in its short menu.

The installer adds the menu entries and the uninstaller removes them. Everything is written under `HKCU\Software\Classes\SystemFileAssociations`. To add or remove the entries without reinstalling:

```
"%LOCALAPPDATA%\LocalPDF\LocalPDF.exe" --register
"%LOCALAPPDATA%\LocalPDF\LocalPDF.exe" --unregister
```

LocalPDF uses the WebView2 runtime, which comes with Windows 11 and up-to-date Windows 10. If it's missing, the installer runs Microsoft's bootstrapper. That is the only download, it happens at install time, and it comes from Microsoft rather than LocalPDF.

### macOS 11 or later

1. Open `LocalPDF_x.y.z_universal.dmg` and drag LocalPDF to Applications.
2. Open LocalPDF once. The build is ad-hoc signed and not notarized, so the first launch needs a right-click on the app, then **Open**. That first launch adds the Finder Quick Actions.
3. Right-click a PDF and choose **Quick Actions** → **LocalPDF: …** (also listed under **Services**). If they don't show up, enable them in **System Settings → Privacy & Security → Extensions → Finder**, or use **Quick Actions → Customize…**

The Quick Actions are Automator services in `~/Library/Services/LocalPDF - *.workflow`. Each one hands the selected files to the app. To remove them, open LocalPDF and click **Remove**, or run:

```
/Applications/LocalPDF.app/Contents/MacOS/LocalPDF --unregister
```

Do this before you drag the app to the Trash. macOS has no uninstall hook, so it can't clean up after a deleted app. If you move the app, open it once and it will update the Quick Actions to the new location.

## Actions

| Action | Works on | Asks for | Writes |
|---|---|---|---|
| Convert | PDFs | Word, PNG, JPEG or text; resolution for images | `name.docx`, `name.txt`, `name.png` (1 page) or a `name (images)` folder |
| Convert to PDF | Images (PNG, JPEG, TIFF, BMP, GIF, WebP) | One PDF or one per image, only when several are selected | `name.pdf` / `name-combined.pdf` |
| Convert to PDF | Office (doc, docx, odt, rtf, xls, xlsx, ods, ppt, pptx, odp) | Nothing | `name.pdf` |
| Compress | PDFs | Nothing (one click) | `name-compressed.pdf` |
| Merge selected files | 2+ PDFs | Nothing (one click). Order is by file name, natural sort | `first-merged.pdf` |
| Split / extract pages | PDFs | Extract pages into one PDF, one PDF per range, or every page | `name-p2-4.pdf`, … or a `name (pages)` folder |
| Rotate pages | PDFs | 90° right, 180°, 90° left; optional page range | `name-rotated.pdf` |
| Unlock | Password-protected PDFs | The file's password | `name-unlocked.pdf` |
| Protect with password | PDFs | New password, typed twice | `name-protected.pdf` (AES-256) |

A dialog only appears when an action needs input. One-click actions start right away and show a small status window that closes on its own when everything went well. If a name is taken, LocalPDF adds ` (2)`, ` (3)` and so on. It never overwrites anything.

Page ranges look like `1-3, 5, 8-`: `8-` means page 8 to the end, `-3` means pages 1 to 3, and `last` is the last page. They're checked as you type against the real page count.

Selecting several files in Explorer starts one LocalPDF process per file. The first one becomes the app, and the others pass their file to it within half a second. That's how Merge gets the whole selection.

### Errors you might see

- **Password-protected files.** Every action except Unlock stops with "*file* is password-protected. Use Unlock first." Merge names the file that blocked it. Files that only restrict editing or printing (owner password only) open normally.
- **Wrong password.** Unlock keeps the dialog open so you can try again.
- **Invalid page ranges.** Syntax errors, page 0, pages past the end and backwards ranges (`5-2`) are reported inline before anything runs. When several files are selected, the message names the file the range doesn't fit.
- **Failed merges.** If any input is damaged, encrypted or not a PDF, nothing is written, and the message names the file.
- **Unsupported conversion inputs.** Unknown file types, folders, mixing PDFs with images or Office files in one Convert, and PDFs without a text layer converted to text (scans) each get a plain explanation. Office conversion without LibreOffice installed says so and links to it.
- **Damaged files** are reported per file. The other files in the selection still get processed.

## Bundled engines

| Engine | What it does | How it ships | License |
|---|---|---|---|
| [qpdf](https://github.com/qpdf/qpdf) 12.4 | Merge, split, rotate, encrypt and decrypt, stream recompression, building PDFs from images | Compiled into the app binary from source (`qpdf` crate, `vendored` feature, with its own zlib and libjpeg) | Apache-2.0 |
| [PDFium](https://pdfium.googlesource.com/pdfium/) chromium/7881 | Page rendering (PNG/JPEG), text extraction, PDF to Word | `pdfium.dll` / `libpdfium.dylib` from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), checked against a pinned SHA-256 at build time | BSD-3-Clause and Apache-2.0 (third-party notices in `engines/licenses`) |
| [LibreOffice](https://www.libreoffice.org) | Office documents to PDF | **Not bundled** (about 350 MB). If installed, LocalPDF runs `soffice --headless` with a throwaway profile. Point `LOCALPDF_SOFFICE` at a custom location | MPL-2.0 |

Ghostscript isn't included. Its AGPL license and size don't suit a small installer. Compression re-encodes photographic images as JPEG (quality 72, at most 2000 px on the long side), leaves masks, line art and unusual colour spaces alone, recompresses every other stream and packs objects into object streams. If the result isn't at least 3% smaller, LocalPDF says so and writes nothing.

**PDF to Word** extracts the text of each page, joins hard-wrapped lines back into paragraphs and starts each PDF page on a new Word page. Pages without a text layer, such as scans and drawings, go in as page images, so nothing goes missing. It doesn't recreate the layout: columns, tables and fonts become plain flowing text.

## How to confirm files never leave your machine

LocalPDF contains no networking code, and you can check that yourself:

1. **Watch it.** Process a few files while monitoring:
   - Windows: Resource Monitor (`resmon`) → Network → tick `LocalPDF.exe`. No connections appear.
   - macOS: `nettop -p LocalPDF` in Terminal, or a firewall such as LuLu or Little Snitch. No connections appear.
2. **Block it and see that nothing breaks.**
   - Windows (admin PowerShell): `New-NetFirewallRule -DisplayName "Block LocalPDF" -Direction Outbound -Action Block -Program "$env:LOCALAPPDATA\LocalPDF\LocalPDF.exe"`
   - macOS: deny LocalPDF in LuLu or Little Snitch, or turn Wi-Fi off. Every action keeps working.
3. **Read the code.** The UI runs under a Content Security Policy that only allows the app's own IPC (`connect-src ipc: http://ipc.localhost`, see `src-tauri/tauri.conf.json`). No updater, analytics or crash-reporting plugins are included (`src-tauri/Cargo.toml`). The engine crate has no HTTP, TLS or async-networking dependency: `cargo tree -p localpdf-core` lists none. The only download in the project is `scripts/fetch-engines.mjs`, which fetches PDFium on the build machine.
4. **Build it yourself** (below) and compare.

Office conversion runs your installed LibreOffice as a separate `soffice` process with a fresh temporary profile. A headless conversion doesn't go online, and you can block `soffice` the same way if you want to be sure.

## Build from source

You need Rust (stable), Node 22, and on Linux the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Linux builds run for development, but the menu integration is Windows and macOS only.

```
npm ci
node scripts/fetch-engines.mjs            # PDFium for this machine (mac-univ for a universal macOS build)
npx tauri build                           # Windows: NSIS installer. macOS: .app and .dmg
npx tauri build --target universal-apple-darwin   # universal macOS build
```

Sizes from CI: the Windows installer is 5.0 MB, and the universal macOS DMG is 12 MB (both Intel and Apple Silicon code, 28 MB unpacked). That covers the app binary with qpdf compiled in, plus PDFium.

Engine tests, including generated PDFs, encryption round trips, merge failures and range errors:

```
cargo test -p localpdf-core
```

There's also a command-line front end to the same engine, useful for scripting:

```
cargo run -p localpdf-core --bin localpdf-cli -- split --mode ranges --pages "1-3, 4-" report.pdf
```

CI (`.github/workflows/build.yml`) runs the engine tests on Windows, macOS and Linux. It also builds the Windows installer, installs it silently, checks the Explorer entries, uninstalls and checks they're gone. On macOS it builds the universal app, registers the Quick Actions, validates every generated plist, then unregisters.

## Layout

```
crates/core/        engine: qpdf ops (doc.rs), PDFium renders (render.rs), DOCX writer,
                    Office bridge, page ranges, output naming, job planning (lib.rs)
src-tauri/          desktop shell: job windows (app.rs), Explorer registry and Finder
                    Quick Actions (shell/), NSIS hooks (windows/hooks.nsh)
ui/                 dialog and status UI (React + Radix ToggleGroup, custom CSS)
scripts/            fetch-engines.mjs: pinned, checksummed PDFium download
```

## Not included

No visual editor, OCR, audio or video conversion, Linux file-manager integration, cloud sync, accounts or analytics. Merging keeps pages but drops bookmarks and form fields. Multi-page TIFFs and animated GIFs convert their first frame only.
