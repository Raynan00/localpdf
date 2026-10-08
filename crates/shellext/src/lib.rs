//! Explorer context menu for the MSIX / Microsoft Store build of LocalPDF.
//!
//! Packaged apps can't add menu entries through the registry (their registry
//! writes are private to the package), so the package manifest declares this
//! DLL as an `IExplorerCommand` handler instead. Explorer loads it in a COM
//! surrogate and asks it for titles; when the user picks an entry it starts
//! `LocalPDF.exe <action> <files…>` from the same folder, exactly like the
//! registry entries of the classic installer do.
//!
//! On Windows 11 these entries appear in the main right-click menu, not only
//! under "Show more options".
#![cfg(windows)]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::core::{implement, Error, Interface, Ref, Result, BOOL, GUID, HRESULT, PWSTR};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_NOTIMPL, E_POINTER, HINSTANCE, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IEnumExplorerCommand_Impl, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray,
    SHStrDupW, ECF_DEFAULT, ECF_HASSUBCOMMANDS, ECS_ENABLED, SIGDN_FILESYSPATH,
};

/// "LocalPDF ▸" on PDFs. Must match the Clsid in the package manifest.
pub const CLSID_PDF_MENU: GUID = GUID::from_u128(0xc8dffbbc_3cb0_44c6_9ef0_ee8c214d518b);
/// "Convert to PDF (LocalPDF)" on images and Office files. Must match the manifest.
pub const CLSID_TO_PDF: GUID = GUID::from_u128(0xf4c974e7_f94c_4cb5_9771_cee5a8bed823);

/// Keep in step with `PDF_ACTIONS` in src-tauri/src/shell/mod.rs.
const PDF_ACTIONS: &[(&str, &str)] = &[
    ("Convert…", "convert"),
    ("Compress", "compress"),
    ("Merge selected files", "merge"),
    ("Split / extract pages…", "split"),
    ("Rotate pages…", "rotate"),
    ("Unlock…", "unlock"),
    ("Protect with password…", "protect"),
];

static MODULE: AtomicIsize = AtomicIsize::new(0);

#[no_mangle]
extern "system" fn DllMain(module: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(module.0 as isize, Ordering::Relaxed);
    }
    true.into()
}

/// LocalPDF.exe sits next to this DLL in the package.
fn app_exe() -> Option<PathBuf> {
    let module = HINSTANCE(MODULE.load(Ordering::Relaxed) as *mut c_void);
    let mut buf = vec![0u16; 32_768];
    let n = unsafe { GetModuleFileNameW(Some(module.into()), &mut buf) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let dll = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    Some(dll.parent()?.join("LocalPDF.exe"))
}

fn dup(s: &str) -> Result<PWSTR> {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { SHStrDupW(windows::core::PCWSTR(wide.as_ptr())) }
}

/// File-system paths of the selected items (virtual items are skipped).
fn selected_paths(items: &IShellItemArray) -> Result<Vec<String>> {
    let mut out = Vec::new();
    unsafe {
        for i in 0..items.GetCount()? {
            let item = items.GetItemAt(i)?;
            if let Ok(p) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                out.push(p.to_string().unwrap_or_default());
                CoTaskMemFree(Some(p.0 as *const c_void));
            }
        }
    }
    out.retain(|p| !p.is_empty());
    Ok(out)
}

fn launch(action: &str, items: &IShellItemArray) -> Result<()> {
    let exe = app_exe().ok_or_else(|| Error::from(E_FAIL))?;
    let files = selected_paths(items)?;
    std::process::Command::new(exe)
        .arg(action)
        .args(files)
        .spawn()
        .map(|_| ())
        .map_err(|_| Error::from(E_FAIL))
}

/// One menu entry: either the "LocalPDF" parent (no action, has children)
/// or a leaf that runs an action.
#[implement(IExplorerCommand)]
struct Command {
    title: &'static str,
    action: Option<&'static str>,
    children: bool,
}

impl IExplorerCommand_Impl for Command_Impl {
    fn GetTitle(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        dup(self.title)
    }

    fn GetIcon(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        // Only the top-level entry carries the app icon, like the classic menu.
        if self.action.is_some() && !self.children && self.title != TO_PDF_TITLE {
            return Err(E_NOTIMPL.into());
        }
        let exe = app_exe().ok_or_else(|| Error::from(E_FAIL))?;
        dup(&format!("{},0", exe.display()))
    }

    fn GetToolTip(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(GUID::zeroed())
    }

    fn GetState(&self, _items: Ref<IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }

