# Ledger & WAL Crash Safety on macOS (JSONL + APFS) — Research Note
Scope: crash-safe write-ahead logging and append-only JSONL ledgers for the `ledger/` + `execute/` modules (Rust, macOS/APFS) · researched 2026-08-30

## Findings

### 1. The durability dance — and why plain `fsync` is not enough on macOS

Canonical crash-safe sequence for creating/replacing a file:

1. write payload to a temp file **in the same directory** (same volume ⇒ rename is atomic);
2. fsync the temp file (data + metadata to the device);
3. `rename(temp, final)` — POSIX rename is atomic w.r.t. crashes: "an instance of *new* will always exist, even if the system should crash in the middle of the operation" [3];
4. fsync the **parent directory** — rename is a directory-entry change; without a dir fsync the new name may be lost while the file data survived [16][18][23].

The macOS-specific trap: Darwin `fsync(2)` "will flush all data from the host to the drive … the drive itself may not physically write the data to the platters for quite some time and it may be written in an out-of-order sequence", and Apple explicitly directs databases to `F_FULLFSYNC` [1]. `fcntl(fd, F_FULLFSYNC)` "asks the drive to flush all buffered data to permanent storage … drains the entire queue of the device and acts as a barrier"; implemented on HFS, FAT, UDF, and **APFS** [2]. It is slow — ~4 ms vs ~0.075 ms for a plain fsync on Apple SSDs [25] — so amortize it (sync per operation batch, not per line). Corroborated by independent writeups [24].

Two more Apple specifics:

- `F_BARRIERFSYNC` (`libc` value 85): fsync + drive barrier; guarantees ordering of previously-fsynced I/O **but makes no durability claim on return** — designed for two-phase ordering ("phase one persisted before phase two"), HFS/APFS only, Apple SSDs guaranteed to support it [2][14].
- Even `F_FULLFSYNC` does not give **cross-file** ordering on APFS: SQLite 3.46.1 (2024) shipped a backport for WAL corruption caused by APFS reordering the DB-file and WAL-file fullfsyncs during checkpoint; the fix inserts a **directory sync between the two F_FULLFSYNC calls** [20]. Lesson: between "WAL append" and "snapshot replace", sync a boundary (the parent dir); do not trust two independent fullfsyncs to order.

### 2. Rust std already does the right thing on Apple

Since PR #60121 (merged 2019), `std::fs::File::sync_all()` **and** `sync_data()` on `target_vendor = "apple"` call `fcntl(fd, F_FULLFSYNC)`; other platforms get `fsync`/`fdatasync` [5][6]. Verified in current master: `library/std/src/sys/fs/unix.rs` defines `os_fsync`/`os_datasync` → `libc::fcntl(fd, libc::F_FULLFSYNC)` under `#[cfg(target_vendor = "apple")]` [5]. So the project needs **no extra crate for the data fsync**. There is no std API for fsyncing a *directory*, but opening the directory itself and calling `sync_all()` works on macOS (open(2) permits O_RDONLY on directories; SQLite's `openDirectory` + `full_fsync` does exactly this) [16][18].

### 3. Atomic rename: `renamex_np`, `RENAME_EXCL`, and the tempfile crate

- Apple's extended rename: `renamex_np(from, to, flags)` / `renameatx_np(...)`. `RENAME_EXCL` returns `EEXIST` if the destination exists (FS capability probe: `getattrlist` `VOL_CAP_INT_RENAME_EXCL`); `RENAME_SWAP` swaps atomically; `RENAME_EXCL | RENAME_SWAP` is `EINVAL` [3].
- `tempfile` 3.27.0 (crates.io 2026-03-11): `NamedTempFile::persist` = plain `rename` (overwrites); `persist_noclobber` = `renameat_with(NOREPLACE)` — atomic on Apple since tempfile 3.18.0 — falling back to `hard_link` + `unlink` where unsupported [7][8][9]. **Docs caveat**: "neither the file contents nor the containing directory are synchronized, so the update may not yet have reached the disk when persist returns" [7] — tempfile provides atomicity, never durability; you still do sync(temp) + sync(parent) yourself.
- `libc` binds everything needed: `F_FULLFSYNC = 51`, `F_BARRIERFSYNC = 85`, `F_OFD_SETLK/LKW/GETLK = 90/91/92`, `RENAME_EXCL = 4`, `renamex_np`, `renameatx_np`, `LOCK_SH/EX/NB/UN = 1/2/4/8` [14].

