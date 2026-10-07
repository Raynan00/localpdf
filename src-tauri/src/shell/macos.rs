//! Finder Quick Actions: one Automator service per action in
//! ~/Library/Services. Each runs a one-line shell script that hands the
//! selected files to this app. They show up under Quick Actions (and the
//! Services menu) in Finder's right-click menu.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Status, PDF_ACTIONS};

const PREFIX: &str = "LocalPDF - ";

const TO_PDF_TYPES: &[&str] = &[
    "public.image",
    "com.microsoft.word.doc",
    "org.openxmlformats.wordprocessingml.document",
    "org.oasis-open.opendocument.text",
    "public.rtf",
    "com.microsoft.excel.xls",
    "org.openxmlformats.spreadsheetml.sheet",
    "org.oasis-open.opendocument.spreadsheet",
    "com.microsoft.powerpoint.ppt",
    "org.openxmlformats.presentationml.presentation",
    "org.oasis-open.opendocument.presentation",
];

struct Service {
    file: String,
    menu: String,
    action: &'static str,
    send_types: &'static [&'static str],
    input_type: &'static str,
}

fn services() -> Vec<Service> {
    let mut list: Vec<Service> = PDF_ACTIONS
        .iter()
        .map(|(id, label, action)| Service {
            file: format!("{PREFIX}{}", capitalize(id)),
            menu: format!("LocalPDF: {label}"),
            action,
            send_types: &["com.adobe.pdf"],
            input_type: "com.apple.Automator.fileSystemObject.PDF",
        })
        .collect();
    list.push(Service {
        file: format!("{PREFIX}Convert to PDF"),
        menu: "LocalPDF: Convert to PDF".into(),
        action: "convert",
        send_types: TO_PDF_TYPES,
        input_type: "com.apple.Automator.fileSystemObject",
    });
    list
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn services_dir() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home).join("Library/Services"))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Single-quote for /bin/sh.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn script(exe: &Path, action: &str) -> String {
    // Detach so the Finder spinner stops right away; the app owns the job.
    format!("{} {action} \"$@\" >/dev/null 2>&1 &", sh_quote(&exe.to_string_lossy()))
}

fn info_plist(s: &Service) -> String {
    let types: String = s.send_types.iter().map(|t| format!("<string>{t}</string>")).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSBackgroundColorName</key>
			<string>background</string>
			<key>NSIconName</key>
			<string>NSActionTemplate</string>
			<key>NSMenuItem</key>
			<dict>
				<key>default</key>
				<string>{menu}</string>
			</dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict>
				<key>NSApplicationIdentifier</key>
				<string>com.apple.finder</string>
			</dict>
			<key>NSSendFileTypes</key>
			<array>{types}</array>
		</dict>
	</array>
</dict>
</plist>
"#,
        menu = xml(&s.menu),
    )
}

fn document_wflow(s: &Service, exe: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key>
	<string>523</string>
	<key>AMApplicationVersion</key>
	<string>2.10</string>
	<key>AMDocumentVersion</key>
	<string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Optional</key>
					<true/>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>AMActionVersion</key>
				<string>2.0.3</string>
				<key>AMApplication</key>
				<array>
					<string>Automator</string>
				</array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key>
					<dict/>
					<key>CheckedForUserDefaultShell</key>
					<dict/>
					<key>inputMethod</key>
					<dict/>
					<key>shell</key>
					<dict/>
					<key>source</key>
					<dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>ActionBundlePath</key>
				<string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key>
				<string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key>
					<string>{command}</string>
					<key>CheckedForUserDefaultShell</key>
					<true/>
					<key>inputMethod</key>
					<integer>1</integer>
					<key>shell</key>
					<string>/bin/sh</string>
					<key>source</key>
					<string></string>
				</dict>
				<key>BundleIdentifier</key>
				<string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key>
				<string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key>
				<false/>
				<key>CanShowWhenRun</key>
				<true/>
				<key>Category</key>
				<array>
					<string>AMCategoryUtilities</string>
				</array>
				<key>Class Name</key>
				<string>RunShellScriptAction</string>
				<key>InputUUID</key>
				<string>8A3F1C2E-4B5D-4E6F-9A0B-1C2D3E4F5A6B</string>
				<key>Keywords</key>
				<array>
					<string>Shell</string>
					<string>Script</string>
				</array>
				<key>OutputUUID</key>
				<string>9B4E2D3F-5C6E-4F70-8B1C-2D3E4F5A6B7C</string>
				<key>UUID</key>
				<string>0C5F3E4A-6D7F-4081-9C2D-3E4F5A6B7C8D</string>
				<key>UnlocalizedApplications</key>
				<array>
					<string>Automator</string>
				</array>
				<key>arguments</key>
				<dict/>
				<key>isViewVisible</key>
				<integer>1</integer>
				<key>location</key>
				<string>309.000000:253.000000</string>
				<key>nibPath</key>
				<string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
			</dict>
			<key>isViewVisible</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>connectors</key>
	<dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key>
		<dict/>
		<key>applicationPaths</key>
		<array/>
		<key>inputTypeIdentifier</key>
		<string>{input}</string>
		<key>outputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>presentationMode</key>
		<integer>15</integer>
		<key>processesInput</key>
		<integer>0</integer>
		<key>serviceApplicationBundleID</key>
		<string>com.apple.finder</string>
		<key>serviceApplicationPath</key>
		<string>/System/Library/CoreServices/Finder.app</string>
		<key>serviceInputTypeIdentifier</key>
		<string>{input}</string>
		<key>serviceOutputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key>
		<integer>0</integer>
		<key>systemImageName</key>
		<string>NSActionTemplate</string>
		<key>useAutomaticInputType</key>
		<integer>0</integer>
		<key>workflowTypeIdentifier</key>
		<string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#,
        command = xml(&script(exe, s.action)),
        input = s.input_type,
    )
}

fn refresh_services() {
    // Make Finder pick up added/removed services without logging out.
    let pbs = "/System/Library/CoreServices/pbs";
    let _ = Command::new(pbs).arg("-flush").status();
    let _ = Command::new(pbs).arg("-update").status();
}

fn ours(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    name.starts_with(PREFIX) && name.ends_with(".workflow")
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn register(exe: &Path) -> io::Result<()> {
    let dir = services_dir()?;
    for old in ours(&dir) {
        fs::remove_dir_all(old)?;
    }
    for s in services() {
        let contents = dir.join(format!("{}.workflow", s.file)).join("Contents");
        fs::create_dir_all(&contents)?;
        fs::write(contents.join("Info.plist"), info_plist(&s))?;
        fs::write(contents.join("document.wflow"), document_wflow(&s, exe))?;
    }
    refresh_services();
    Ok(())
}

pub fn unregister() -> io::Result<()> {
    let dir = services_dir()?;
    for old in ours(&dir) {
        fs::remove_dir_all(old)?;
    }
    refresh_services();
    Ok(())
}

pub fn status(exe: Option<&Path>) -> Status {
    let wflow = services_dir()
        .ok()
        .map(|d| d.join(format!("{PREFIX}Compress.workflow/Contents/document.wflow")))
        .and_then(|p| fs::read_to_string(p).ok());
    let registered = wflow.is_some();
    let stale = match (wflow, exe) {
        (Some(w), Some(exe)) => !w.contains(&xml(&sh_quote(&exe.to_string_lossy()))),
        _ => false,
    };
    Status { supported: true, registered, stale, location: "Finder Quick Actions" }
}
