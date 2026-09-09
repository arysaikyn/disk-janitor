//! Syscall-level integration tests — SPEC Part B §7 'Syscall-level' + 'Scanner edge cases'.
//! macOS/APFS only, raw libc calls, tempdirs under /tmp — never real user data.
//! Rows mapped: RENAME_EXCL→EEXIST; hardlink nlink across rename; RENAME_NOFOLLOW_ANY→ELOOP;
//! dua-core walk file counts; st_dev reject in descend (mounted-volume-mid-tree row).

#![cfg(target_os = "macos")]

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use tempfile::tempdir;

/// RENAME_NOFOLLOW_ANY (0x10) is not exported by libc — xnu bsd/sys/unistd.h.
const RENAME_NOFOLLOW_ANY: libc::c_uint = 0x0000_0010;

fn cstr(p: &Path) -> CString {
    CString::new(p.as_os_str().as_bytes()).expect("tempdir paths contain no NUL")
}

/// SPEC §7 'Syscall-level' (dataless materialization-OFF; scan §3.1 responsibility
/// "never materialize dataless files"): setiopolicy_np with the dataless-OFF policy
/// must succeed on this kernel.
#[test]
fn setiopolicy_np_dataless_off_returns_zero() {
    let rc = disk_janitor::scan::platform::macos::set_dataless_off();
    assert_eq!(
        rc, 0,
        "setiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES=3, SCOPE_PROCESS=0, OFF=1) \
         must return 0"
    );
}

/// SPEC §7 'Syscall-level' — `RENAME_EXCL → EEXIST` when the target exists (the capability
/// scan advertises via `VolumeInfo.rename_excl`).
#[test]
fn renamex_excl_eexist_on_existing_target() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    std::fs::write(&from, b"a").unwrap();
    std::fs::write(&to, b"b").unwrap();

    let from_c = cstr(&from);
    let to_c = cstr(&to);
    let rc = unsafe { libc::renamex_np(from_c.as_ptr(), to_c.as_ptr(), libc::RENAME_EXCL) };
    assert_eq!(
        rc, -1,
        "renamex_np(RENAME_EXCL) onto an existing target must fail"
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EEXIST)
    );
    // Neither side may have been touched.
    assert_eq!(std::fs::read(&from).unwrap(), b"a");
    assert_eq!(std::fs::read(&to).unwrap(), b"b");
}

/// SPEC §7 'Syscall-level' — hardlink `nlink` preserved across rename (nlink is the
/// Hardlinked-evidence source: deleting one of N links frees 0 bytes).
#[test]
fn hardlink_nlink_preserved_across_rename() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    std::fs::write(&a, vec![0u8; 64]).unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    assert_eq!(std::fs::metadata(&a).unwrap().nlink(), 2);

    let renamed = dir.path().join("renamed.bin");
    std::fs::rename(&b, &renamed).unwrap();
    assert_eq!(
        std::fs::metadata(&a).unwrap().nlink(),
        2,
        "rename must preserve the link count"
    );
    assert_eq!(std::fs::metadata(&renamed).unwrap().nlink(), 2);
}

/// SPEC §7 'Syscall-level' — `RENAME_NOFOLLOW_ANY → ELOOP` when a path component is a
/// symlink. Kernels predating the flag (macOS < 10.15) return EINVAL — accepted as a skip.
#[test]
fn rename_nofollow_any_eloop_on_symlink_component() {
    let dir = tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::write(real.join("f.txt"), b"x").unwrap();
    // Intermediate path component is a symlink → NOFOLLOW_ANY must refuse the whole rename.
    std::os::unix::fs::symlink("real", dir.path().join("linkdir")).unwrap();

    let from = dir.path().join("linkdir").join("f.txt");
    let to = dir.path().join("out.txt");
    let from_c = cstr(&from);
    let to_c = cstr(&to);
    let rc = unsafe { libc::renamex_np(from_c.as_ptr(), to_c.as_ptr(), RENAME_NOFOLLOW_ANY) };
    assert_eq!(
        rc, -1,
        "renamex_np(RENAME_NOFOLLOW_ANY) through a symlink must fail"
    );
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ELOOP) => {}       // kernel supports the flag
        Some(libc::EINVAL) => return, // kernel predates the flag: skip
        other => panic!("expected ELOOP (or EINVAL skip), got {other:?}"),
    }
    // The rename must not have happened.
    assert!(real.join("f.txt").exists());
    assert!(!to.exists());
}

