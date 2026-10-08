First release of LocalPDF: offline PDF tools in the Windows Explorer and macOS Finder right-click menu.

Right-click a PDF to compress, merge, split, rotate, unlock or password-protect it, or convert it to Word, JPG, PNG or text. Right-click a photo, scan, Word, Excel or PowerPoint file to turn it into a PDF. The result lands next to the original, which is never changed.

Everything runs on your computer: no uploads, no account, no telemetry, and no internet needed. Free and open source (MIT).

## Download

| Your computer | File | Size |
|---|---|---|
| Windows 10 / 11 | `LocalPDF_0.1.0_x64-setup.exe` | about 130 MB |
| Mac with Apple Silicon (M1 and later) | `LocalPDF_0.1.0_aarch64.dmg` | about 190 MB |
| Mac with Intel | `LocalPDF_0.1.0_x64.dmg` | about 195 MB |
| Windows, if Smart App Control blocks the installer (see below) | `LocalPDF_0.1.0_x64-lite-setup.exe` | about 5 MB |

Not sure which Mac you have? Apple menu → **About This Mac**. "Chip: Apple M…" means Apple Silicon; "Processor: Intel" means Intel.

Most of the download is the built-in Office converter (LibreOffice), so Word, Excel and PowerPoint files convert without installing anything else.

## Install

### Windows
1. Run `LocalPDF_0.1.0_x64-setup.exe`. It installs for your user account only, so it doesn't ask for admin rights.
2. If a blue box says **"Windows protected your PC"**, click **More info → Run anyway**. This appears because the installer isn't code-signed yet.
3. If your browser (for example Edge) says the file **"isn't commonly downloaded"**, open the downloads list, click **⋯ → Keep → Show more → Keep anyway**.
4. Right-click any PDF. On Windows 11, LocalPDF is under **Show more options** (or press Shift+F10).

**"Smart App Control blocked an app"?** Some Windows 11 PCs have Smart App Control turned on, and it blocks unsigned installers with no way to continue. A code-signed installer is on the way. Until then, use **LocalPDF Lite** (`…-lite-setup.exe`): the same PDF tools in a much smaller installer. Lite doesn't include the Office converter; it converts Word, Excel and PowerPoint files if LibreOffice is installed on your PC. Turning Smart App Control off isn't a good workaround: it checks apps every time they run, so LocalPDF would only keep working while it stays off.

### Mac (macOS 11 or later)
1. Open the `.dmg` and drag **LocalPDF** into **Applications**. Run it from Applications, not from the disk image.
2. Open LocalPDF. Because it isn't notarized by Apple yet, macOS blocks the first launch:
   - **macOS 15 (Sequoia) or later:** close the warning, go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to LocalPDF, then enter your password.
   - **macOS 14 or earlier:** right-click LocalPDF in Applications, choose **Open**, then **Open** again.

   You only do this once.
3. That first launch adds LocalPDF to Finder. Right-click a file → **Quick Actions** → **LocalPDF: …**

If the Quick Actions don't show up, turn them on in **System Settings → Privacy & Security → Extensions → Finder**, or via **Quick Actions → Customize…** in the right-click menu.

## What's in this release
- **PDFs:** Compress, Merge (select several files), Split or extract pages, Rotate, Unlock (with the password), Protect with a password, Convert to Word, JPG, PNG or text.
- **To PDF:** photos and scans (JPG, PNG, TIFF, BMP, GIF, WebP), and Word, Excel, PowerPoint and OpenDocument files.
- Outputs are named next to the original (`report-compressed.pdf`, `scan.pdf`) and never overwrite anything.
- Clear messages for encrypted files, wrong passwords, invalid page ranges, damaged files and unsupported inputs.

## Uninstall
- **Windows:** Settings → Apps → LocalPDF → Uninstall. The right-click entries are removed with it.
- **Mac:** open LocalPDF and click **Remove** to take it out of the right-click menu, then move LocalPDF to the Bin.

## Known limitations
- The builds aren't code-signed yet, hence the one-time warnings above.
- Scanned PDFs without a text layer can't be converted to text (no OCR yet).
- No e-signing yet.

Found a bug or have an idea? [Open an issue](https://github.com/Raynan00/localpdf/issues).
