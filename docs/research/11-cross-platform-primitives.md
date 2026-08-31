# Cross-Platform FS Primitives (macOS · Windows · Linux) — Research Note

Per-platform filesystem primitives and crates for Disk Janitor's 3-platform design:
atomic no-clobber move (quarantine), parallel walker, trash, durability, paths.
macOS baseline = SPEC's `renamex_np(RENAME_EXCL)`; this note documents what Windows/Linux
change, add, or forbid. · researched 2026-08-30

## Findings

### 1. Atomic move per platform (quarantine + WAL/restore)

**Baseline (macOS, unchanged from SPEC):** `renamex_np(RENAME_EXCL)` — same-volume,
no-clobber, atomic. This note uses it as the semantics gold standard.

**Windows.** Two API families, availability differs:

| API | Mechanism | No-clobber? | Replace-with-open-handles? | Min OS |
|---|---|---|---|---|
| `MoveFileExW` | kernel32; flags `MOVEFILE_REPLACE_EXISTING` (0x1), `MOVEFILE_WRITE_THROUGH` (0x8), `MOVEFILE_COPY_ALLOWED` (0x2) | yes — omit REPLACE (fails `if lpNewFileName exists`… reports error for dir) | NO — replacement "replaces its contents", fails on open/read-only targets; classic race window | Windows XP [S2] |
| `SetFileInformationByHandle` + `FileRenameInfo` (class 3) | `FILE_RENAME_INFO` union, `ReplaceIfExists: BOOLEAN` | yes (ReplaceIfExists=FALSE) | no | Windows Vista [S3] |
| `SetFileInformationByHandle` + `FileRenameInfoEx` (class 65) | `FILE_RENAME_INFO_0.Flags`: `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` (0x1), `FILE_RENAME_FLAG_POSIX_SEMANTICS` (0x2), `FILE_RENAME_FLAG_IGNORE_READONLY_ATTRIBUTE` (0x40) | yes — omit REPLACE_IF_EXISTS ⇒ "rename MUST fail if target exists" | yes w/ POSIX_SEMANTICS: "existing handles to the replaced file continue to be valid; subsequent opens of the target name will open the renamed file" | FileRenameInfoEx appears in Win10 1607 (RS1) headers [S4][S5]; POSIX_SEMANTICS usable per rust-lang#123985 "As of Windows 10 1709 and later" [S6] |

