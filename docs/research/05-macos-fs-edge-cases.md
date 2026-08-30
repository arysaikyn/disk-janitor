# macOS Filesystem Edge Cases — Research Note

Safe atomic mover & quarantine design (`renamex_np`/`RENAME_EXCL`, APFS clone/snapshot/purgeable/firmlink, `F_FULLFSYNC`, dataless iCloud files, UF_/SF_ flags, xattrs, per-volume quarantine) · researched 2026-08-30

> Verification method: Apple man pages + macOS 26 (Darwin 25.3.0) Command Line Tools SDK headers read directly; nix/libc/rustix sources read on GitHub main/master; crate versions/licenses from the crates.io API; plus small C probes compiled and run on this machine (`/tmp` + `/System/Volumes/VM`, APFS) — results marked **[empirical]**.

## Findings

### 1. Extended rename: `renamex_np` / `renameatx_np` (macOS 10.12+)

Declared in `<sys/stdio.h>` behind `__DARWIN_C_LEVEL >= __DARWIN_C_FULL`, availability `__OSX_AVAILABLE(10.12)` (verified in SDK + darwin-xnu `bsd/sys/stdio.h`):

```c
int renamex_np(const char *from, const char *to, unsigned int flags);
int renameatx_np(int fromfd, const char *from, int tofd, const char *to, unsigned int flags);
```

Flags (exact values from SDK `<sys/stdio.h>`):

| Constant | Value | Effect |
|---|---|---|
| `RENAME_SECLUDE` | 0x1 | "seclude" rename (unrelated observers don't see it) |
| `RENAME_SWAP` | 0x2 | atomically swap source and target (file↔dir allowed); `EINVAL` if OR'd with `RENAME_EXCL` |
| `RENAME_EXCL` | 0x4 | fail `EEXIST` if destination exists |
| `RENAME_NOFOLLOW_ANY` | 0x10 | `ELOOP` if any symlink is met during path resolution |
| `RENAME_RESOLVE_BENEATH` | 0x20 | `ENOTCAPABLE` if from/to escapes the starting hierarchy |

`rename(2)` semantics that matter for the mover (man page, Apple SDK):

- Plain `rename` **guarantees an instance of `new` always exists even if the system crashes mid-operation** — the move itself can never lose the file. [source 5]
- `EXDEV` when old/new are on different file systems. **[empirical]** `renamex_np("/tmp/x", "/System/Volumes/VM/x", RENAME_EXCL)` → `errno=18 (Cross-device link)` — different APFS *volumes in the same container* still fail; "same volume" means same `st_dev`.
- `ENOTSUP` if the file system doesn't support the flags (exFAT/FAT sticks: don't assume `RENAME_EXCL`).
- `EDEADLK` if `from` names a **dataless** file/dir that must be materialized while the thread's I/O policy forbids materialization (`getiopolicy_np(3)`) — see §5.
- `EPERM` on sticky directories (`/tmp`, `.Trash`) unless you own the entry.
- `rename` moves the **symlink itself**, never its target.
- Known HFS+ 10.12.3 bug rdar://33171610: `RENAME_EXCL` wrongly returned `EEXIST` for case-only changes in one dir. **[empirical]** Works on APFS today (case-change rename with `RENAME_EXCL` → rc 0). [source 11]

Capability probing: `getattrlist(path, …)` with `volattr = ATTR_VOL_INFO | ATTR_VOL_CAPABILITIES` returns `vol_capabilities_attr_t`; index `VOL_CAPABILITIES_INTERFACES`; test `VOL_CAP_INT_RENAME_EXCL` (0x00080000) and `VOL_CAP_INT_RENAME_SWAP` (0x00040000) from `<sys/attr.h>`.

**[empirical]** Root APFS volume reports interfaces mask `0x039b6edf`: `RENAME_EXCL` ✓, `CLONE` ✓, `SNAPSHOT` ✓, `BARRIERFSYNC` ✓, `PUNCHHOLE` ✓ — but **`RENAME_SWAP` bit is clear although `renamex_np(…, RENAME_SWAP)` succeeds and really swaps contents** (verified A/B payloads). Treat capability bits as advisory: probe once per volume, then trust errno (`EINVAL`/`ENOTSUP`) at call time.