/// SPEC §7 'Scanner edge cases' — a mounted volume appearing mid-tree is rejected by the
/// `st_dev` check in `descend` (firmlink-safe mount skipping, scan §3.1). A real second
/// filesystem cannot be mounted in userspace, so the deterministic proxy is a symlink to an
/// outside directory: dua-core never follows symlinks, and the dev-checked descend must not
/// be asked to traverse anything off-root — assert the walk sees only the real files.
#[test]
fn walk_roots_st_dev_reject_does_not_escape_root() {
    let dir = tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
    std::fs::write(dir.path().join("sub").join("b.txt"), b"b").unwrap();

    let outside = tempdir().unwrap();
    std::fs::create_dir(outside.path().join("deep")).unwrap();
    std::fs::write(outside.path().join("deep").join("leak.txt"), b"x").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).unwrap();

    let root_dev = std::fs::metadata(dir.path()).unwrap().dev();
    let mut files = 0usize;
    for (_idx, ev) in dua_core::walk_roots(
        vec![(0usize, dir.path().to_path_buf())],
        2,
        dua_core::Order::ParentFirst,
        dua_core::Options {
            skip_metadata: false,
            apfs_clone_metadata: true,
        },
        // Same descend contract as scan::platform::macos::run: dirs only, same st_dev.
        move |_idx, e: &dua_core::Entry| {
            let Some(Ok(md)) = e.metadata.as_ref() else {
                return false;
            };
            e.file_type.is_dir() && md.dev() == root_dev
        },
    ) {
        if let dua_core::RootEvent::Entry(Ok(e)) = ev {
            if e.file_type.is_file() {
                files += 1;
                assert!(
                    !e.path().ends_with("leak.txt"),
                    "walk must not traverse through the symlink to the outside tree"
                );
            }
        }
    }
    assert_eq!(
        files, 2,
        "only the two real files under the root may be seen"
    );
}

/// SPEC §7 'Syscall-level' + 'Fixture trees' — dua-core `walk_roots` file/dir counts on a
/// synthetic tree (nested dirs, empty dir, symlink): symlinks are reported but never
/// followed, and every regular file is seen exactly once.
#[test]
fn walk_roots_counts_files_on_synthetic_tree() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("d1/d2")).unwrap();
    std::fs::write(dir.path().join("root.txt"), vec![0u8; 10]).unwrap();
    std::fs::write(dir.path().join("d1").join("m.txt"), vec![0u8; 100]).unwrap();
    std::fs::write(dir.path().join("d1/d2").join("n.txt"), vec![0u8; 1000]).unwrap();
    std::fs::create_dir(dir.path().join("empty")).unwrap();
    std::os::unix::fs::symlink("root.txt", dir.path().join("ln")).unwrap();

    let mut files = 0usize;
    let mut dirs = 0usize;
    let mut symlinks = 0usize;
    for (_idx, ev) in dua_core::walk_roots(
        vec![(0usize, dir.path().to_path_buf())],
        2,
        dua_core::Order::ParentFirst,
        dua_core::Options {
            skip_metadata: false,
            apfs_clone_metadata: true,
        },
        |_idx, _e| true,
    ) {
        if let dua_core::RootEvent::Entry(Ok(e)) = ev {
            if e.file_type.is_dir() {
                dirs += 1;
            } else if e.file_type.is_file() {
                files += 1;
            } else if e.file_type.is_symlink() {
                symlinks += 1;
            }
        }
    }
    assert_eq!(files, 3, "root.txt + m.txt + n.txt");
    assert_eq!(dirs, 4, "root + d1 + d2 + empty");
    assert_eq!(symlinks, 1, "ln is reported as a symlink entry");
}