    fn Invoke(&self, items: Ref<IShellItemArray>, _ctx: Ref<IBindCtx>) -> Result<()> {
        match (self.action, items.as_ref()) {
            (Some(action), Some(items)) => launch(action, items),
            _ => Ok(()),
        }
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(if self.children { ECF_HASSUBCOMMANDS.0 as u32 } else { ECF_DEFAULT.0 as u32 })
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        if !self.children {
            return Err(E_NOTIMPL.into());
        }
        let commands: Vec<IExplorerCommand> = PDF_ACTIONS
            .iter()
            .map(|&(title, action)| Command { title, action: Some(action), children: false }.into())
            .collect();
        Ok(SubCommands { commands, next: std::cell::Cell::new(0) }.into())
    }
}

const TO_PDF_TITLE: &str = "Convert to PDF (LocalPDF)";

#[implement(IEnumExplorerCommand)]
struct SubCommands {
    commands: Vec<IExplorerCommand>,
    next: std::cell::Cell<usize>,
}

impl IEnumExplorerCommand_Impl for SubCommands_Impl {
    fn Next(&self, count: u32, out: *mut Option<IExplorerCommand>, fetched: *mut u32) -> HRESULT {
        if out.is_null() {
            return E_POINTER;
        }
        let mut n = 0;
        while (n as u32) < count {
            let Some(cmd) = self.commands.get(self.next.get()) else { break };
            unsafe { out.add(n).write(Some(cmd.clone())) };
            self.next.set(self.next.get() + 1);
            n += 1;
        }
        if !fetched.is_null() {
            unsafe { *fetched = n as u32 };
        }
        if n as u32 == count {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, count: u32) -> Result<()> {
        self.next.set((self.next.get() + count as usize).min(self.commands.len()));
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        self.next.set(0);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(SubCommands { commands: self.commands.clone(), next: std::cell::Cell::new(self.next.get()) }.into())
    }
}

fn root_command(clsid: &GUID) -> Option<Command> {
    match *clsid {
        CLSID_PDF_MENU => Some(Command { title: "LocalPDF", action: None, children: true }),
        CLSID_TO_PDF => Some(Command { title: TO_PDF_TITLE, action: Some("convert"), children: false }),
        _ => None,
    }
}

#[implement(IClassFactory)]
struct Factory {
    clsid: GUID,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<windows::core::IUnknown>, iid: *const GUID, object: *mut *mut c_void) -> Result<()> {
        if object.is_null() || iid.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe { *object = std::ptr::null_mut() };
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let cmd: IExplorerCommand = root_command(&self.clsid).ok_or_else(|| Error::from(CLASS_E_CLASSNOTAVAILABLE))?.into();
        unsafe { cmd.query(iid, object).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// COM entry point: hand out a class factory for one of our two menu classes.
///
/// # Safety
/// Called by COM with valid pointers.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(clsid: *const GUID, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if clsid.is_null() || iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if root_command(&*clsid).is_none() {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory { clsid: *clsid }.into();
    factory.query(iid, out)
}

/// Explorer's surrogate decides when to unload; staying loaded is harmless.
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(cmd: &IExplorerCommand) -> String {
        unsafe {
            let p = cmd.GetTitle(None).unwrap();
            let s = p.to_string().unwrap();
            CoTaskMemFree(Some(p.0 as *const c_void));
            s
        }
    }

    #[test]
    fn class_factory_builds_both_menus() {
        unsafe {
            let mut raw = std::ptr::null_mut();
            DllGetClassObject(&CLSID_PDF_MENU, &IClassFactory::IID, &mut raw).ok().unwrap();
            let factory = IClassFactory::from_raw(raw);
            let root: IExplorerCommand = factory.CreateInstance(None).unwrap();
            assert_eq!(title(&root), "LocalPDF");
            assert_eq!(root.GetFlags().unwrap(), ECF_HASSUBCOMMANDS.0 as u32);

            let subs = root.EnumSubCommands().unwrap();
            let mut got = Vec::new();
            loop {
                let mut slot = [None];
                let mut n = 0;
                let _ = subs.Next(&mut slot, Some(&mut n));
                if n == 0 {
                    break;
                }
                got.push(title(slot[0].as_ref().unwrap()));
            }
            assert_eq!(got.len(), PDF_ACTIONS.len());
            assert_eq!(got[1], "Compress");

            let mut raw = std::ptr::null_mut();
            DllGetClassObject(&CLSID_TO_PDF, &IClassFactory::IID, &mut raw).ok().unwrap();
            let factory = IClassFactory::from_raw(raw);
            let convert: IExplorerCommand = factory.CreateInstance(None).unwrap();
            assert_eq!(title(&convert), TO_PDF_TITLE);

            let unknown = GUID::from_u128(1);
            let mut raw = std::ptr::null_mut();
            assert_eq!(DllGetClassObject(&unknown, &IClassFactory::IID, &mut raw), CLASS_E_CLASSNOTAVAILABLE);
        }
    }
}