Same-`st_dev` rename is atomic w.r.t. concurrent observers and never leaves a partial state; **[empirical]** firmlinked System/Data paths share `st_dev` (`/`, `/Users`, `/System/Volumes/Data/Users` all `dev=16777234`), so firmlink traversal does **not** break same-volume moves.

### 2. EXDEV and the quarantine-must-be-same-volume rule

| From → To | Result | Notes |
|---|---|---|
| `/tmp` → `/System/Volumes/VM` (same container, different volume) | **EXDEV** [empirical] | APFS container ≠ one volume |
| `~/...` → `/Volumes/USB/...` | EXDEV | separate container/device |
| `~/...` → `/System/Volumes/Data/Users/...` | OK [empirical] | firmlink, same `st_dev` |
| local volume → NFS/SMB mount | EXDEV | also no-trash semantics (source 22) |

Cross-device fallback ladder (plan-phase must detect and refuse or downgrade): ① `clonefile` + `rename` (still `EXDEV` for clone, so ②) `copyfile(3)` with `COPYFILE_MOVE | COPYFILE_ALL` (copy+unlink, **not atomic** — only with explicit user consent), else refuse with evidence.

### 3. APFS specifics: clonefile, snapshots, purgeable, firmlinks

**clonefile(2)** (`<sys/clonefile.h>`, 10.12+): `clonefile/clonefileat/fclonefileat`; copy-on-write clone sharing data blocks; dest must not exist (`EEXIST`, **[empirical]**); `EXDEV` across filesystems (man); flags `CLONE_NOFOLLOW 0x1`, `CLONE_NOOWNERCOPY 0x2`, `CLONE_ACL 0x4`, `CLONE_NOFOLLOW_ANY 0x8`, `CLONE_RESOLVE_BENEATH 0x10`; setuid/setgid bits dropped on the clone; later overwrite of a shared block can `ENOSPC`; directory cloning strongly discouraged (man). Use: cheap snapshot of the ledger/WAL before migration; size-evidence without reading data.

**Snapshots**: `fs_snapshot_create/list/delete/rename/mount/revert` in `<sys/snapshot.h>` (10.12+). **create/delete/revert require the Apple-granted `com.apple.developer.vfs.snapshot` entitlement** (DTS whitelist; forums thread 89635); `fs_snapshot_list` is usable without it. Public alternatives: `tmutil localsnapshot`, Disk Utility. Consequence for purge: **bytes stay pinned by existing local snapshots**; freed-space reporting must be honest ("purgeable after snapshot expiry").

**Purgeable space**: on-disk inode flags in the Apple File System Reference: `INODE_IS_PURGEABLE 0x00080000` ("deleted at the next purge — purge is requested from user space and performed by the OS"), `INODE_WANTS_TO_BE_PURGEABLE 0x00100000`, `INODE_IS_SYNC_ROOT 0x00200000` (fileproviderd sync roots). These are **not** visible via `st_flags` (outside the `UF_/SF_` settable masks). System purge (CacheDelete) frees caches first and Time-Machine snapshots last, staged; not dependable for immediate space. [sources 4, 15]

**Firmlinks**: bidirectional directory links stitching the read-only sealed System volume (`/`, `apfs, sealed, read-only` in `mount` output) with the writable Data volume (`/System/Volumes/Data`). Marked by `SF_FIRMLINK 0x00800000` st_flag and the `com.apple.fs.firmlink` xattr (`FIRMLINK_EA_NAME`); not user-creatable (forums 117521). `fcntl(F_GETPATH_NOFIRMLINK)` and `ATTR_CMNEXT_NOFIRMLINKPATH` (needs `FSOPT_ATTR_CMN_EXTENDED`) resolve the non-firmlinked real path — use for canonical WAL paths. [sources 4, 16, 17]

