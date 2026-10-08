# LocalPDF: offline PDF tools in your right-click menu (Windows and Mac)

**Merge, compress, split, rotate, convert, unlock and password-protect PDFs without uploading them anywhere.** LocalPDF adds these actions to the Windows Explorer and macOS Finder right-click menu and processes every file on your own computer. No uploads, no account, no ads, no telemetry. Free and open source.

Most "free PDF tools" are websites that upload your contracts, bank statements and IDs to someone else's server. LocalPDF does the same jobs offline: right-click, pick an action, and the result appears next to the original file.

| You want to… | Right-click → |
|---|---|
| Combine several PDFs into one | **Merge selected files** |
| Make a PDF smaller for email | **Compress** |
| Pull out some pages, or split a PDF into separate files | **Split / extract pages** |
| Fix sideways or upside-down pages | **Rotate pages** |
| Turn a PDF into Word (DOCX), JPG, PNG or text | **Convert** |
| Turn photos, scans or Word, Excel and PowerPoint files into a PDF | **Convert to PDF** |
| Remove a password you know | **Unlock** |
| Add a password (AES-256) | **Protect with password** |

**Why LocalPDF**
- **Private by design:** files never leave your machine, and it works with Wi-Fi off. Here's [how to check that yourself](#how-to-confirm-files-never-leave-your-machine).
- **No app to open:** it lives in the right-click menu, and dialogs appear only when an action needs input.
- **Safe:** originals are never modified or overwritten.
- **One install, nothing else:** everything is built in, including the Word, Excel and PowerPoint converter. Built on qpdf, PDFium (the PDF engine inside Google Chrome) and LibreOffice.
- **Free and open source** (MIT).

Works on **Windows 10, Windows 11 and macOS 11 or later**, on Intel and Apple Silicon.

## Install

### Windows 10 / 11

1. Run `LocalPDF_x.y.z_x64-setup.exe`. It installs for the current user, so it doesn't need admin rights. The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC": click **More info → Run anyway**.
2. Right-click a PDF. On Windows 10 the **LocalPDF** submenu is in the main menu. On Windows 11 it's under **Show more options** (or Shift+F10), because Windows 11 only puts signed, packaged shell extensions in its short menu.

The installer adds the menu entries and the uninstaller removes them. Everything is written under `HKCU\Software\Classes\SystemFileAssociations`. To add or remove the entries without reinstalling:

```
"%LOCALAPPDATA%\LocalPDF\LocalPDF.exe" --register
"%LOCALAPPDATA%\LocalPDF\LocalPDF.exe" --unregister
```

LocalPDF uses the WebView2 runtime, which comes with Windows 11 and up-to-date Windows 10. If it's missing, the installer runs Microsoft's bootstrapper. That is the only download, it happens at install time, and it comes from Microsoft rather than LocalPDF.

### macOS 11 or later

1. Download the DMG for your Mac: `LocalPDF_x.y.z_aarch64.dmg` for Apple Silicon (M1 and later), `LocalPDF_x.y.z_x64.dmg` for Intel. Open it and drag LocalPDF to Applications.
2. Open LocalPDF once. That first launch adds the Finder Quick Actions. The build isn't notarized yet, so macOS blocks the first launch:
   - **macOS 15 (Sequoia) or later:** open the app, close the warning, then go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to LocalPDF.
   - **macOS 14 or earlier:** right-click the app in Applications, choose **Open**, then **Open** again.
   - Or in Terminal: `xattr -dr com.apple.quarantine /Applications/LocalPDF.app`
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
- **Unsupported conversion inputs.** Unknown file types, folders, mixing PDFs with images or Office files in one Convert, and PDFs without a text layer converted to text (scans) each get a plain explanation.
- **Damaged files** are reported per file. The other files in the selection still get processed.

## Bundled engines

