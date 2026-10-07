//! Explorer context menu, written under HKCU\Software\Classes.
//!
//! PDFs get a cascading "LocalPDF" entry with every action; images and Office
//! files get a single "Convert to PDF (LocalPDF)" entry. On Windows 11 these
//! classic entries live under "Show more options".

use std::io;
use std::path::Path;

use localpdf_core::kinds::{IMAGE_EXTS, OFFICE_EXTS};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
use winreg::RegKey;

use super::{Status, PDF_ACTIONS};

const PDF_KEY: &str = r"Software\Classes\SystemFileAssociations\.pdf\shell\LocalPDF";
const TO_PDF_VERB: &str = "LocalPDF.ToPdf";

fn to_pdf_key(ext: &str) -> String {
    format!(r"Software\Classes\SystemFileAssociations\.{ext}\shell\{TO_PDF_VERB}")
}

fn command(exe: &Path, action: &str) -> String {
    format!("\"{}\" {action} \"%1\"", exe.display())
}

pub fn register(exe: &Path) -> io::Result<()> {
    // Start clean so renamed or removed verbs from older versions disappear.
    let _ = unregister_keys();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let icon = format!("{},0", exe.display());

    let (root, _) = hkcu.create_subkey(PDF_KEY)?;
    root.set_value("MUIVerb", &"LocalPDF")?;
    root.set_value("Icon", &icon)?;
    root.set_value("SubCommands", &"")?;
    root.set_value("MultiSelectModel", &"Player")?;
    for (i, (id, label, action)) in PDF_ACTIONS.iter().enumerate() {
        // Explorer sorts sub-verbs by key name; the prefix keeps our order.
        let (verb, _) = root.create_subkey(format!(r"shell\{:02}{id}", i + 1))?;
        verb.set_value("MUIVerb", label)?;
        verb.set_value("MultiSelectModel", &"Player")?;
        let (cmd, _) = verb.create_subkey("command")?;
        cmd.set_value("", &command(exe, action))?;
    }

    for ext in IMAGE_EXTS.iter().chain(OFFICE_EXTS) {
        let (verb, _) = hkcu.create_subkey(to_pdf_key(ext))?;
        verb.set_value("MUIVerb", &"Convert to PDF (LocalPDF)")?;
        verb.set_value("Icon", &icon)?;
        verb.set_value("MultiSelectModel", &"Player")?;
        let (cmd, _) = verb.create_subkey("command")?;
        cmd.set_value("", &command(exe, "convert"))?;
    }
    notify_shell();
    Ok(())
}

fn unregister_keys() -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut result = Ok(());
    let keys = std::iter::once(PDF_KEY.to_string())
        .chain(IMAGE_EXTS.iter().chain(OFFICE_EXTS).map(|e| to_pdf_key(e)));
    for key in keys {
        match hkcu.delete_subkey_all(&key) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => result = Err(e),
        }
    }
    result
}

pub fn unregister() -> io::Result<()> {
    let r = unregister_keys();
    notify_shell();
    r
}

pub fn status(exe: Option<&Path>) -> Status {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let cmd: Option<String> = hkcu
        .open_subkey_with_flags(format!(r"{PDF_KEY}\shell\02compress\command"), KEY_READ)
        .and_then(|k| k.get_value(""))
        .ok();
    let registered = cmd.is_some();
    let stale = match (cmd, exe) {
        (Some(c), Some(exe)) => !c.starts_with(&format!("\"{}\"", exe.display())),
        _ => false,
    };
    Status { supported: true, registered, stale, location: "Explorer right-click menu" }
}

fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as _, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) };
}
