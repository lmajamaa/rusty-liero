//! Converts a Liero installation (LIERO.EXE, LIERO.CHR, LIERO.SND) into an
//! OpenLiero TC folder. Rust port of the C++ tctool in `src/tc_tool`.

mod exe;
mod model;
mod rand;
mod reader;
mod save;
mod toml;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

/// The Liero 1.33 executable and the TCs based on it are this size.
const EXE_SIZES: std::ops::RangeInclusive<u64> = 135000..=137000;

/// Finds the Liero executable in `source_dir`, reads it together with LIERO.CHR and
/// LIERO.SND, and writes the converted TC to `dest_dir`. Progress messages go to `log`.
pub fn convert_tc(source_dir: &Path, dest_dir: &Path, log: impl Fn(&str)) -> Result<(), Error> {
    if !source_dir.is_dir() {
        return Err(Error(format!("Directory not found: {}", source_dir.display())));
    }

    let exe_path = find_exe(source_dir)?;

    // TODO: Some TCs change the name of the .SND or .CHR for some reason.
    // We could read that name from the exe to make them work.
    let chr_path = find_file(source_dir, "LIERO.CHR")?;
    let snd_path = find_file(source_dir, "LIERO.SND")?;

    log(&format!("Converting {}...", file_name(&exe_path)));

    let common = exe::load(&read(&exe_path)?, &read(&chr_path)?, &read(&snd_path)?)?;

    log(&format!("Writing to {}...", dest_dir.display()));

    save::save(&common, dest_dir)
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|e| Error(format!("Could not read {}: {}", path.display(), e)))
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Files in `dir`, sorted by name so the result doesn't depend on the file system.
fn files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| Error(format!("Could not list {}: {}", dir.display(), e)))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    Ok(paths)
}

/// The first file whose name contains ".EXE" and whose size fits Liero 1.33.
fn find_exe(dir: &Path) -> Result<PathBuf, Error> {
    for path in files(dir)? {
        if !file_name(&path).to_uppercase().contains(".EXE") {
            continue;
        }
        if fs::metadata(&path).map(|m| EXE_SIZES.contains(&m.len())).unwrap_or(false) {
            return Ok(path);
        }
    }
    Err(Error(format!("Could not find a suitable LIERO.EXE in {}", dir.display())))
}

/// Looks up `name` in `dir`, ignoring case.
fn find_file(dir: &Path, name: &str) -> Result<PathBuf, Error> {
    files(dir)?
        .into_iter()
        .find(|path| file_name(path).eq_ignore_ascii_case(name))
        .ok_or_else(|| Error(format!("Missing {} in {}", name, dir.display())))
}