### 4. fsync vs `F_FULLFSYNC` on APFS — what actually flushes

- `fsync(2)`: flushes host buffers to the *drive*; the drive itself may delay, reorder, or never physically persist; Apple's man page calls the resulting loss scenario "easily reproduced with real world workloads". [source 2]
- `fcntl(fd, F_FULLFSYNC)` — cmd **51** (`<sys/fcntl.h>`): fsync + asks the drive to flush all buffered data to permanent storage; **drains the device queue and acts as a barrier**; implemented on HFS, FAT, UDF and **APFS**; slow; some FireWire bridges ignore it. **[empirical]** `fcntl(fd, F_FULLFSYNC)` on an open APFS file → rc 0. [source 3]
- `fcntl(fd, F_BARRIERFSYNC)` — cmd **85**: fsync + drive barrier; orders previously-fsynced I/O before later I/O on the same device **without** full durability; HFS/APFS; needs hardware barrier support (Apple SSDs guarantee it). Right tool when ordering matters but durability of everything doesn't.
- libc 0.2.189 exposes `F_FULLFSYNC`/`F_BARRIERFSYNC`; nix 0.31.3 wraps both (`fcntl::FcntlArg::F_FULLFSYNC`, `F_BARRIERFSYNC`) plus `F_GETPATH`/`F_GETPATH_NOFIRMLINK` on apple_targets (sources 7, 8).
- **WAL recipe**: write WAL record → `F_FULLFSYNC` on WAL fd → `renameatx_np(RENAME_EXCL)` → optionally `F_FULLFSYNC` the parent dir fd (open `O_RDONLY|O_DIRECTORY`) → mark committed. Directory-entry durability after rename is the textbook gap on POSIX; on APFS an arXiv study of crash-consistent checkpointing found rename typically durable without dir-sync, but the dir-sync closes it and is cheap at WAL-commit frequency. [source 24]
- Corroboration: Apple's bundled SQLite historically got durability semantics wrong without fullfsync (bonsaidb writeup). [source 21]

### 5. Dataless iCloud / File Provider files — detect without triggering download

Official guidance is **TN3150 "Getting ready for dataless files"** (2023-05-09):

- Detection: call `stat` or `getattrlist` and test **`SF_DATALESS` (0x40000000) in `stat.st_flags`** (SDK `<sys/stat.h>`; read-only synthetic flag). Caveat Apple states explicitly: `stat`/`getattrlist` **do materialize dataless intermediate directories** in the path.
- Opt-out: `setiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES /* 3 */, IOPOL_SCOPE_THREAD|_PROCESS, IOPOL_MATERIALIZE_DATALESS_FILES_OFF /* 1 */)` — then I/O on dataless items fails `EDEADLK` instead of downloading; restore with the value from `getiopolicy_np`. Constants in `<sys/resource.h>`: `_DEFAULT 0`, `_OFF 1`, `_ON 2`, `_ORIG 4`. [source 1]
- Rename of a dataless item under OFF policy → `EDEADLK` (rename(2) ERRORS list). So: scanner thread sets OFF; apply-phase must materialize-or-refuse dataless targets before quarantine (and classify them, never purge them).
- `st_blocks == 0` heuristic (man: blocks in 512-byte units; short symlinks legitimately have 0): is a *fallback* only — sparse files on APFS allocate blocks for written tails (**[empirical]** 10 MiB sparse file: `st_size=10485764 st_blocks=20488`), and `getattrlist` `ATTR_FILE_ALLOCSIZE` (0x4) reads allocated size without reading data. Prefer `SF_DATALESS` first; never open() a dataless file (that materializes).
- iCloud Drive specifics: Sonoma-era FileProvider eviction creates APFS dataless files (no more `.name.icloud` stubs for modern setups; legacy `.icloud` stubs still exist in some providers). File Provider domains mount under `~/Library/CloudStorage/<provider>` (mandatory since ~13.0); dataless dirs trigger `fetchContents` on access (WWDC21 10182). Detection of *materialized-ness* via high-level API: `URLResourceValues`/`NSFileManager` `NSURLUbiquitousItemDownloadingStatusKey` (== `NSURLUbiquitousItemDownloadingStatusCurrent` ⇔ data present locally) — read via `resourceValues(forKeys:)`, does not trigger download. [sources 1, 12, 13, 14]
- `INODE_IS_SYNC_ROOT 0x00200000` (on-disk) marks fileproviderd sync roots — treat `~/Library/CloudStorage` as HandsOff by default.

