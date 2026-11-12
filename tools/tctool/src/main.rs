// No unsafe code, and none may be added (PLAN.md, phase 4).
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: tctool [options] <path-to-tc>

Converts a Liero installation (LIERO.EXE, LIERO.CHR, LIERO.SND) into a TC folder.

Options:
  --tc-name NAME      Name of the TC folder to write (default: name of <path-to-tc>)
  --config-root DIR   Folder that holds the TC folder (default: current folder)
  -h, --help          Show this help";

/// Exit code for bad arguments; 1 is used when the conversion fails.
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let mut config_root = PathBuf::new(); // Default to current dir
    let mut tc_name: Option<String> = None;
    let mut source: Option<PathBuf> = None;

    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h") | Some("--help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Some(opt @ ("--config-root" | "--tc-name")) => {
                let Some(value) = args.next() else {
                    eprintln!("tctool: {opt} needs a value");
                    return ExitCode::from(USAGE_ERROR);
                };
                if opt == "--config-root" {
                    config_root = PathBuf::from(value);
                } else {
                    tc_name = Some(value.to_string_lossy().into_owned());
                }
            }
            Some(opt) if opt.starts_with('-') => {
                eprintln!("tctool: unknown option {opt}\n\n{USAGE}");
                return ExitCode::from(USAGE_ERROR);
            }
            _ if source.is_some() => {
                eprintln!("tctool: more than one path given");
                return ExitCode::from(USAGE_ERROR);
            }
            _ => source = Some(PathBuf::from(arg)),
        }
    }

    let Some(source) = source else {
        eprintln!("{USAGE}");
        return ExitCode::from(USAGE_ERROR);
    };

    let Some(tc_name) = tc_name.or_else(|| leaf(&source)) else {
        eprintln!("tctool: can't work out a TC name from {}, use --tc-name", source.display());
        return ExitCode::from(USAGE_ERROR);
    };

    let dest = config_root.join("TC").join(tc_name);

    match liero_tctool::convert_tc(&source, &dest, |msg| println!("{msg}")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tctool: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The last component of `path`, also for paths like `dir/` or `.`.
fn leaf(path: &Path) -> Option<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}