### 4. Record framing for JSONL with crc32fast

Why frame at all: a crash can tear a line mid-write; a bare JSON line cannot distinguish "torn tail" from "corrupt middle". SQLite's answer is per-frame checksums plus a **cumulative chain** [16]; git's answer is content addressing (object name = hash of header+content) plus a whole-file trailer checksum on packs [21][22]. Pragmatic middle ground for a JSONL ledger:

```
{"v":1,"seq":42,"ts":"2026-08-30T12:00:00Z","op":"apply","crc32":1234567890,"rec":{...}}
```

- one self-contained JSON object per line, always newline-terminated;
- `crc32` computed with `crc32fast` over the serialized `rec` bytes (+ `seq`), stored in-band;
- optional `prev_crc` chaining (SQLite-style cumulative checksum) detects *reordering/loss of middle lines*, which independent per-line CRCs cannot [16];
- read with `BufRead::read_until(b'\n')`, never bare `lines()`: byte-exact reads expose whether the final line was newline-terminated and give you the exact offset of the last complete record — required to truncate a torn tail (§5).

### 5. Torn-write detection and tail-truncation recovery

SQLite's WAL recovery is the reference algorithm: recovery is a single pass over the WAL; "the checksums are verified on each frame of the WAL as it is read. The scan stops at the end of the file or at the first invalid checksum", and `mxFrame` becomes the index of the last valid commit frame [16]. A frame is valid iff (a) its salt matches the WAL header salt and (b) the cumulative checksum over header+all prior frames+this frame matches [16]. Translated to a JSONL ledger:

- **Torn tail**: last line lacks `\n` or fails CRC/JSON parse ⇒ it was never fsynced completely ⇒ truncate file to the offset of the last complete line (`truncate`), log the discarded bytes (park them in `*.torn` for forensics). Torn-tail repair on append is established practice in Rust JSONL stores [19].
- **Corrupt middle line** (CRC fails but later lines are fine): do NOT truncate past valid data. SQLite stops at the first invalid frame — but for an *operations ledger* the safer policy is **skip-and-continue**: mark the line as `skipped{offset,reason}` in replay output and keep replaying, because a ledger's later operations (e.g. `restore`) may be independent of the damaged one. Record both `seq` gaps and skipped lines in the recovery report.
- **Salt/epoch field** (SQLite's Salt-1/2 motivation [16]): a random per-epoch value in the file header lets you detect stale frames from a previous file generation after rewrite/compaction.
- Compaction (`checkpoint` in SQLite terms): replay valid records into a fresh file written via the full dance (§1), then atomic-rename over the old one — WAL content is only "committed into" the snapshot after the snapshot itself is synced [16].

### 6. File locking on macOS: `flock` vs fcntl/lockf

| | `flock(2)` | `fcntl F_SETLK/F_SETLKW` | `fcntl F_OFD_SETLK` | `lockf(3)` |
|---|---|---|---|---|
| granularity | whole file | byte-range | byte-range | byte-range |
| ownership | **open file description** (dup/fork share one lock) [4] | process (classic SysV semantics: close of *any* fd to the file drops all the process's locks on it) [4] | open file description; avoids the close() hazard [4] | process |
| macOS | yes, man page present [4] | yes [4] | yes, constants 90/91/92 in libc 0.2.187 [14] | yes [17] |
| crates | `fs2` 0.4.3, `fs4` 1.1.0, `nix::fcntl::Flock` | `nix` `FcntlArg::F_SETLK` | `libc` raw | `libc::lockf` |

Apple's own man page says: "If an application wishes only to do entire file locking, the `flock(2)` system call is more efficient" [4]. All are **advisory** — every cooperating process must lock [4]. Recommendation for Disk Janitor: single-user CLI/TUI on a local APFS volume ⇒ `flock(LOCK_EX | LOCK_NB)` on a sidecar `<db>.lock` (or on the ledger file itself), via `fs4`/`fs2`/`nix`; fail fast with a "another janitor instance is running" message. Beware `fs2`/`fs4` docs: locks are advisory and released when the handle drops [12][13]. Avoid NFS assumptions entirely (out of scope; flock over NFS is historically unreliable [17]).

### 7. Reference patterns: SQLite WAL and git

- **SQLite WAL** (public domain [26]): append-only `-wal` file; transactions commit at a frame with a non-zero commit marker; checkpoint later transfers WAL content into the main DB; "Checksums and counters attached to each frame are used to determine which frames within the WAL are valid" [15][16]. The `-shm` index is derived, never fsynced ("does not need to be preserved across a crash"), rebuilt by recovery on first connect [16]. Lesson: **keep derived state out of the durable log** — Disk Janitor's in-memory plan state should be rebuildable purely from the JSONL.
- **SQLite rollback journal** (for contrast): header is padded to a full sector "so that if a power loss occurs while writing the sector, information that follows the header will be (hopefully) undamaged"; a journal is committed by *delete / truncate-to-zero / overwrite-header* — three commit styles mirroring `journal_mode` DELETE/TRUNCATE/PERSIST [16]. The "overwrite the magic header" trick is a cheap commit marker for a WAL you want to invalidate wholesale.
- **git**: content-addressed objects (name = SHA-1 of `"<type> <len>\0" + content`) make every write verifiable by recomputation; packfiles end with a trailer checksum of the whole pack [21][22]. Lesson for the ledger: store per-record CRC in-band (git-style verify-on-read) and optionally a file trailer CRC at compaction time.

### 8. Schema for the Disk Janitor operations ledger (proposal)

Two files in `~/.disk-janitor/`: `ledger.jsonl` (permanent history, the R5 台账) and `wal.jsonl` (crash-recovery journal, reset after each checkpoint).

Record envelope (all records): `{v, seq, ts, kind, crc32, rec}` — `seq` monotonic per file, `ts` RFC3339, `crc32` over canonical `rec` serialization (crc32fast), `prev_crc` optional chain.

| `kind` | `rec` payload (fields) |
|---|---|
| `scan_result` | `root`, `files_scanned`, `total_bytes`, `duration_ms`, `session_id` |
| `verdict` | `path_hash` (never raw path for privacy parity with LLM invariant — raw path stays local), `size`, `grade` (PureCache/Rebuildable/Redownloadable/Expensive/HandsOff), `evidence: [{kind, detail, strength}]` |
| `plan` | `plan_id`, `entries: [{path_hash, size, grade, action}]`, `policy_snapshot` |
| `apply` | `plan_id`, `entry`, `original_path`, `quarantine_path`, `renamex_np: true` |
| `purge` | `plan_id`, `entry`, `bytes_freed` (typed confirm echoed), `quarantine_path` |
| `restore` | `plan_id`, `entry`, `from quarantine_path`, `to original_path`, `undoes: <apply seq>` |
| `undo_record` (embedded in apply) | `original_path`, `quarantine_path`, `byte_count`, `grace_expires_at` (ts + 72 h) |

Recovery rules on startup: (1) replay `ledger.jsonl` skipping corrupt lines (§5); (2) replay any tail of `wal.jsonl` — an `apply`/`purge`/`restore` record is *committed* only if its CRC validates; (3) incomplete operations found only in `wal.jsonl` are rolled forward if idempotent (restore/apply both re-runnable: rename fails benignly with ENOENT if already done) or rolled back by deleting the WAL tail; (4) verify `grace_expires_at` — expired quarantines become purge candidates. WAL append protocol per operation: serialize record + CRC → `write_all` to `wal.jsonl` → `sync_data()` (F_FULLFSYNC on Apple) → only then perform the actual filesystem mutation → append committed-result record → after N records (or session end), compact WAL into `ledger.jsonl` via the temp-file dance + dir fsync [15][16][18].

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| Stebalien/tempfile | MIT OR Apache-2.0 (Cargo.toml `license` field, tag 3.27.0) [8] | atomic-rename persist via `renameat_with(NOREPLACE)` incl. macOS path and fallbacks | `src/file/imp/unix.rs` (`persist`) [9] |
| srijs/rust-crc32fast | MIT OR Apache-2.0 (Cargo.toml) [8] | SIMD + table CRC32 used as the frame checksum | whole crate; no repo code to copy |
| al8n/fs4 | MIT OR Apache-2.0 (Cargo.toml, fs4 1.1.0) [8] | flock wrapper shape, `TryLockError`, async adapters | `src/unix.rs` (`lock_impl!` → `rustix::fs::flock`) [10] |
| danburkert/fs2-rs | MIT/Apache-2.0 (Cargo.toml 0.4.3; LICENSE-MIT header) [8] | reference flock semantics doc-comment ("locks are advisory", dup/fork caveats) | `src/os/unix.rs`, `src/lib.rs` doc [12] |
| nix-rust/nix | MIT (docs.rs crate page; Cargo.toml `license = "MIT"`) [8] | `Flock` RAII guard, `FcntlArg::F_FULLFSYNC` / `F_BARRIERFSYNC` bindings on apple targets | `src/fcntl.rs` [11] |
| sqlite/sqlite | Public domain (copyright.html: "SQLite Is Public Domain") [26] | recovery algorithm (stop at first invalid checksum), WAL frame format, dir-fsync discipline | `src/os_unix.c` (`full_fsync`, `openDirectory`, DIRSYNC), `walformat.html` [16][18] |
| git/git | GPL-2.0-only (COPYING) — **copy-with-care: read as design reference only, do not import code** | content-addressing + trailer-checkpoint design ideas | `Documentation/gitformat-pack.adoc` [21][22] |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| `tempfile` 3.27.0 | atomic temp file + rename for WAL compaction & snapshot replace | `persist_noclobber` is atomic on Apple via renameat_with since 3.18.0; docs stress it does NOT fsync for you [7][9] | hand-rolled libc `renamex_np` |
| `crc32fast` 1.5.1 (2026-08-22) | per-record CRC32 framing | SIMD-accelerated, streaming `Hasher` API [8] | `crc` crate (more variants, slower) |
| `nix` 0.31.3 | `fcntl(F_FULLFSYNC)`, `Flock`, `renameat` | MIT; apple-target bindings for F_FULLFSYNC/F_BARRIERFSYNC confirmed in `src/fcntl.rs` [11][8] | raw `libc` |
| `libc` 0.2.187 stable (1.0.0-alpha.4 is pre) | constants: `F_FULLFSYNC=51`, `F_BARRIERFSYNC=85`, `RENAME_EXCL=4`, `renamex_np` [14] | only if a nix API is missing (e.g. `renamex_np` is NOT in nix 0.31.3 — only `renameat2` on linux-gnu) | — |
| `fs4` 1.1.0 (or `fs2` 0.4.3) | advisory file lock | fs4 = maintained fs2 fork on rustix; sync feature MSRV 1.75; flock(2) under the hood [10][12][13] | `nix::fcntl::Flock` |
| `serde_json` 1.0.151 | JSONL record serialization | canonical `to_vec` for CRC input; `serde` 1.0.229 [8] | — |
| `fs-err` 3.3.1 (optional) | better io::Error context on ledger paths | MIT OR Apache-2.0 [8] | manual error wrapping |
| — do NOT pick: `sled` 1.0.0-alpha.124 | embedded KV | alpha line stalled since 2024-10; ledger is simple enough to own [8] | own JSONL (recommended) |

## Risks & open questions

1. **F_FULLFSYNC latency vs crash window**: ~4 ms/sync on Apple SSDs [25]; per-operation WAL sync at 10k files/scan ≈ 40 s pure sync. Open question: is batching WAL syncs per *operation group* (apply/purge/restore step) acceptable, or must every apply be individually synced? Proposal: sync at every state *transition*, not every record.
2. **Cross-file ordering on APFS**: SQLite 3.46.1 needed a directory sync *between* two fullfsyncs [20]. Our protocol has the same shape (WAL sync → FS mutation → snapshot replace); open question whether one parent-dir fsync after each mutation suffices in practice — needs a fault-injection test (kill -9 + dtrace/`FSInOut` harness).
3. **External volumes**: `renamex_np`/RENAME_EXCL and F_FULLFSYNC support on exFAT/FAT USB sticks is partial (FAT: fullfsync yes per [2]; RENAME_EXCL capability-probed [3]). Quarantine must be same-volume (SPEC §7); WAL must live on the *same volume as the quarantine* or recovery is impossible after crash.
4. **flock lifetime bugs**: duplicated fds share one flock; a forked child unlocking kills the parent's lock [4][12] — irrelevant for a single-process TUI but document it for the agent loop's worker threads.
5. **`seq` gap policy**: crash between WAL append and mutation means a committed WAL record describes an operation that never ran. Replay must reconcile WAL records against observed FS state (does `quarantine_path` exist?) — cheap idempotency check, must be spec'd before M2.
6. **ledger privacy vs undo**: `undo_record.original_path` must store the real path (restore needs it) while verdict records use `path_hash`; confirm this asymmetry is acceptable under the privacy invariant (it is local-only, LLM never reads the ledger).

## Sources

1. `fsync(2)` — Apple system calls manual (via keith.github.io/xcode-man-pages): fsync caveat + F_FULLFSYNC recommendation — https://keith.github.io/xcode-man-pages/fsync.2.html — accessed 2026-08-30
2. `fcntl(2)` — Apple system calls manual: F_FULLFSYNC (implemented on HFS/FAT/UDF/APFS), F_BARRIERFSYNC two-phase barrier semantics — https://keith.github.io/xcode-man-pages/fcntl.2.html — accessed 2026-08-30
3. `rename(2)` — Apple system calls manual: renamex_np/renameatx_np, RENAME_EXCL (EEXIST), RENAME_SWAP, VOL_CAP_INT_RENAME_EXCL, crash-atomicity guarantee — https://keith.github.io/xcode-man-pages/rename.2.html — accessed 2026-08-30
4. `flock(2)` — Apple system calls manual: LOCK_SH/EX/NB/UN, advisory semantics, locks-on-open-file-description, EWOULDBLOCK — https://keith.github.io/xcode-man-pages/flock.2.html — accessed 2026-08-30
5. rust-lang/rust `library/std/src/sys/fs/unix.rs` (master): `os_fsync`/`os_datasync` → `fcntl(fd, F_FULLFSYNC)` under `target_vendor = "apple"` — https://raw.githubusercontent.com/rust-lang/rust/master/library/std/src/sys/fs/unix.rs — accessed 2026-08-30
6. rust-lang/rust PR #60121 "Fix sync_all on macos/ios" (merged 2019): sync_all/sync_data now use F_FULLFSYNC on Apple — https://github.com/rust-lang/rust/pull/60121 — accessed 2026-08-30
7. tempfile docs — NamedTempFile::persist / persist_noclobber ("atomically replace", "neither the file contents nor the containing directory are synchronized") — https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html — accessed 2026-08-30
8. crates.io API (crates.io/api/v1/crates/…, fetched 2026-08-30): tempfile 3.27.0, crc32fast 1.5.1, fs2 0.4.3, fs4 1.1.0, nix 0.31.3, libc 0.2.187 (stable line; 1.0.0-alpha.4 pre), serde_json 1.0.151, serde 1.0.229, rustix 1.1.4, fs-err 3.3.1, sled 1.0.0-alpha.124; licenses from Cargo.toml `license` fields and crates.io version metadata
9. tempfile `src/file/imp/unix.rs` (3.27.0): persist → `renameat_with(NOREPLACE)` on Apple, fallback hard_link+unlink — https://docs.rs/tempfile/latest/src/tempfile/file/imp/unix.rs.html — accessed 2026-08-30
10. fs4 `src/unix.rs`: lock_impl! macro → `rustix::fs::flock(LockExclusive/NonBlocking…)` — https://raw.githubusercontent.com/al8n/fs4/master/src/unix.rs — accessed 2026-08-30
11. nix `src/fcntl.rs` (master): `FcntlArg::F_FULLFSYNC`/`F_BARRIERFSYNC` (apple_targets), `FlockArg`, deprecated `flock()` + `Flock` RAII — https://raw.githubusercontent.com/nix-rust/nix/master/src/fcntl.rs — accessed 2026-08-30
12. fs2 `src/os/unix.rs` (0.4.3): `libc::flock` implementation; lib.rs advisory-lock caveats — https://docs.rs/fs2/0.4.3/src/fs2/os/unix.rs.html and https://docs.rs/fs2/0.4.3/src/fs2/lib.rs.html — accessed 2026-08-30
13. fs4 docs.rs: feature list (sync/async runtimes), MSRV 1.75 sync, platform notes, MIT/Apache copyright 2015 Dan Burkert / 2026 Al Liu — https://docs.rs/fs4/latest/fs4/ — accessed 2026-08-30
14. libc `src/unix/bsd/apple/mod.rs` (0.2.187, local registry verified): `F_FULLFSYNC=51`, `F_BARRIERFSYNC=85`, `F_OFD_SETLK/LKW/GETLK=90/91/92`, `RENAME_SWAP=2`, `RENAME_EXCL=4`, `renamex_np`, `renameatx_np`, `LOCK_SH/EX/NB/UN=1/2/4/8` — https://docs.rs/libc/latest/x86_64-apple-darwin/libc/constant.F_FULLFSYNC.html (doc mirror) — accessed 2026-08-30
15. SQLite file format §4 "The Write-Ahead Log", §4.1 WAL File Format, §4.2 Checkpoint Algorithm — https://sqlite.org/fileformat2.html — accessed 2026-08-30
16. SQLite WAL-mode file format (walformat.html): frame validity (salt + cumulative checksum), §3 Recovery single-pass scan stopping at first invalid checksum, shm never fsynced, WAL_RECOVER_LOCK — https://sqlite.org/walformat.html — accessed 2026-08-30
17. SQLite rollback journal format §3 (journal header sector-padding, three commit styles DELETE/TRUNCATE/PERSIST, checksum nonce anti-stale-data) — https://sqlite.org/fileformat2.html (§3) — accessed 2026-08-30
18. sqlite `src/os_unix.c` (version-3.53.1 & master): `full_fsync` F_FULLFSYNC-with-fsync-fallback, `openDirectory` comment ("fsync() a directory to make sure file creation and deletion events are flushed"), `UNIXFILE_DIRSYNC` set for new journals — https://raw.githubusercontent.com/sqlite/sqlite/version-3.53.1/src/os_unix.c — accessed 2026-08-30
19. Rust JSONL torn-tail practice: omni-core `shared/jsonl.rs` (repair_tail, read_checked) and sift_core::fsck (TruncatedTail via read_until, `BufReader::lines` silent-merge hazard) — https://docs.rs/omni-core/latest/src/omni_core/shared/jsonl.rs.html ; https://docs.rs/siftcore/latest/sift_core/fsck/index.html — accessed 2026-08-30
20. DB News: "SQLite 3.46.1 Backport Patches WAL Corruption on APFS With fullfsync=1" (APFS reorders F_FULLFSYNC between DB and WAL; fix = directory sync between them) — https://db-news.com/sqlite-3-46-1-backport-patches-wal-corruption-on-apfs-with-fullfsync1 — accessed 2026-08-30
21. Pro Git 2nd ed., Git Internals — Git Objects: content-addressed storage, SHA-1 over header+content — https://git-scm.com/book/en/v2/Git-Internals-Git-Objects — accessed 2026-08-30
22. gitformat-pack: "The trailer records a pack checksum of all of the above" — https://git-scm.com/docs/gitformat-pack — accessed 2026-08-30
23. arXiv 2511.18323 "Crash-Consistent Checkpointing for AI Training on macOS/APFS": file-sync vs file-sync+dir-sync durability/latency on APFS — https://arxiv.org/html/2511.18323v1 — accessed 2026-08-30
24. Lactose Intolerant: "fsync may not be enough to guarantee data durability on Mac OS" (corroboration) — https://lactoseintolerant.dev/fsync-may-not-be-enough-to-guarantee-data-durability-on-mac-os/ — accessed 2026-08-30
25. coordinode-lsm-tree PR #374 perf notes: F_FULLFSYNC ~3.9 ms vs ~0.075 ms plain fsync on macOS; SyncMode Normal/Full — https://github.com/structured-world/coordinode-lsm-tree/pull/374 — accessed 2026-08-30
26. SQLite copyright page: "SQLite Is Public Domain" — https://sqlite.org/copyright.html — accessed 2026-08-30