Flags are specified identically in the [MS-FSCC] protocol spec and ntifs.h [S1][S7].
`FILE_RENAME_FLAG_NO_REPLACE` **does not exist** — no-clobber = *omit*
`REPLACE_IF_EXISTS` (MS-FSCC: "If not set … the rename operation MUST fail if a file
with the given name already exists") [S1]. `MOVEFILE_CREATE_HARDLINK` (0x10) is
"Reserved for future use" [S2].

What Disk Janitor should do (achievable guarantee):
- **apply/quarantine (no-clobber):** open source with `DELETE` access
  (`FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`), then
  `SetFileInformationByHandle(FileRenameInfoEx, Flags=0)` — atomic, fails if target
  exists, no TOCTOU. Same volume only (else `ERROR_NOT_SAME_DEVICE`).
- **restore/overwrite:** on Win10 1709+: `REPLACE_IF_EXISTS | POSIX_SEMANTICS`
  (atomic replace, old handles stay valid). Pre-1709 fallback: `MoveFileExW
  (MOVEFILE_REPLACE_EXISTING)` — weaker: not atomic w.r.t. open targets
  (ERROR_ACCESS_DENIED / sharing violations). This exactly mirrors what Rust std
  does: `fs::rename` = `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, and on
  `ERROR_ACCESS_DENIED` re-tries via `FileRenameInfoEx` with
  `REPLACE_IF_EXISTS | POSIX_SEMANTICS` [S8][S9]. std documents: "On Windows 10
  version 1607 and above, the behavior is the same as Unix if the filesystem
  supports `FileRenameInfoEx`" [S10].
- **Rust bindings:** `windows` crate exposes `FileRenameInfoEx`
  (`FILE_INFO_BY_HANDLE_CLASS`), `FILE_RENAME_INFO` + `FILE_RENAME_INFO_0` union
  (`ReplaceIfExists: bool | Flags: u32`) in `Win32::Storage::FileSystem` [S11];
  std uses windows-sys constants `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` /
  `FILE_RENAME_FLAG_POSIX_SEMANTICS` [S8]. MSVCRT's own `std::filesystem::rename`
  converged on the same flags (STL PR #2062; notes
  `IGNORE_READONLY_ATTRIBUTE` ≈ Win10 1809+) [S5].

**Linux.** `renameat2(2)`: Linux 3.15, glibc 2.28 wrapper (needs `_GNU_SOURCE`;
on older kernels than the syscall, no glibc fallback for the *flags* variant —
glibc only falls back for `renameat`) [S12].
- Flags: `RENAME_NOREPLACE` ("Don't overwrite newpath … error if newpath already
  exists"), `RENAME_EXCHANGE`, `RENAME_WHITEOUT`. No-clobber quarantine move =
  `renameat2(AT_FDCWD, src, AT_FDCWD, dst, RENAME_NOREPLACE)`; `EEXIST` on target.
- **Filesystem support matrix for RENAME_NOREPLACE** (man rename(2)) [S12]:
  ext4 (3.15); btrfs, tmpfs, cifs (3.17); xfs (4.0); "many other filesystems" in
  4.9 incl. ext2, minix, reiserfs, jfs, vfat, bpf. **NFS: unsupported** — kernel
  `nfs_rename()` returns `-EINVAL` for *any* flags [S13]; man BUGS also warns on
  NFS "you can not assume that if the operation failed, the file was not renamed"
  [S12]. ⇒ policy: probe per-volume (ENOENT on first NOREPLACE op or
  `/proc/self/mountinfo` fstype check) and fall back to `O_TMPFILE` staging or
  unique-name-then-rename (non-atomic no-clobber) on NFS.
- Unsupported-filesystem fallback that preserves *atomicity* on any fs:
  `O_TMPFILE` (Linux 3.11+) + `linkat(fd, "", AT_FDCWD, dst, AT_EMPTY_PATH)`
  (or `/proc/self/fd/N` + `AT_SYMLINK_FOLLOW` without `CAP_DAC_READ_SEARCH`).
  fs support: initially ext2/3/4, UDF, Minix, tmpfs; XFS 3.15; Btrfs/F2FS 3.16;
  ubifs 4.9 [S14].
- **Rust bindings:** `libc` 0.2.189 ships `RENAME_NOREPLACE=1`,
  `RENAME_EXCHANGE=2`, `RENAME_WHITEOUT=4` constants and the `renameat2` fn
  (gnu module) [S15]; `rustix` 1.1.4 exposes `fs::renameat_with(olddirfd, old,
  newdirfd, new, flags: RenameFlags)` — thin, `libc`-free syscall layer [S16].
  Raw `syscall(SYS_renameat2, …)` only needed for glibc < 2.28 (musl: wrapper
  since 1.1.24, not re-verified here).

**macOS delta for this section:** none — `renamex_np(RENAME_EXCL)` already gives
the strongest of the three; Windows 1709 POSIX-rename and Linux `RENAME_NOREPLACE`
are its semantic equivalents for replace/no-clobber respectively.

### 2. Durability (WAL survives crash) per platform

| Platform | Data flush | Dir-entry flush | Equivalent guarantee to macOS `F_FULLFSYNC`? |
|---|---|---|---|
| macOS | `fcntl(F_FULLFSYNC)` (spec baseline) | fsync dir fd | — (baseline) |
| Windows | `FlushFileBuffers(hFile)` — needs `GENERIC_WRITE` handle; "writes all the buffered information for a specified file to the device" [S17]. Per-open alternative: `FILE_FLAG_WRITE_THROUGH` (data goes to disk per-write; "File system metadata is always cached. Therefore, to store any metadata changes to disk, the file must either be flushed or be opened with FILE_FLAG_WRITE_THROUGH") [S18]. Volume-wide `FlushFileBuffers` on a volume handle = admin-only [S17] | **no directory-fsync API**; metadata durability relies on FlushFileBuffers on the file + NTFS journaling | weaker than F_FULLFSYNC: does not force-drive-cache flush the way F_FULLFSYNC requests it; documented behavior is cache→device write, not platter barrier |
| Linux | `fsync(fd)`; `fdatasync` for data-only; `syncfs(fd)` flushes the whole filesystem containing fd ("Linux waits for I/O completions, and thus sync() or syncfs() provide the same guarantees as fsync() called on every file in the … filesystem") — Linux 2.6.39, glibc 2.14; error reporting for failed inodes only since Linux 5.8 [S19] | **fsync(dirfd) is required**: "Calling fsync() does not necessarily ensure that the entry in the directory containing the file has also reached disk. For that an explicit fsync() on a file descriptor for the directory is also needed" [S20] | yes when fsync(file)+fsync(dir) both done (POSIX-grade; F_FULLFSYNC is still stricter vs drive cache) |

WAL protocol mapping: write WAL record → `fsync`(WAL file) [+ `fsync(dirfd)` of WAL dir
on Linux; `FlushFileBuffers` on Windows] → perform quarantine move → new WAL record
marking completion. Restore is then always possible after power loss.

### 3. Dataless files: placeholders per platform (scan must not trigger hydration)

| Platform | Mechanism | Detection | Danger |
|---|---|---|---|
| macOS | iCloud dataless files (baseline; SPEC §7) | `NSURLUbiquitousItemDownloadingStatusKey`, `st_flags` (`UF_COMPRESSED`-adjacent; APFS) | read triggers download |
| Windows | OneDrive / cldflt **placeholders**: cloud files API since Windows 10 1709 [S21]. Attribute `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (0x00400000): "the file or directory is not fully present locally … Reading the file / enumerating the directory will be more expensive than normal … Only kernel-mode callers can set this bit" [S22]. Sibling `FILE_ATTRIBUTE_RECALL_ON_OPEN` (0x00040000) appears only in directory enumerations [S22]; `FILE_ATTRIBUTE_OFFLINE`/`PINNED`/`UNPINNED` round out the hierarchy-storage family [S22] | read `dwFileAttributes` from `FindFirstFileW`/`WIN32_FIND_DATAW` (walker already has it) or `GetFileInformationByHandleEx(FileBasicInfo)`; open with `FILE_FLAG_OPEN_NO_RECALL` to *not* recall from remote storage [S23] | reading content hydrates (downloads); byte-count must use on-disk size, not logical size; don't copy placeholders |
| Linux | no OS-level dataless files (NFS "silly" / autofs / overlayfs are the nearest analogues; fanotify-based offline fs out of scope) | fstype from `/proc/self/mountinfo` | none of the OneDrive/iCloud kind |

Windows classification rule: `attributes & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`
⇒ treat like macOS iCloud placeholder: evidence-only scan (size from metadata),
grade stays computable, never open the file, never include hydrated-size in purge math.

### 4. Same-volume detection + hardlink identity

- **Cross-platform crate:** `same-file` 1.0.6 (Unlicense/MIT): `is_same_file` and
  reusable `Handle`. Unix backend compares `(st_dev, st_ino)` [S24]; Windows
  backend uses `GetFileInformationByHandleEx(FileIdInfo)` →
  `(volume_serial_number, file_index)` with a documented fallback because file
  index alone can alias after delete+recreate [S24]. Use `Handle`-equality for
  quarantine-volume check instead of raw `st_dev` — works on Windows paths incl. UNC.
- **Raw stat path:** Rust `std::os::unix::fs::MetadataExt` (`st_dev`, `st_ino`,
  `st_nlink`, `st_blocks`) is portable across Unix incl. Linux [S25]; Windows
  equivalent is `FILE_ID_INFO` via windows-rs, or
  `BY_HANDLE_FILE_INFORMATION` (`dwVolumeSerialNumber`, `nFileIndexLow/High`).
- **NTFS specifics:** hard links via `CreateHardLinkA` — "more than one path
  references a single file in the same volume … can't reference directories …
  can't reference files on different volumes" [S26] (same math as APFS/HFS
  hardlinks: freeing one link frees 0 bytes ⇒ ledger `st_nlink`/nFileIndex logic
  carries over). Junctions = directory reparse points, may cross local volumes
  [S26]; walker must not descend junctions by default (cycles / cross-volume).