### 6. File flags (UF_/SF_), xattrs, resource forks, hardlinks, symlinks

**BSD flags** (`<sys/stat.h>`, `st_flags`, set via `chflags(2)`; exposed by nix `FileFlag` (missing `SF_DATALESS`/`UF_DATAVAULT`/`SF_FIRMLINK` — use libc constants or own consts)):

| Flag | Value | Meaning for Disk Janitor |
|---|---|---|
| `UF_HIDDEN` | 0x8000 | Finder-hidden hint; quarantine dir should set it [empirical: set via chflags, survives rename] |
| `UF_COMPRESSED` | 0x20 | HFS+/APFS compression: `st_size` > real blocks; never trust size without `ATTR_FILE_ALLOCSIZE` |
| `UF_IMMUTABLE`/`SF_IMMUTABLE` | 0x2/0x20000 | rename/purge will EPERM — pre-check and surface evidence |
| `UF_APPEND`/`SF_APPEND` | 0x4/0x40000 | writes append-only; delete may still work but file is active |
| `UF_TRACKED` | 0x40 | document-ID tracking: no delete/rename notifications — moving it breaks ID tracking |
| `UF_DATAVAULT` | 0x80 | entitlement required to read — classify HandsOff |
| `UF_OPAQUE` | 0x8 | union-mount veil, rare |
| `SF_RESTRICTED` | 0x80000 | SIP-protected; EPERM guaranteed |
| `SF_NOUNLINK` | 0x100000 | cannot unlink/rename/mount-over |
| `SF_FIRMLINK` | 0x800000 | firmlink inode |
| `SF_DATALESS` | 0x40000000 | dataless (see §5) — read-only, can't set |

**Extended attributes** (xattr(2)/`getxattr` etc.; libc has full xattr API; constants `XATTR_NOFOLLOW 0x1`, `XATTR_CREATE`, `XATTR_REPLACE`, `XATTR_SHOWCOMPRESSION 0x20`; names `XATTR_FINDERINFO_NAME "com.apple.FinderInfo"`, `XATTR_RESOURCEFORK_NAME "com.apple.ResourceFork"`; max name 127):

- `com.apple.quarantine` — Gatekeeper mark; UTF-8 `flags;timestamp-hex;agent;UUID` (4 semicolon fields); presence ⇒ file downloaded by an app ⇒ good Redownloadable/Expensive evidence; preserved by rename; events DB `~/Library/Preferences/com.apple.LaunchServices.QuarantineEventsV2`. [sources 19, 20]
- `com.apple.FinderInfo` (32 B), `com.apple.ResourceFork` — a file with a resource fork may carry data there; purge math must count it; on non-native filesystems it is externalized as `._name` AppleDouble files (plus `.DS_Store` Finder droppings — pure junk, safe grade, `dot_clean(1)` merges `._` files). [source 18]
- `com.apple.metadata:kMDItemWhereFroms` — Spotlight provenance; extra download-source evidence.
- `com.apple.decmpfs` — compression header (pairs with `UF_COMPRESSED`); **do not copy with plain read/write** (decompresses); use `clonefile`/`copyfile` to preserve.
- Renaming a file preserves all xattrs **[empirical: flags survive rename]**; quarantine moves therefore lose nothing.

