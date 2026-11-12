//! The hash run, natively: `liero-hashrun [--tree] [ticks] [interval]
//! [sections]`, run from the repository's root, where `TC/openliero` is. It
//! prints the lines `tools/hashrun/run.ts` compares with
//! `tools/hashrun/expected.txt`; `sections` is a comma-separated list of
//! those to run (`run.ts --only=`). With `--tree` the game's files are a
//! tree in memory, as the web build's will be (`liero_game::disk`), instead
//! of a folder on disk.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

/// Copies folder `from` to `to`, which doesn't exist yet.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// A folder of the run's own, as `/` is in the browser, with only the TC.
/// The sections write their files there, making the folders they need.
fn make_folder(root: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root.join("TC"))?;
    copy_dir(Path::new("TC/openliero"), &root.join("TC/openliero"))
}

/// Puts `TC/openliero` from the disk into the tree, at the same path.
fn load_tree(from: &Path, to: &str) -> std::io::Result<()> {
    use liero_game::disk;
    let made = |ok: bool| if ok { Ok(()) } else { Err(std::io::Error::other(format!("can't make {to} in the tree"))) };
    made(disk::create_dir(to.as_bytes()))?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = format!("{to}/{}", e.file_name().to_string_lossy());
        if e.file_type()?.is_dir() {
            load_tree(&e.path(), &name)?;
        } else {
            let data = std::fs::read(e.path())?;
            made(disk::write(name.as_bytes(), &data))?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let tree = args.iter().any(|a| a == "--tree");
    args.retain(|a| a != "--tree");
    let number = |i: usize, default: i32| args.get(i).map_or(Ok(default), |a| a.parse::<i32>().map_err(|_| format!("not a number: {a}")));
    let (ticks, interval) = match (number(0, 30000), number(1, 100)) {
        (Ok(t), Ok(i)) => (t, i),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let only: Vec<String> = args.get(2).map(|s| s.split(',').filter(|n| !n.is_empty()).map(str::to_owned).collect()).unwrap_or_default();

    // The run's own folder, as `/` is in the browser: a tree, or a
    // temporary folder on disk.
    let root = (!tree).then(|| std::env::temp_dir().join(format!("liero-hashrun-{}", std::process::id())));
    let set_up = match &root {
        None => {
            liero_game::disk::use_tree();
            liero_game::disk::create_dir(b"TC");
            load_tree(Path::new("TC/openliero"), "TC/openliero")
        }
        Some(root) => make_folder(root).and_then(|()| std::env::set_current_dir(root)),
    };
    if let Err(e) = set_up {
        eprintln!("can't set up the run's folder with TC/openliero from the current folder: {e}");
        if let Some(root) = &root {
            let _ = std::fs::remove_dir_all(root);
        }
        return ExitCode::FAILURE;
    }

    let stdout = std::io::stdout();
    let mut w = std::io::BufWriter::new(stdout.lock());
    let r = liero_game::hashrun::hash_run(ticks, interval, &only, &mut |line| {
        writeln!(w, "{line}").expect("can't write the output");
    });
    w.flush().expect("can't write the output");
    if let Some(root) = &root {
        let _ = std::fs::remove_dir_all(root);
    }
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