- **Alternate data streams (ADS):** NTFS streams named `file:stream:$DATA`
  (CreateFile "To create a file stream, specify the name of the file, a colon,
  and then the name of the stream") [S23]. Consequences: (a) file size ≠ sum of
  data — Zone.Identifier etc. live in ADS; (b) "CreateFile with supersede
  disposition will fail if performed on a file where there is already an open
  alternate data stream" [S23] — relevant to overwrite-style restore; (c)
  enumerate with `FindFirstStreamW`/`GetFileInformationByHandleEx(FileStreamInfo)`.
- **Linux same-device:** quarantine dir must resolve to same mount; `st_dev`
  comparison suffices for bind mounts too; `/proc/self/mountinfo` field (3)
  "major:minor: the value of st_dev for files on this filesystem" + field (9)
  fstype lets the classifier map a path → (device, fstype) to decide
  RENAME_NOREPLACE availability per volume [S27].

### 5. Walker portability

| Walker | Platforms | Notes |
|---|---|---|
| `ignore` 0.4.33 (Unlicense OR MIT) | all | `WalkParallel` + `WalkBuilder`: `threads`, `follow_links`, `hidden`, `same_file_system`, `git_ignore`/`require_git`/`overrides`, `filter_entry` [S28]. Gitignore semantics are a *feature* for dev-junk (skip `.git`, vendored dirs) but must be disabled for honest byte accounting. |
| `jwalk` 0.9.0 (MIT) | all (cross-platform; `std::fs::read_dir` under the hood, rayon pool) — the "macOS-only" reputation is stale: last release 2026-08-05 [S29] | fastest generic option pre-2026; no per-OS bulk metadata |
| **dua-core 3.3.0** (MIT) | **per-OS native backends since dua-cli 2.41–2.43 (2026-08)**: macOS `getattrlistbulk(2)` (incl. APFS clone-id accounting, firmlink flag), Windows `GetFileInformationByHandleEx(FileIdBothDirInfo)` bulk enumeration, Unix fallback = std [S30][S31]. Changelog: 2.41.0 (2026-08-03) "30x and more performance on Windows"; 2.42.0 "~30% better traversal performance on macOS due to the usage of bulk-metadata APIs"; 2.42.1 bulk-reading on macOS+Windows [S31] | best evidence that a 3-platform metadata-first walker is tractable in-repo; borrows design more than code (workspace-coupled) |

Decision: `ignore::WalkParallel` as the portable engine + copy dua-core's per-OS
`getattrlistbulk` / `FileIdBothDirInfo` enumeration as our `scan/` fast path,
keeping `jwalk` unnecessary (dua-core superseded it in-tree).

### 6. Trash semantics per platform

| Platform | Backend (trash 5.2.6) | Restore story | Gotchas for Disk Janitor |
|---|---|---|---|
| macOS | `NSFileManager trashItemAtURL:` (method `NsFileManager`) or Finder osascript (default; plays sound, enables "Put Back"; needs permissions) — selectable via `TrashContext::set_delete_method` [S32] | Finder "Put Back" only via Finder method | NSFileManager method has the macOS "Put Back"-missing bug (upstream links sindresorhus/macos-trash#4, trash-rs#14) [S32]; our own quarantine+restore is still the product, OS trash is a secondary exit |
| Windows | `IFileOperation` COM (`CoCreateInstance(FileOperation)`), `FOF_NO_UI \| FOF_ALLOWUNDO \| FOF_WANTNUKEWARNING` [S33] | Recycle Bin restore, original path preserved by shell | COM apartment: feature flags `coinit_apartmentthreaded` (default) / `coinit_multithreaded` / `coinit_speed_over_memory` [S34]; `FOFX_EARLYFAILURE` used for strict purge ops [S33] |
| Linux | FreeDesktop Trash spec 1.0: home trash `~/.local/share/Trash` + per-mount `.Trash-$UID`/`.Trash`; `.trashinfo` files record original path + deletion date; crate "blindly assumes the distribution follows this specification" [S35] | `os_limited::restore_all` replays `trashinfo` via `fs::rename` [S35] | Cross-topdir moves fall back to copy+delete (non-atomic) — crate does `rename` first, on `CrossesDevices` "falling back to copy+delete" [S35]; `os_limited::{list, purge_all, restore_all, trash_folders, metadata}` exist on Win+Linux only, macOS backend is delete-only [S36]; upstream documents a potential-UB note around non-threadsafe `getmntent` on Linux/FreeBSD (internally Mutex-protected) [S36] |

Platform delta: on Linux/Windows the OS trash can serve as the 72 h-grace
surface *if* volume-local; FreeDesktop copy+delete fallback violates the
zero-bytes-freed invariant ⇒ prefer our own quarantine dir and use trash only
for `purge` (typed-confirm) handoff to the shell.

### 7. Paths (XDG/Known Folders) & std portability gaps

- `dirs` 6.0.0 (MIT OR Apache-2.0) maps per-OS conventions from one API [S37]:
  Linux `cache_dir` = `$XDG_CACHE_HOME` or `$HOME/.cache`; Windows =
  `{FOLDERID_LocalAppData}`; macOS = `$HOME/Library/Caches`. `config_dir`:
  `$XDG_CONFIG_HOME|~/.config` / `{FOLDERID_RoamingAppData}` /
  `~/Library/Application Support`. Linux-only `executable_dir`, `runtime_dir`
  return `None` on macOS+Windows [S37] — the classify/ rule tables must treat
  per-OS cache roots as data, not hardcode `~/Library/Caches`.
- **std gaps found in source:** `std::os::darwin::fs::MetadataExt` is the only
  place `st_flags`/`st_gen`/`st_birthtime` exist (`#[cfg(target_os = "macos")]`
  within darwin module) [S38] — BSD user file flags (uchg, hidden) have **no
  std API on Linux/Windows**; gate macOS-only flag logic with
  `#[cfg(target_os = "macos")]` + libc `chflags`/`stat` on other BSDs, and use
  `FILE_ATTRIBUTE_*` on Windows. `st_birthtime` analog on Windows =
  `FILE_BASIC_INFO.CreationTime` (std `Metadata::created()` is per-platform
  best-effort); Linux ext4 has birth time only via `statx(2)` (rustix
  `StatxFlags::BTIME`) [S39].
- **cfg-gating idioms observed in the wild (copy these):**
  - trash-rs: `#[cfg(target_os = "windows")] #[path = "windows.rs"] mod platform;`
    + unix-freedesktop + macos triple — one `platform` facade per OS [S36].
  - dua-core: `#[cfg(any(not(any(windows, target_os = "macos")), test))]` std
    fallback with per-OS `mod macos; mod windows;` overrides [S31].
  - xattr 1.6.1: `platforms!` macro mapping `target_os` lists to modules with
    `SUPPORTED_PLATFORM` const + optional compile-fail feature [S40].
  - filetime 0.2.29: per-OS `src/unix/macos.rs` (getattrlist fast path,
    utimensat fallback) [S41].
- **xattr per-OS reality:** `xattr` crate covers android/linux/macos/hurd
  (`linux_macos.rs`: `getxattr`/`fsetxattr`) + freebsd/netbsd; **Windows is
  compile-ok-but-unsupported** (`SUPPORTED_PLATFORM=false`, io::Error at
  runtime) [S40] — NTFS EAs/ADS need windows-rs, out of xattr's scope. Linux
  user xattrs (`user.*`) are per-fs (ext4/xfs yes, tmpfs yes, vfat no) —
  relevant for storing evidence tags on quarantined items portably: prefer the
  JSONL ledger over fs xattrs.

### 8. Platform delta vs macOS (summary)

| Capability | macOS (baseline) | Windows | Linux |
|---|---|---|---|
| atomic no-clobber move | `renamex_np(RENAME_EXCL)` | `SetFileInformationByHandle(FileRenameInfoEx, Flags=0)` (all Win10 ≥1607) | `renameat2(RENAME_NOREPLACE)` (kernel 3.15+; ext4/btrfs/xfs/tmpfs ok; **NFS no**) |
| atomic replace-with-open-handles | `renamex_np` variants | `REPLACE_IF_EXISTS\|POSIX_SEMANTICS` (Win10 1709+); else `MoveFileExW` non-atomic | plain `rename()` always POSIX |
| data flush | `F_FULLFSYNC` | `FlushFileBuffers` / `FILE_FLAG_WRITE_THROUGH` | `fsync`/`fdatasync`/`syncfs` (5.8+) |
| dir-entry flush | fsync(dirfd) | none (NTFS journal) | fsync(dirfd) mandatory |
| dataless placeholders | iCloud status keys | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (1709 cldflt) | n/a |
| hardlinks | st_nlink, APFS clones | `CreateHardLink`, `FILE_ID_INFO`, junctions | `st_nlink` |
| carries over unchanged | scan/classify/plan/ledger/WAL/purge/restore *architecture*; evidence engine; privacy invariant | — | — |
| must re-implement | — | execute/ backend (rename trio, flush, walker bulk-enum, trash COM), paths via FOLDERID | execute/ backend (renameat2 + probe/fallback, syncfs pair, walker), paths via XDG |
| impossible | — | dir-fsync; `F_FULLFSYNC`-grade barrier; no-clobber via a single `MoveFileExW` call | OS-trash-equivalent for non-Freedesktop environments; RENAME_NOREPLACE on NFS |

## Recommended crates

| Crate | Version | License | Purpose | Notes |
|---|---|---|---|---|
| `windows` | 0.62.2 | MIT OR Apache-2.0 [S42] | FileRenameInfoEx, FileIdInfo, FlushFileBuffers, FOLDERID_*, FileIdBothDirInfo | Win10 ≥1709 minimum; use the `Win32::Storage::FileSystem` + `Win32::System::Threading` feature sets |
| `rustix` | 1.1.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT [S42] | `fs::renameat_with`, `fs::syncfs`, `fs::statx` | `RenameFlags::NOREPLACE`; libc-free syscall layer; Linux/FreeBSD |
| `libc` | 0.2.189 | MIT OR Apache-2.0 [S42] | `renameat2`, `RENAME_NOREPLACE` constants, `statx` | SPEC R1 already mandates libc/nix |
| `dirs` | 6.0.0 | MIT OR Apache-2.0 [S42] | per-OS cache/config/data/home dirs | runtime_dir/executable_dir are Linux-only ⇒ gate by `Option` |
| `ignore` | 0.4.33 | Unlicense OR MIT [S42] | `WalkParallel`, gitignore filters | only walker that is fully portable today |
| `trash` | 5.2.6 | MIT [S42] | OS trash (Recycle Bin / FreeDesktop / NSWorkspace) | crate msrv 1.85; our own quarantine is the product, trash is `purge` handoff |
| `same-file` | 1.0.6 | Unlicense/MIT [S42] | same-volume / hardlink identity | portable dev+ino vs volume+file-index |
| `filetime` | 0.2.29 | MIT/Apache-2.0 [S42] | set/preserve mtime across move/restore | per-OS utimensat/getattrlist [S41] |
| `xattr` | 1.6.1 | MIT OR Apache-2.0 [S42] | evidence tags on quarantined files (unix only) | Windows unsupported ⇒ ledger-based, not fs-xattr |

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| rust-lang/rust | MIT OR Apache-2.0 (library/std/Cargo.toml) | Windows `rename()` two-tier fallback (MoveFileExW → FileRenameInfoEx) — reference implementation of the API choice | `library/std/src/sys/fs/windows.rs` (`fn rename`) [S8] |
| microsoft/STL | MIT (LICENSE) | canonical Win10 version/flag matrix for POSIX rename (1607 class, 1709 POSIX, 1809 IGNORE_READONLY) | `<filesystem>` rename path, PR #2062 [S5] |
| Byron/dua-cli | MIT (workspace) | per-OS native enumeration: macOS `getattrlistbulk` APFS clone accounting (`SF_FIRMLINK` const, clone_id); Windows `FileIdBothDirInfo` bulk walk + allocated_size + hard_link_id | `crates/dua-lib/src/macos/` + `crates/dua-lib/src/windows.rs` [S30][S31] |
| Byron/trash-rs | MIT (Cargo.toml) | `IFileOperation` + flag usage (FOF_ALLOWUND…); FreeDesktop `.trashinfo` parsing + cross-topdir detection; `set_delete_method` macOS switching | `src/windows.rs`, `src/freedesktop.rs`, `src/macos/mod.rs` [S32][S33][S35] |
| BurntSushi/same-file | Unlicense/MIT | Handle equality (FILE_ID_INFO dual-field), doc comment on file-index aliasing | `src/win.rs`, `src/unix.rs` [S24] |
| Stebalien/xattr | MIT OR Apache-2.0 | `platforms!` macro — pattern for one facade, per-OS backends, runtime `SUPPORTED_PLATFORM` const | `src/sys/mod.rs` [S40] |
| soc/dirs-rs | MIT OR Apache-2.0 | FOLDERID ↔ XDG mapping decision table for classify/ | `README.md` function table [S37] |

## Risks & open questions

1. **Windows version floor.** Disk Janitor's atomic replace relies on Win10
   1709 POSIX rename; setting MSRV to Win10 1709+ is reasonable but must be a
   documented platform-requirement. Pre-1709 falls back to the weaker,
   non-atomic `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` — restore correctness
   still holds (WAL-first), but concurrent reader races surface as
   `ERROR_ACCESS_DENIED`. Decide: hard floor vs degrade-gracefully.
2. **NFS silently rejects RENAME_NOREPLACE** (`-EINVAL`) [S12][S13]. Scanner
   must detect nfs/fuse via `/proc/self/mountinfo` field (9) fstype [S27] and
   either route through `O_TMPFILE`-staged atomic create or refuse auto-action
   on those volumes (HandsOff). Same caveat for Windows SMB shares (no POSIX
   semantics over SMB; cf. MS-FSCC/SMB rename) — treat network shares as
   external volumes.
3. **FreeDesktop copy+delete trash fallback is non-atomic** [S35]: using the
   `trash` crate for cross-volume purge can leave junk mid-copy on crash.
   Mitigation: only trash to a same-volume trash dir; for cross-volume use our
   own `rename`+typed-confirm purge.
4. **`trash` crate getmntent UB note** [S36]: the Linux/FreeBSD backend uses
   non-threadsafe `getmntent` under a Mutex — if Disk Janitor's classify/ thread
   also calls libc mount APIs, UB is possible. Pinning `trash` calls to a single
   dedicated thread (the purge/ actor) avoids it.
5. **xattrs are not portable for evidence tags** (tmpfs-vfat inconsistency,
   Windows unsupported) [S40]: ledger-only is the right call; do not rely on
   fs-level `user.dj.*` xattrs for restore metadata.
6. **`st_flags`/BSD flags are macOS-only in std** [S38]: classify/ must not
   assume `uchfg`/`hidden` are inspectable on Linux/Windows. Linux immutable
   flag = `chattr`/`FS_IOC_GETFLAGS` (rustix `fs::ioctl` + `FS_IOC_*`, not in
   this note's scope); Windows readonly = `FILE_ATTRIBUTE_READONLY`. Treat
   immutable/readonly files as HandsOff by default regardless of platform.

## Sources

1. [MS-FSCC] FileRenameInformationEx spec — https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fscc/4217551b-d2c0-42cb-9dc1-69a716cf6d0c (accessed 2026-08-30)
2. MoveFileExW (winbase.h) — https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw (accessed 2026-08-30)
3. SetFileInformationByHandle (fileapi.h) — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle (accessed 2026-08-30)
4. abi-laboratory kernel32 headers diff (1511→1607) — https://abi-laboratory.pro/compatibility/Windows_10_1511_10586.494_to_Windows_10_1607_14393.0/x86_64/headers_diff/kernel32.dll/diff.html (accessed 2026-08-30)
5. microsoft/STL PR #2062 — https://github.com/microsoft/STL/pull/2062 (accessed 2026-08-30)
6. rust-lang/rust issue #123985 (Win10 1709 POSIX_SEMANTICS) — https://github.com/rust-lang/rust/issues/123985 (accessed 2026-08-30)
7. FILE_RENAME_INFORMATION (ntifs.h, driver docs) — https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information (accessed 2026-08-30)
8. Rust std Windows fs rename impl — https://github.com/rust-lang/rust/blob/master/library/std/src/sys/fs/windows.rs (accessed 2026-08-30)
9. windows-rs FILE_RENAME_INFO struct — https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Storage/FileSystem/struct.FILE_RENAME_INFO.html (accessed 2026-08-30)
10. Rust std fs::rename docs — https://doc.rust-lang.org/std/fs/fn.rename.html (source: `library/std/src/fs.rs`) (accessed 2026-08-30)
11. windows-rs FileRenameInfoEx constant — https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Storage/FileSystem/constant.FileRenameInfoEx.html (accessed 2026-08-30)
12. rename(2) — renameat2, RENAME_*, fs support matrix, BUGS — https://man7.org/linux/man-pages/man2/rename.2.html (accessed 2026-08-30)
13. Linux kernel nfs_rename (fs/nfs/dir.c, `if (flags) return -EINVAL`) — https://github.com/torvalds/linux/blob/master/fs/nfs/dir.c (accessed 2026-08-30)
14. open(2) O_TMPFILE — https://man7.org/linux/man-pages/man2/open.2.html (accessed 2026-08-30)
15. libc crate constants/fn (src/unix/linux_like/linux/{mod.rs,gnu/mod.rs}) — https://github.com/rust-lang/libc (accessed 2026-08-30)
16. rustix fs::renameat_with — https://docs.rs/rustix/latest/rustix/fs/fn.renameat_with.html (accessed 2026-08-30)
17. FlushFileBuffers (fileapi.h) — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers (accessed 2026-08-30)
18. File Caching (FILE_FLAG_WRITE_THROUGH, metadata caching) — https://learn.microsoft.com/en-us/windows/win32/fileio/file-caching (accessed 2026-08-30)
19. sync, syncfs(2) — https://man7.org/linux/man-pages/man2/sync.2.html (accessed 2026-08-30)
20. fsync(2) (directory-entry caveat) — https://man7.org/linux/man-pages/man2/fsync.2.html (accessed 2026-08-30)
21. Cloud Sync Engines portal — https://learn.microsoft.com/en-us/windows/win32/cfapi/cloud-files-api-portal (accessed 2026-08-30)
22. File Attribute Constants (RECALL_ON_DATA_ACCESS etc.) — https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants (accessed 2026-08-30)
23. CreateFileW (FILE_FLAG_OPEN_NO_RECALL, ADS naming, supersede caveat) — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew (accessed 2026-08-30)
24. same-file unix/win backends — https://github.com/BurntSushi/same-file/tree/master/src (accessed 2026-08-30)
25. Rust std os::unix::fs::MetadataExt — https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html (accessed 2026-08-30)
26. Hard Links and Junctions (NTFS) — https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions (accessed 2026-08-30)
27. proc(5) mountinfo (major:minor = st_dev), Documentation/filesystems/proc.rst — https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html and https://github.com/torvalds/linux/blob/master/Documentation/filesystems/proc.rst (accessed 2026-08-30)
28. ignore crate WalkBuilder/WalkParallel — https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html (accessed 2026-08-30)
29. jwalk 0.9.0 (crates.io) — https://crates.io/crates/jwalk (accessed 2026-08-30)
30. dua-cli dua-lib native walkers — https://github.com/Byron/dua-cli/tree/master/crates/dua-lib/src (accessed 2026-08-30)
31. dua-cli CHANGELOG (2.41–2.43 native walker history) — https://github.com/Byron/dua-cli/blob/master/CHANGELOG.md (accessed 2026-08-30)
32. trash-rs macOS backend (Finder vs NsFileManager) — https://github.com/Byron/trash-rs/blob/master/src/macos/mod.rs (accessed 2026-08-30)
33. trash-rs windows.rs (IFileOperation flags) — https://github.com/Byron/trash-rs/blob/master/src/windows.rs (accessed 2026-08-30)
34. trash-rs Cargo.toml (COM features, version) — https://github.com/Byron/trash-rs/blob/master/Cargo.toml (accessed 2026-08-30)
35. trash-rs freedesktop.rs (topdir, copy+delete fallback) — https://github.com/Byron/trash-rs/blob/master/src/freedesktop.rs (accessed 2026-08-30)
36. trash-rs lib.rs (cfg platform map, os_limited, UB note) — https://github.com/Byron/trash-rs/blob/master/src/lib.rs (accessed 2026-08-30)
37. dirs-rs README (per-OS path table) — https://github.com/soc/dirs-rs (accessed 2026-08-30)
38. Rust std os::darwin::fs (st_flags cfg) — https://github.com/rust-lang/rust/blob/master/library/std/src/os/darwin/fs.rs (accessed 2026-08-30)
39. rustix StatxFlags (BTIME) — https://docs.rs/rustix/latest/rustix/fs/struct.StatxFlags.html (accessed 2026-08-30)
40. xattr platforms! macro (SUPPORTED_PLATFORM) — https://github.com/Stebalien/xattr/blob/master/src/sys/mod.rs (accessed 2026-08-30)
41. filetime macos.rs (utimensat fallback) — https://github.com/alexcrichton/filetime/blob/master/src/unix/macos.rs (accessed 2026-08-30)
42. crates.io version/license fields (windows 0.62.2, windows-sys 0.61.2, rustix 1.1.4, libc 0.2.189, dirs 6.0.0, ignore 0.4.33, trash 5.2.6, same-file 1.0.6, filetime 0.2.29, xattr 1.6.1, jwalk 0.9.0, dua-core 3.3.0) — https://crates.io (accessed 2026-08-30)
