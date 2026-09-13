//! macOS syscall substrate for `execute/` (SPEC Part B §3.4, §5): `renamex_np`
//! quarantine moves, `renameatx_np` restore renames, `chflags UF_HIDDEN`,
//! lstat flag/device probes, dir-fd syncs, symlink-safe recursive unlink.
//!
//! Constants libc 0.2.189 lacks are defined locally (SPEC [05 §reusable]):
//! `RENAME_NOFOLLOW_ANY = 0x10`; the `UF_*`/`SF_*` `st_flags` bits come from the
//! canonical table in `lib.rs` and are re-exported at the bottom of the constants
//! block, cross-checkable against `<sys/stat.h>`.

use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

// --- constants ---
//
// `RENAME_EXCL`/`RENAME_NOFOLLOW_ANY` are `<sys/stdio.h>` *rename* flags, not `st_flags`
// bits, and libc 0.2.189 lacks `RENAME_NOFOLLOW_ANY`; they stay local to the module that
// issues the syscall. The `UF_*`/`SF_*` `st_flags` bits live in ONE place — the canonical
// table in `lib.rs` — and are re-exported below so `macos::UF_HIDDEN` keeps working.
pub(crate) const RENAME_EXCL: u32 = 0x0000_0004;
pub(crate) const RENAME_NOFOLLOW_ANY: u32 = 0x0000_0010;

pub(crate) use crate::{SF_DATALESS, SF_NOUNLINK, SF_RESTRICTED, UF_HIDDEN, UF_IMMUTABLE};

/// Per-volume quarantine root name — `<mnt>/.Trash-janitor/`, mkdir 0700 + UF_HIDDEN.
pub(crate) const QUARANTINE_ROOT_NAME: &str = ".Trash-janitor";

fn cstr(p: &Path) -> io::Result<CString> {
    CString::new(p.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL byte"))
}

fn lstat(p: &Path) -> io::Result<libc::stat> {
    let c = cstr(p)?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::lstat(c.as_ptr(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(st)
}

/// Resolve intermediate symlinks in `p` so `RENAME_NOFOLLOW_ANY` never trips
/// ELOOP on ordinary paths: on macOS `/var`, `/tmp` etc. are symlinks, and the
/// flag rejects a symlink at ANY point in the path. The FINAL component is
/// preserved as-is — a symlink entry still moves as a symlink (SPEC [05 §7]).
fn canonical_parent(p: &Path) -> io::Result<PathBuf> {
    let name = p.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no final component")
    })?;
    let parent = p.parent().unwrap_or(Path::new("/"));
    let resolved = parent.canonicalize()?;
    Ok(resolved.join(name))
}

/// Quarantine move: `renamex_np(from, to, RENAME_EXCL | RENAME_NOFOLLOW_ANY)`.
/// The syscall IS the existence check — `EEXIST` on collision, no TOCTOU window
/// vs a probe-then-rename pair. `RENAME_NOFOLLOW_ANY` moves a symlink itself.
pub(crate) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let from = canonical_parent(from)?;
    let to = canonical_parent(to)?;
    let from = cstr(&from)?;
    let to = cstr(&to)?;
    let rc = unsafe {
        libc::renamex_np(
            from.as_ptr(),
            to.as_ptr(),
            RENAME_EXCL | RENAME_NOFOLLOW_ANY,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Set UF_HIDDEN on `p` in addition to its existing flags (never clears flags).
pub(crate) fn set_hidden(p: &Path) -> io::Result<()> {
    let st = lstat(p)?;
    let flags = st.st_flags | UF_HIDDEN;
    let c = cstr(p)?;
    if unsafe { libc::chflags(c.as_ptr(), flags) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// lstat `st_flags` probe — TOCTOU armor at apply time.
pub(crate) fn lstat_flags(p: &Path) -> io::Result<u32> {
    Ok(lstat(p)?.st_flags as u32)
}

/// lstat `(st_dev, st_ino)` identity probe — the filesystem-truthful identity of a
/// directory, used to reconcile two path *spellings* of one directory. APFS
/// firmlinks make `/private/tmp` and `/System/Volumes/Data/private/tmp` the same
/// inode reached by different strings, which `canonicalize()` cannot see (it
/// resolves symlinks only). Two paths denote one directory iff this pair matches.
pub(crate) fn lstat_identity(p: &Path) -> io::Result<(u64, u64)> {
    let st = lstat(p)?;
    Ok((st.st_dev as u64, st.st_ino as u64))
}

/// lstat `st_dev` probe — same-volume check at apply time (EXDEV armor).
pub(crate) fn lstat_dev(p: &Path) -> io::Result<u64> {
    Ok(lstat(p)?.st_dev as u64)
}

/// Dir-fd sync — open `O_RDONLY|O_DIRECTORY` and `sync_all()`; APFS reorders two
/// independent fullfsyncs, so the parent dir sync closes the op-group boundary
/// (SPEC §5.2 step 4, [08 §1]). The ledger `Store::sync_boundary()` wraps this
/// same primitive (op-group syncs go through it); kept here as the substrate.
#[allow(dead_code)]
pub(crate) fn sync_dir(p: &Path) -> io::Result<()> {
    fs::File::open(p)?.sync_all()
}

/// Recursive unlink for purge — never follows symlinks at any level.
pub(crate) fn remove_all(path: &Path) -> io::Result<()> {
    let md = fs::symlink_metadata(path)?;
    if md.file_type().is_symlink() || md.is_file() {
        return fs::remove_file(path);
    }
    for ent in fs::read_dir(path)? {
        remove_all(&ent?.path())?;
    }
    fs::remove_dir(path)
}