**Hardlinks**: `st_nlink > 1` ⇒ shared inode; renaming one link leaves `st_nlink` unchanged (**[empirical]**: link 2→2 after rename). Quarantining one hardlink does *not* free the other's bytes — byte-count honesty requires per-inode (`st_dev`,`st_ino`) dedup in scan; purge of "the last link" only frees bytes when count drops to 0. `st_blocks`-based size must be attributed once per inode. Directory hardlinks: APFS/HFS+ mostly disallow (dir `st_nlink` semantics differ; deadlock caveat in rename(2) CAVEATS).

**Symlinks**: every walker step must use `O_NOFOLLOW`-style discipline: prefer `fstatat(fd, name, AT_SYMLINK_NOFOLLOW)` (nix `fstatat` + `AtFlags::AT_SYMLINK_NOFOLLOW`; libc `AT_SYMLINK_NOFOLLOW 0x20`, `AT_FDCWD -2`, `AT_REMOVEDIR 0x80`), `getattrlistat` + `FSOPT_NOFOLLOW`/`FSOPT_NOFOLLOW_ANY`, `renamex_np … RENAME_NOFOLLOW_ANY` (moves the link itself; fails `ELOOP` if a *path component* is a symlink — TOCTOU armor). Quarantine of a symlink: rename the link, never follow; classify the target only if explicitly resolved.

