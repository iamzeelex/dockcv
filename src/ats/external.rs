//! The extraction stacks that are not ours, run over the same file.
//!
//! poppler, pdfminer.six and Apache PDFBox are what a large part of the market
//! reads a CV with — directly, or under a licensed parser. They are dev-time
//! tools: nothing here is compiled into DockCV, nothing is shipped, and the app
//! stays a self-contained binary that needs none of them installed.
//!
//! `scripts/ats-tools.sh` puts them in `target/ats-tools/`. When they are not
//! there, every reader in this module answers `None` and the harness says so in
//! its report rather than failing — a contributor without a Java runtime still
//! gets a green `cargo test`, and CI, which runs the script, still gets the
//! full table.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One external reading, named the way the report names it.
pub struct External {
    pub engine: &'static str,
    pub text: String,
}

/// Where `scripts/ats-tools.sh` installs what it installs.
fn tools_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ats-tools")
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every external engine that is installed, over one PDF on disk.
///
/// The three `pdftotext` modes are three different readers, not one with
/// options: the default sorts, `-raw` follows the content stream, and
/// `-layout` reconstructs columns. A CV that survives all three survives the
/// shell pipelines a great many small ATS are built out of.
pub fn read_all(pdf_path: &Path) -> Vec<External> {
    let path = pdf_path.to_string_lossy().to_string();
    let mut out = Vec::new();

    for (engine, flag) in [
        ("pdftotext", ""),
        ("pdftotext -raw", "-raw"),
        ("pdftotext -layout", "-layout"),
    ] {
        let mut args = vec![];
        if !flag.is_empty() {
            args.push(flag);
        }
        args.push(path.as_str());
        args.push("-");
        if let Some(text) = run("pdftotext", &args) {
            out.push(External { engine, text });
        }
    }

    let pdfminer = tools_dir().join("venv/bin/pdf2txt.py");
    if pdfminer.is_file() {
        if let Some(text) = run(&pdfminer.to_string_lossy(), &[path.as_str()]) {
            out.push(External {
                engine: "pdfminer.six",
                text,
            });
        }
    }

    if let Some(jar) = pdfbox_jar() {
        if let Some(text) = run(
            "java",
            &[
                "-jar",
                &jar.to_string_lossy(),
                "export:text",
                "-i",
                path.as_str(),
                "-o",
                "/dev/stdout",
            ],
        ) {
            out.push(External {
                engine: "PDFBox",
                text,
            });
        }
    }

    out
}

/// The pinned jar, whatever version the script last fetched.
fn pdfbox_jar() -> Option<PathBuf> {
    std::fs::read_dir(tools_dir())
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("pdfbox-app-"))
                .unwrap_or(false)
        })
}
