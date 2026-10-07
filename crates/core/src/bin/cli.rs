//! Command-line front end to the same engine the app uses. Handy for
//! scripting and for checking LocalPDF's behaviour without the GUI.
//!
//!   localpdf-cli compress a.pdf
//!   localpdf-cli convert --to docx a.pdf
//!   localpdf-cli split --mode ranges --pages "1-3,4-" a.pdf
//!   localpdf-cli rotate --angle 270 --pages 2 a.pdf
//!   LOCALPDF_PASSWORD=secret localpdf-cli protect a.pdf

use std::path::PathBuf;
use std::process::ExitCode;

use localpdf_core::{Action, CompressLevel, ConvertTarget, Options, SplitMode};

fn usage() -> ExitCode {
    eprintln!(
        "usage: localpdf-cli <convert|compress|merge|split|rotate|unlock|protect> [options] <files…>\n\
         options: --to png|jpeg|docx|txt|pdf  --dpi N  --separate\n\
         \x20        --mode extract|ranges|each  --pages SPEC  --angle 90|180|270\n\
         \x20        --level light|balanced|strong  --password PW (or LOCALPDF_PASSWORD)"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(action) = args.next().as_deref().and_then(Action::parse) else { return usage() };
    let mut opts = Options { password: std::env::var("LOCALPDF_PASSWORD").ok(), ..Default::default() };
    let mut files = Vec::new();
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_default();
        match a.as_str() {
            "--to" => {
                opts.target = Some(match val().as_str() {
                    "png" => ConvertTarget::Png,
                    "jpg" | "jpeg" => ConvertTarget::Jpeg,
                    "docx" | "word" => ConvertTarget::Docx,
                    "txt" | "text" => ConvertTarget::Txt,
                    "pdf" => ConvertTarget::Pdf,
                    _ => return usage(),
                })
            }
            "--dpi" => opts.dpi = val().parse().ok(),
            "--separate" => opts.combine = Some(false),
            "--mode" => {
                opts.split_mode = Some(match val().as_str() {
                    "extract" => SplitMode::Extract,
                    "ranges" => SplitMode::Ranges,
                    "each" => SplitMode::Each,
                    _ => return usage(),
                })
            }
            "--pages" => opts.ranges = Some(val()),
            "--angle" => opts.angle = val().parse().ok(),
            "--password" => opts.password = Some(val()),
            "--level" => {
                opts.level = Some(match val().as_str() {
                    "light" => CompressLevel::Light,
                    "balanced" => CompressLevel::Balanced,
                    "strong" => CompressLevel::Strong,
                    _ => return usage(),
                })
            }
            s if s.starts_with("--") => return usage(),
            _ => files.push(PathBuf::from(a)),
        }
    }
    let plan = match localpdf_core::plan(action, &files) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let report = localpdf_core::run(&plan, &opts, &mut |p| {
        if !p.label.is_empty() {
            eprintln!("[{}/{}] {}", p.done + 1, p.total, p.label);
        }
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if report.failures.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