**getattrlistbulk(2)** — `int getattrlistbulk(int dirfd, struct attrlist *, void *buf, size_t, uint64_t options)`: bulk per-directory-entry metadata in one syscall (returns link info for symlinks, order unspecified); exactly what the parallel walker wants (Foundation's own dir enumeration sits on it — see TN3150 trace). libc exposes `getattrlistbulk` + `FSOPT_*` + `VOL_CAP_INT_*` constants. [source 10]

### 7. Quarantine-directory design (same-volume, hidden, reversible)

Requirements derived from §1–§2:

- **Per-volume quarantine root**, not one global dir: `<volume-mountpoint>/.Trash-janitor/` (or `.janitor-quarantine/`), sibling to Apple's own layout (`~/.Trash` boot volume; `/Volumes/X/.Trashes/<uid>` per external volume). Same-`st_dev` ⇒ `renamex_np(RENAME_EXCL)` stays atomic; EXDEV can't happen by construction (verify with `fstatat`/`statfs` on both ends before moving; refuse on mismatch).
- Create with `mkdir(…, 0700)` + `chflags(path, UF_HIDDEN)` (**[empirical]** flag sticks through rename) and never reuse Apple's `.Trashes` (root-owned, ACL/permission minefield — even elevated listing is unreliable there; Finder owns its lifecycle). [source 22]
- Per-operation subdir `<volume>/.Trash-janitor/<wal-op-id>/` so restore = reverse `renameatx_np` recorded in the WAL; purge = `unlinkat(REMOVE_DIR recursive)` typed-confirm.
- Name collisions: `RENAME_EXCL` returns `EEXIST` [empirical] — retry with `-<n>` suffix instead of overwrite; the syscall *is* the existence check (no TOCTOU vs. plain `rename`).
- Never place quarantine on a dataless/File-Provider volume (`~/Library/CloudStorage` is mounted, has trash semantics we don't control); whitelist: APFS/HFS+ volumes with `VOL_CAP_INT_RENAME_EXCL` set; exFAT/FAT → refuse auto-apply with evidence (`ENOTSUP`).
- External-volume edge: mount-point disappears mid-quarantine ⇒ WAL records `st_dev`+`mntonname` (from `statfs`/`getfsstat`) so restore can report "volume absent" instead of failing cryptically. Network volumes: Apple has never supported trash/temp dirs on them (filesystem-dev list) — mark HandsOff. [source 23]
- ACL note from rename(2) CAVEATS: moving into a dir with inheritable ACLs does **not** apply them — quarantine ACL must be set explicitly after move if used.

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| nix-rust/nix v0.31.3 (crates.io) | MIT (crates.io `license` field) | typed `renameat`, `fcntl` cmds `F_FULLFSYNC`/`F_BARRIERFSYNC`/`F_GETPATH(_NOFIRMLINK)`, `fstatat`+`AtFlags`, `FileFlag` bitflags, `Errno` mapping | `src/fcntl.rs` (`RenameFlags` is **Linux-only** — not macOS), `src/unistd.rs::renameat`, `src/sys/stat.rs` |
| rust-lang/libc v0.2.189 | MIT OR Apache-2.0 (crates.io) | raw decls: `renamex_np`, `renameatx_np`, `clonefile(,at)`, `fclonefileat`, `getattrlist(,at,bulk)`, xattr family; consts `RENAME_SWAP/EXCL`, `VOL_CAP_INT_*`, `FSOPT_*`, `UF_/SF_` (no `SF_DATALESS`/`UF_DATAVAULT`/`RENAME_NOFOLLOW_ANY` — define those yourself), `F_FULLFSYNC=51`, `F_BARRIERFSYNC=85` | `src/unix/bsd/apple/mod.rs` (lines ~3199, 3745–3751, 4634–4635, 4787–4795, 4987) |
| bytecodealliance/rustix v1.1.4 | Apache-2.0 OR MIT OR Apache-2.0-WITH-LLVM-exception (crates.io) | io_uring-free `openat`/`fstatat` types, `fs::xattr` module (`getxattr/listxattr/setxattr/removexattr` + `l*`/`f*` variants), `Arg` path API | `src/fs/xattr.rs`, `src/fs/` — **no** macOS rename-with-flags |
| ArturKovacs/trash v5.2.6 (crates.io `trash`) | MIT (crates.io) | reference for per-volume macOS trash semantics + restore metadata format (do not use the crate itself for quarantine moves — it uses Finder Trash semantics, not our WAL) | `src/platform/macos.rs` in that repo |
| hyper0x/icloud-pull | MIT (GitHub API `license.spdx_id`) | reads BSD flags to find evicted iCloud files — tiny working example of §5 detection | repo root |
| apple/darwin-xnu | APSL-2.0 (Apple Public Source License — **copy-with-care**: reference only, do not paste code) | syscall flag semantics source of truth (`bsd/sys/stdio.h`, `bsd/vfs/vfs_syscalls.c`) | header files |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| `libc` 0.2.189 | raw FFI decls for `renamex_np`, `clonefile`, `getattrlistbulk`, xattr | only crate that already declares the Darwin syscalls we need; MIT OR Apache-2.0 | hand-written `extern "C"` blocks (define missing consts `SF_DATALESS`, `UF_DATAVAULT`, `RENAME_NOFOLLOW_ANY`, `RENAME_RESOLVE_BENEATH`, `RENAME_SECLUDE`, `IOPOL_*` locally) |
| `nix` 0.31.3 | typed errno + `fstatat`, `fcntl(F_FULLFSYNC)`, `renameat` | ergonomic error mapping; feature `fs` needed for fcntl enums | rustix (no macOS rename-flags either) |
| `rustix` 1.1.4 | safe fd/path APIs + full `fs::xattr` | best xattr abstraction; `Arg` path trait avoids UB on interior NULs | xattr crate |
| `xattr` 1.6.1 | minimal cross-platform xattr get/set/list/remove | MIT OR Apache-2.0; fine if rustix feels heavy | rustix `fs::xattr` |
| `walkdir` | ordered traversal base for the scanner | de-facto standard; pairs with `getattrlistbulk` for the hot path | custom readdir loop |
| `trash` 5.2.6 | optional Finder-Trash interop (show user where Apple trash lives) | MIT; **not** for our quarantine moves | none (skip) |

## Risks & open questions

- **`RENAME_SWAP` capability-bit mismatch**: root APFS reports `VOL_CAP_INT_RENAME_SWAP` clear yet `RENAME_SWAP` works [empirical]. Question: does any real macOS filesystem clear the bit *and* fail the call (vs. merely returning `EINVAL`)? Probe-and-fallback (try once, cache result per volume) is the safe pattern.
- **Dataless + `EDEADLK` interplay**: scanner sets `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` process-wide? or thread-scoped only (crossbeam workers)? Process-wide affects every library we call; needs a runtime toggle + tests against a real iCloud Drive volume.
- **Purge honesty under APFS snapshots**: bytes freed by purge stay pinned by Time Machine local snapshots; our "freed bytes" counter must quote the *purgeable-after-snapshot-expiry* number or users will report "it lied". How to read snapshot pinning without `fs_snapshot_list` entitlement is unresolved (entitlement path exists but requires DTS).
- **exFAT/FAT external volumes**: no `RENAME_EXCL`/`clonefile`; quarantine design needs a documented downgrade or hard refusal. Decision needed: refuse auto-apply (recommended) vs. non-atomic copy fallback.
- **`st_blocks==0` vs sparse vs compressed**: `SF_DATALESS` is authoritative, but pre-13.x macOS (HFS+ external disks, older APFS) has no dataless concept — heuristic layering (dataless flag → `ATTR_FILE_ALLOCSIZE` → size) must be per-filesystem-version aware; untested on HFS+ here.
- **Quarantine dir discoverability**: hidden via `UF_HIDDEN` but visible to `ls -a`/du tools; other cleaners may flag `.Trash-janitor` as junk — consider `com.apple.FinderInfo` type marker or naming convention negotiation; also open question whether TCC (Privacy) will prompt for `/Volumes` access at apply time (likely yes for external volumes).

## Sources

1. Apple TN3150 — Getting ready for dataless files — https://developer.apple.com/documentation/technotes/tn3150-getting-ready-for-data-less-files — accessed 2026-08-30
2. fsync(2) man page (Apple archive; identical text in local SDK man) — https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html — accessed 2026-08-30
3. fcntl(2) man page (F_FULLFSYNC, F_BARRIERFSYNC, F_GETPATH_NOFIRMLINK) — https://keith.github.io/xcode-man-pages/fcntl.2.html (mirror of Apple SDK man; verified against local `man 2 fcntl` headers) — accessed 2026-08-30
4. Apple File System Reference (PDF, 2020-06-22): `INODE_IS_PURGEABLE`, `FIRMLINK_EA_NAME`, snapshot metadata — https://developer.apple.com/support/downloads/Apple-File-System-Reference.pdf — accessed 2026-08-30
5. rename(2) man page — local SDK (`man 2 rename`, Command Line Tools 26.x, macOS 26/Darwin 25.3.0) — accessed 2026-08-30 (mirror: https://manp.gs/mac/2/rename)
6. darwin-xnu `bsd/sys/stdio.h` (renamex_np/renameatx_np availability 10.12, flag values) — https://github.com/apple/darwin-xnu/blob/master/bsd/sys/stdio.h — accessed 2026-08-30
7. nix crate source (master): fcntl enums incl. `F_FULLFSYNC`/`F_BARRIERFSYNC`; `RenameFlags` Linux-gated — https://github.com/nix-rust/nix/blob/master/src/fcntl.rs and `/src/sys/stat.rs` — accessed 2026-08-30
8. nix on crates.io — version 0.31.3, license MIT — https://crates.io/api/v1/crates/nix/0.31.3 — accessed 2026-08-30
9. libc on crates.io — version 0.2.189, license MIT OR Apache-2.0 — https://crates.io/api/v1/crates/libc/0.2.189 — accessed 2026-08-30
10. getattrlistbulk(2) man page — local SDK; libc decl https://github.com/rust-lang/libc/blob/main/src/unix/bsd/apple/mod.rs — accessed 2026-08-30
11. OpenRadar 33171610 — renameatx_np RENAME_EXCL case-change EEXIST on HFS+ (10.12.3) — http://openradar.appspot.com/33171610 — accessed 2026-08-30
12. Apple File Provider — Synchronizing files using file provider extensions (fetchContents/dataless) — https://developer.apple.com/documentation/fileprovider/synchronizing-files-using-file-provider-extensions — accessed 2026-08-30
13. WWDC21 10182 — Sync files to the cloud with FileProvider on macOS — https://developer.apple.com/videos/play/wwdc2021/10182/ — accessed 2026-08-30
14. Eclectic Light Company — iCloud Drive in Sonoma: FileProvider and eviction — https://eclecticlight.co/2023/11/21/icloud-drive-in-sonoma-fileprovider-and-eviction/ — accessed 2026-08-30
15. Eclectic Light Company — Aren't snapshots purgeable? / Why can purging snapshots fail? — https://eclecticlight.co/2026/08/24/arent-snapshots-purgeable/ and https://eclecticlight.co/2026/08/27/why-can-purging-snapshots-fail/ — accessed 2026-08-30
16. Apple Developer Forums 117521 — firmlinks not user-creatable — https://developer.apple.com/forums/thread/117521 — accessed 2026-08-30
17. Eclectic Light Company — macOS Catalina boot volume layout (firmlink map) — https://eclecticlight.co/2019/10/08/macos-catalina-boot-volume-layout/ — accessed 2026-08-30
18. Apple StackExchange — `._` AppleDouble files & `dot_clean` — https://apple.stackexchange.com/questions/14980/why-are-dot-underscore-files-created-and-how-can-i-avoid-them — accessed 2026-08-30
19. Eclectic Light Company — Quarantine and the quarantine flag (xattr format) — https://eclecticlight.co/2020/10/29/quarantine-and-the-quarantine-flag/ — accessed 2026-08-30
20. Launch Services Release Notes — File Quarantine properties (kLSQuarantine*) — https://developer.apple.com/library/archive/releasenotes/Carbon/RN-LaunchServices/index.html — accessed 2026-08-30
21. Bonsaidb — SQLite on macOS ACID/durability note — https://bonsaidb.io/blog/acid-on-apple/ — accessed 2026-08-30
22. Apple Discussions / StackExchange — per-volume `.Trashes` permissions — https://discussions.apple.com/thread/8539616 and https://apple.stackexchange.com/questions/399093/not-even-root-can-ls-trashes-on-external-disk-in-macos — accessed 2026-08-30
23. filesystem-dev list — no trash/temp dirs on network volumes — https://www.mail-archive.com/filesystem-dev@lists.apple.com/msg00085.html — accessed 2026-08-30
24. arXiv 2511.18323 — Crash-Consistent Checkpointing on macOS/APFS (rename durability + dir fsync) — https://arxiv.org/html/2511.18323v1 — accessed 2026-08-30
25. Apple Developer Forums 89635 — fs_snapshot_create entitlement (com.apple.developer.vfs.snapshot) — https://developer.apple.com/forums/thread/89635 — accessed 2026-08-30
26. trash crate (crates.io API: v5.2.6, MIT; repo ArturKovacs/trash) — https://crates.io/api/v1/crates/trash/5.2.6 — accessed 2026-08-30
27. xattr crate (crates.io API: v1.6.1, MIT OR Apache-2.0) — https://crates.io/api/v1/crates/xattr/1.6.1 — accessed 2026-08-30
28. rustix crate (crates.io API: v1.1.4, license triple) + source — https://crates.io/api/v1/crates/rustix/1.1.4 , https://github.com/bytecodealliance/rustix/blob/main/src/fs/xattr.rs — accessed 2026-08-30
29. icloud-pull repo (MIT via GitHub API) — https://github.com/hyper0x/icloud-pull — accessed 2026-08-30
30. Empirical probes run 2026-08-30 on this machine (macOS 26, Darwin 25.3.0, APFS): RENAME_EXCL/NOFOLLOW_ANY/SWAP|EXCL errors; EXDEV `/tmp`→`/System/Volumes/VM`; clonefile EEXIST; F_FULLFSYNC rc=0; VOL_CAP interfaces mask 0x039b6edf; chflags UF_HIDDEN survives rename; hardlink nlink preserved; sparse st_blocks; firmlink st_dev equality. (Local C probes; sources are the compiled outputs, not reproducible URLs.)