| Engine | What it does | How it ships | License |
|---|---|---|---|
| [qpdf](https://github.com/qpdf/qpdf) 12.4 | Merge, split, rotate, encrypt and decrypt, stream recompression, building PDFs from images | Compiled into the app binary from source (`qpdf` crate, `vendored` feature, with its own zlib and libjpeg) | Apache-2.0 |
| [PDFium](https://pdfium.googlesource.com/pdfium/) chromium/7881 | Page rendering (PNG/JPEG), text extraction, PDF to Word | `pdfium.dll` / `libpdfium.dylib` from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), checked against a pinned SHA-256 at build time | BSD-3-Clause and Apache-2.0 (third-party notices in `engines/licenses`) |
| [LibreOffice](https://www.libreoffice.org) 26.8.0 | Word, Excel, PowerPoint and OpenDocument files to PDF | Bundled: the official build, checked against The Document Foundation's published SHA-256, with parts conversion never uses removed (help, galleries, templates, spell dictionaries, Java, Python). Runs as `soffice --headless` with a throwaway profile. Inside `engines\libreoffice` on Windows and `LocalPDF.app/Contents/Resources/LibreOffice.app` on macOS | MPL-2.0 |

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

Office conversion runs the LibreOffice that ships inside LocalPDF as a separate `soffice` process with a fresh temporary profile. A headless conversion doesn't go online, and you can block `soffice` the same way if you want to be sure.

## Build from source

You need Rust (stable), Node 22, and on Linux the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Linux builds run for development, but the menu integration is Windows and macOS only.

```
npm ci
node scripts/fetch-engines.mjs win-x64    # PDFium + LibreOffice (mac-arm64 / mac-x64 on a Mac)
npx tauri build --bundles nsis            # Windows installer

# macOS, per architecture (LibreOffice has no universal build):
npx tauri build --target aarch64-apple-darwin --bundles app
scripts/macos-package.sh target/aarch64-apple-darwin/release/bundle/macos/LocalPDF.app aarch64
```

Download sizes: about 130 MB for the Windows installer (590 MB once installed) and about 190 MB for each Mac disk image (515 MB in Applications). Most of that is LibreOffice, trimmed to what Office conversion needs; the app itself (qpdf compiled in, plus PDFium) is about 15 MB.

Engine tests, including generated PDFs, encryption round trips, merge failures and range errors:

```
cargo test -p localpdf-core
```

There's also a command-line front end to the same engine, useful for scripting:

```
cargo run -p localpdf-core --bin localpdf-cli -- split --mode ranges --pages "1-3, 4-" report.pdf
```

CI (`.github/workflows/build.yml`) runs the engine tests on Windows, macOS and Linux. It also builds the Windows installer, installs it silently, checks the Explorer entries, uninstalls and checks they're gone. On macOS it builds the Apple Silicon and Intel apps, registers the Quick Actions, validates every generated plist, then unregisters. On both platforms it converts real Word, Excel and PowerPoint files with the bundled LibreOffice.

## Layout

```
crates/core/        engine: qpdf ops (doc.rs), PDFium renders (render.rs), DOCX writer,
                    Office bridge, page ranges, output naming, job planning (lib.rs)
src-tauri/          desktop shell: job windows (app.rs), Explorer registry and Finder
                    Quick Actions (shell/), NSIS hooks (windows/hooks.nsh)
ui/                 dialog and status UI (React + Radix ToggleGroup, custom CSS)
scripts/            fetch-engines.mjs: pinned, checksummed PDFium download
```

## FAQ

**Is there a free offline alternative to Adobe Acrobat for merging and compressing PDFs?**
Yes. LocalPDF merges, compresses, splits, rotates and converts PDFs offline on Windows and Mac for free. It isn't a full editor: it doesn't edit text or fill forms.

**Can I compress a PDF without uploading it?**
Yes. Right-click the PDF and choose **Compress**. The smaller copy is saved next to the original, and nothing is uploaded.

**Can I convert PDF to Word offline?**
Yes. Choose **Convert → Word**. Text is extracted into an editable DOCX. Scanned pages are added as images, because LocalPDF doesn't do OCR.

**How do I merge PDFs on Windows or Mac without installing Acrobat?**
Select the PDFs, right-click, and choose **Merge selected files**. They're combined in file-name order.

**Can it remove a PDF password?**
Only if you know the password. **Unlock** saves an unprotected copy; it doesn't crack passwords.

**Does it work without internet?**
Yes, completely. LocalPDF contains no networking code.

## Not included

No visual editor, OCR, audio or video conversion, Linux file-manager integration, cloud sync, accounts or analytics. Merging keeps pages but drops bookmarks and form fields. Multi-page TIFFs and animated GIFs convert their first frame only.
