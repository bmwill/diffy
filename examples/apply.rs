//! A minimal patch-apply tool using diffy's multi-file patch support.
//!
//! Usage:
//!
//! ```console
//! apply <patch-file> [target-dir]
//! ```
//!
//! Applies a git-format patch file to a target directory
//! (defaults to the current directory).
//!
//! Assumes the default `a/` and `b/` path prefixes
//! from `git diff` and GNU `diff -u`.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use diffy::apply_bytes;
use diffy::binary::BinaryPatch;
use diffy::patch_set::FileOperation;
use diffy::patch_set::ParseOptions;
use diffy::patch_set::PatchKind;
use diffy::patch_set::PatchSet;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: {} <patch-file> [target-dir]", args[0]);
        return ExitCode::FAILURE;
    }
    let patch_file = Path::new(&args[1]);
    let target_dir = args.get(2).map_or_else(|| Path::new("."), |p| Path::new(p));

    if let Err(e) = apply_patch_file(patch_file, target_dir) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn apply_patch_file(patch_file: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read(patch_file)?;

    let patches = PatchSet::parse_bytes(&content, ParseOptions::gitdiff());

    for file_patch in patches {
        let file_patch = file_patch?;
        let operation = {
            let op = file_patch.operation();
            // Rename/Copy paths come from git headers without a/b prefix.
            let strip = match op {
                FileOperation::Rename { .. } | FileOperation::Copy { .. } => 0,
                _ => 1,
            };
            op.strip_prefix(strip)
        };

        match operation {
            FileOperation::Create(path) => {
                let target = dst.join(path_from_bytes(&path)?);
                let patched = match file_patch.patch() {
                    PatchKind::Text(patch) => apply_bytes(&[], patch)?,
                    PatchKind::Binary(BinaryPatch::Marker) => continue,
                    PatchKind::Binary(patch) => patch.apply(&[])?,
                };
                create_parent_dirs(&target)?;
                fs::write(&target, patched)?;
                eprintln!("create {}", target.display());
            }
            FileOperation::Delete(path) => {
                let target = dst.join(path_from_bytes(&path)?);
                fs::remove_file(&target)?;
                eprintln!("delete {}", target.display());
            }
            FileOperation::Modify { original, modified } => {
                let src_path = dst.join(path_from_bytes(&original)?);
                let dst_path = dst.join(path_from_bytes(&modified)?);
                let patched = match file_patch.patch() {
                    PatchKind::Text(patch) => {
                        let base = fs::read(&src_path)?;
                        apply_bytes(&base, patch)?
                    }
                    PatchKind::Binary(BinaryPatch::Marker) => continue,
                    PatchKind::Binary(patch) => {
                        let base = fs::read(&src_path)?;
                        patch.apply(&base)?
                    }
                };
                create_parent_dirs(&dst_path)?;
                fs::write(&dst_path, patched)?;
                if src_path != dst_path {
                    fs::remove_file(&src_path)?;
                    eprintln!("rename {} -> {}", src_path.display(), dst_path.display());
                } else {
                    eprintln!("modify {}", dst_path.display());
                }
            }
            FileOperation::Rename { from, to } => {
                let src_path = dst.join(path_from_bytes(&from)?);
                let dst_path = dst.join(path_from_bytes(&to)?);
                create_parent_dirs(&dst_path)?;
                fs::rename(&src_path, &dst_path)?;
                eprintln!("rename {} -> {}", src_path.display(), dst_path.display());
            }
            FileOperation::Copy { from, to } => {
                let src_path = dst.join(path_from_bytes(&from)?);
                let dst_path = dst.join(path_from_bytes(&to)?);
                create_parent_dirs(&dst_path)?;
                fs::copy(&src_path, &dst_path)?;
                eprintln!("copy {} -> {}", src_path.display(), dst_path.display());
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Result<&Path, Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStrExt;
    Ok(Path::new(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Result<&Path, Box<dyn std::error::Error>> {
    Ok(Path::new(std::str::from_utf8(bytes)?))
}

fn create_parent_dirs(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
