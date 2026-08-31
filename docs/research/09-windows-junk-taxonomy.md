# Windows-Junk Taxonomy — Research Note

Canonical taxonomy of Windows machine disk junk: per-path locations (env-var form), official
regen/cleanup command + rebuild cost, risk tier (safe / rebuildable / re-downloadable / expensive /
hands-off per docs/SPEC.md), and distinguishing evidence. Mirrors docs/research/03-dev-junk-taxonomy.md
(macOS). · researched 2026-08-30

## Findings

### 0. Cross-cutting evidence signals (Windows, feeds classify/ engine)

Same engine contract as macOS [03 §0], but the *implementation substrate* differs:

| Signal | How to read it on Windows | Verdict impact |
|---|---|---|
| **Temp resolution order** | `GetTempPathW` checks `TMP` → `TEMP` → `USERPROFILE` → Windows dir, first hit wins [S1] | never assume `%TEMP%` = `%LOCALAPPDATA%\Temp`; resolve via API for the "actively written" gate |
| **File locks** | No `lsof` equivalent. Probe = `CreateFileW(GENERIC_READ, FILE_SHARE_READ\|WRITE\|DELETE)`; `ERROR_SHARING_VIOLATION` ⇒ held by a live process [S2] | any sharing violation ⇒ refuse auto-action; optional: Restart Manager API enumerates lockers |
| **NTFS hard links** | `BY_HANDLE_FILE_INFORMATION.nNumberOfLinks > 1` (WinSxS hard-links binaries system-wide [S3]; pnpm/uv hard-link stores) | freed-byte estimate must count unique extents; deleting one of N links frees 0 bytes |
| **Reparse points** | `FILE_ATTRIBUTE_REPARSE_POINT` + tag (`IO_REPARSE_TAG_MOUNT_POINT` = junction, symlink) | walker MUST NOT follow junctions (cycles, other volumes, `\?\` escapes); treat junction target as outside scan root |
| **OneDrive dataless files** | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `FILE_ATTRIBUTE_OFFLINE` (§5) | stat-only scan; opening content triggers a recall download; classify separately, never "purge" |
| **Long-path risk** | classic MAX_PATH 260; long paths opt-in since Win10 1607, `\\?\` prefix for 32,767 wchar [S4] | manifest paths must carry the `\\?\` form when >260; rename targets validated |
| **mtime recency** | unchanged rule (mtime < 10 min ⇒ refuse) | Windows quirk: Storage Sense may already have deleted %TEMP% files behind the app's back [S5] — mtime evidence is *floor*, not guarantee |
| **git presence** | unchanged (`.git` ancestor ⇒ RebuildableFromSource prior) | same as macOS lane |

### 1. OS-managed system junk (no macOS analog — the big Windows deltas)

| Path | What it is | Official cleanup / regen | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `%SystemRoot%\SoftwareDistribution\Download` | Windows Update download cache (BITS/WU payloads); `...\DataStore` holds the WU database | Microsoft's own reset sequence: `net stop wuauserv` → delete/rename folder → `net start wuauserv`; WU re-downloads content on next scan [S6] | Re-downloadable | dir sits under `%SystemRoot%`; contains hash-named `.cab`/`.psf` payloads + `DataStore.edb`; only meaningful while WU idle |
| Delivery Optimization cache | P2P/HTTP update cache shared by WU, Store, Defender, Office, Edge updates (client default download mode 1) [S7] | `Delete-DeliveryOptimizationCache` PowerShell cmdlet (`DeliveryOptimization` module; `-File`, `-Force`, `-IncludePinnedFiles`) [S8]; cleanmgr "Delivery Optimization Files" handler | Re-downloadable | DO self-expires cache entries; cmdlet is the only supported purge — never file-level delete (files pinned by DO service) |
| `C:\Windows\WinSxS` | Component Store: all component versions for servicing/rollback/update; "isn't as large as it appears" — binaries hard-linked out into system dirs [S3] | **NEVER manual delete** — "Deleting files from the WinSxS folder … may severely damage your system so that your PC might not boot and make it impossible to update" [S9]. Tool-gated only: `Dism.exe /online /Cleanup-Image /StartComponentCleanup` (immediate superseded-version removal); add `/ResetBase` (removes ALL superseded versions; existing updates no longer uninstallable); Task Scheduler `\Microsoft\Windows\Servicing\StartComponentCleanup` (30-day grace, 1 h timeout); cleanmgr "Windows Update Cleanup" [S9] | Hands-off (files) / tool-invocation | dir under `%SystemRoot%`, `WinSxS` name + `Manifests\`/`Temp\` layout; `nNumberOfLinks > 1` dominates; repaired via `DISM /RestoreHealth` + `sfc /scannow` [S6] |
| `C:\Windows\Installer` | Windows Installer cache: `.msi`/`.msp` + `.spl` state needed for uninstall/repair/update of every MSI product | **None — should not be deleted.** "If the installer cache is compromised, you may not immediately see problems until you take an action such as uninstalling, repairing, or updating"; missing files unique per machine; recovery can require full OS rebuild (MSI errors 1612/1620/1635/1636/1642/1706/1714) [S10] | Hands-off | hidden `%SystemRoot%\Installer`; random `.msi`/`.msp` names (no product mapping); size grows monotonically with installed software |
| `C:\Windows.old` (+ `$WINDOWS.~BT`, `$WINDOWS.~WS`) | previous Windows installation + upgrade artifacts after feature update | Settings → System → Storage → Temporary files / Cleanup recommendations → "Previous Windows installation(s)" [S11]; `cleanmgr /autoclean` deletes upgrade leftovers [S12]; OS auto-deletes after 10 days [S11] | Expensive (deletion kills rollback; irreversible) | top-of-drive `Windows.old`; only exists within 10-day post-upgrade window; admin-required; never file-level move — tool-gated only |

### 2. Temp dirs, caches, diagnostics (platform-managed, user-writable)

| Path | What it is | Official cleanup / regen | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `%TEMP%` = `%LOCALAPPDATA%\Temp` (user); `%SystemRoot%\Temp` (system, admin) | app scratch space; resolution order TMP→TEMP→USERPROFILE→Windows dir [S1] | cleanmgr "Temporary Files": "safely delete temporary files that haven't been modified within the last week" [S12]; Storage Sense "temporary files" toggle deletes user-Temp items by age [S5] | Safe (with age gate) | recycle-in-place always same volume; .tmp suffixes + random names; live app handles possible ⇒ lock probe first; system Temp requires admin |
| `%SystemRoot%\Prefetch` (`*.pf`) | prefetch trace files + `Layout.ini`; SuperFetch scenario `.db` records [S13]; `OperationStart`/`OperationEnd` record app-level traces into `filename.pf`, LRU-capped [S14] | no official purge command exists (regen = OS re-traces app launches); third-party "prefetch cleaner" tools are unsupported folklore | Rebuildable (slow start-ups for a while) — treat as delete-discouraged | `*.pf` naming `<EXE>-<hash>.pf`; only meaningful on HDD-era systems; harmless to keep |
| `%LOCALAPPDATA%\CrashDumps` | WER LocalDumps default `DumpFolder` (DumpCount 10, DumpType minidump default; full dumps via `DumpType=2`) [S15] | none needed — pure diagnostic output; delete aged dumps | Safe | `*.dmp` files named after crashing processes; also `%WINDIR%\System32\Config\SystemProfile` and `%WINDIR%\ServiceProfiles` for service crashes [S15] |
| `%LOCALAPPDATA%\Microsoft\Windows\WER\ReportArchive` + `ReportQueue` | queued/archived WER reports (orphaned when reporting disabled) | WER purges per policy; no official user purge cmd — safe to archive-prune | Safe | `Report.wer` files; queue dirs hold undelivered reports |
| `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db` (+ `%LOCALAPPDATA%\Microsoft\Windows\INetCache` leftovers) | Explorer thumbnail DB (thumbcache_256/1024/…; iconcache_*.db) | cleanmgr "Thumbnails" handler (VolumeCaches key `Thumbnail Cache`) [S16] | Rebuildable (thumbs regenerate on folder visit) | `thumbcache_*.db`/`iconcache_*.db` naming; Explorer locks them while running ⇒ delete at boot or via handler |
| `%LOCALAPPDATA%\Microsoft\Windows\INetCache`, `WebCache` (`%LOCALAPPDATA%\Microsoft\Windows\WebCache*`) | IE-legacy + WebView2/SmartScreen-adjacent caches; `WebCacheV01.dat` ESE database still used by shell services | no official per-file purge command; Storage Sense "temporary files" is the supported path [S5] | Safe (cache-only children), Hands-off (`WebCacheV01.dat` while explorer.exe alive) | `INetCache\IE\…` 8-char random dirs; ESE `.dat` files lock-checked |

### 3. Cloud-content tier: OneDrive Files On-Demand placeholders

| Path | What it is | Official cleanup / regen | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `%USERPROFILE%\OneDrive\…` online-only files | Files On-Demand placeholders: blue-cloud files occupy ~0 bytes locally; opening downloads content. On-Demand default-on since OneDrive build 23.066, requires Win10 1709+ [S17] | regen = right-click → "Free up space" (dehydration; official verb) or Storage Sense cloud-content dehydration threshold (0–365 days policy `ConfigStorageSenseCloudContentDehydrationThreshold`) [S5][S17] | Expensive to *delete* (deletion propagates to cloud recycle bin); "free up space" = Safe dehydration | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`/`FILE_ATTRIBUTE_OFFLINE`/pinned+unpinned attributes; logical `st_size` ≫ allocated — byte-count honesty requires allocated size; NEVER move placeholders across volumes (forces full recall) |

Scan-side note: stat-only traversal is mandatory for the OneDrive subtree; the "orphan placeholder with no account" case (OneDrive uninstalled, `CloudFiles` reparse points remain) surfaces as user-visible prompt, not auto-delete.

### 4. Developer ecosystems on Windows (paths differ from macOS; semantics mostly carry over)

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `%LocalAppData%\npm-cache` | npm content-addressable cache (`cacache`); npm cache `Default: Windows: %LocalAppData%\npm-cache`, Posix `~/.npm`; self-healing, "npm will not remove data by itself: the cache will grow as new packages are installed" [S18] | `npm cache verify` (GC + integrity) / `npm cache clean --force`; regen = transparent re-download | Safe (PureCache) | `_cacache/content-v2/sha512/…` layout identical to macOS; `npm cache npx ls/rm` for the npx cache [S18] |
| `%LocalAppData%\pnpm\store` | pnpm global store (Windows default `~/AppData/Local/pnpm/store` unless `$PNPM_HOME`/`$XDG_DATA_HOME` set) [S19] | `pnpm store prune` — "not harmful and has no side effects on your projects"; re-downloads if needed [S20] | Safe (PureCache) | store must sit on same disk as installs (hardlinks); non-home disks ⇒ `<disk>\.pnpm-store`; per-disk store rule [S19] |
| `<globalFolder>\cache\cache.zip` under `%LOCALAPPDATA%\Yarn\Berry` (Yarn Berry default global: `globalFolder: "${HOME}/.yarn/berry"` — env-expanded; `enableGlobalCache: true` default writes cache into globalFolder) [S21] | zip-compressed package cache | re-fetch on `yarn install` | Safe (PureCache) | `.yarnrc.yml` `globalFolder`/`cacheFolder`/`enableGlobalCache` decide location — parse them; zero-install repos commit `.yarn/cache` ⇒ git check first (same as macOS) [S21] |
| `%LocalAppData%\pip\Cache` (pip ≥26.2: "Windows: %LocalAppData%\pip\Cache") | HTTP-v2 + built-wheel cache [S22] | `pip cache purge` / `pip cache remove <pkg>` | Safe (PureCache) | `http-v2\` + `wheels\` subdirs; locally-built wheels are the expensive-to-regen slice [S22] |
| `%LOCALAPPDATA%\uv\cache` (uv: "system-appropriate cache directory … `%LOCALAPPDATA%\uv\cache` on Windows") | versioned buckets (`archive-v0`, `wheels-v6`, …); hardlinks into venvs; "it's *never* safe to modify the cache directly" [S23] | `uv cache clean [pkg]`; `uv cache prune` = "safe to run periodically" [S23] | Safe (PureCache) | bucket-dir naming; `CACHEDIR.TAG`; same-volume linking requirement [S23] |
| `<proj>\target\` | cargo build artifacts (`target/debug`, `target/release`, `debug/incremental`, `deps`, `build`); `CARGO_TARGET_DIR`/`build.target-dir` relocate [S24] | `cargo build` (minutes, dep-scaled) | RebuildableFromSource | sibling `Cargo.toml` + `Cargo.lock`; `target/debug/.fingerprint` layout — same as macOS lane |
| `%USERPROFILE%\.cargo\{registry,git}` (`CARGO_HOME`) | registry index/cache/src + git db/checkouts; "you can always remove any part of the cache and Cargo will do its best to restore sources" [S25] | auto re-fetch/re-extract on next build | Safe (PureCache for registry/git) — but `bin/`, `.crates.toml`, `.crates2.json`, `credentials.toml` = NOT cache [S25] | `registry/cache/*.crate` vs unpacked `registry/src`; installed-tool binaries must be excluded from any purge |
| `%USERPROFILE%\.gradle\caches\{modules-2, transforms-3, build-cache-1, jars-9}`, `wrapper\dists\` | Gradle User Home caches (`C:\Users\<USERNAME>\.gradle` default; `GRADLE_USER_HOME` override) [S26] | Gradle self-cleans: unused release distros after 30 d inactivity, downloaded resources 30 d, created resources/build-cache 7 d [S26] | Safe (PureCache) | trailing format-version numbers in dir names; version-specific caches per Gradle release |
| `<proj>\build\` + `<proj>\.gradle\` | project build output + project-local cache (`.gradle/<version>`, `configuration-cache`) [S26] | `gradlew build` re-runs; `.gradle/<version>` self-deletes after 7 d inactivity [S26] | RebuildableFromSource | `gradlew`/`settings.gradle.kts` siblings |
| `%USERPROFILE%\.m2\repository` | Maven local repository (default `${user.home}/.m2/repository`); "clean it out if you are short on disk space (or erase it completely if you are willing to download everything again)" [S27] | delete whole repo (or subdirs); `mvn` re-downloads | Safe (PureCache) | `groupId\artifactId\version\*.jar` layout; `settings.xml` `<localRepository>` can relocate [S27] |
| `%USERPROFILE%\.nuget\packages` (global-packages); `%localappdata%\NuGet\v3-cache` (http-cache); `%temp%\NuGetScratch` | NuGet per-machine folders; exact env-var/MSBuild override table in official doc [S28] | `dotnet nuget locals all --clear` / per-folder `--clear` [S28] | Safe (PureCache) — NuGet docs: clearing is for troubleshooting/forcing remote fetch; cleanup tool must delete `.nupkg.metadata` first if doing selective pruning [S28] | `dotnet nuget locals all --list` prints resolved locations — the classify engine should shell out rather than guess [S28] |
| `%LOCALAPPDATA%\Docker\wsl\data\docker_data.vhdx` | Docker Desktop (WSL2 backend) VM disk: images, containers, volumes, build cache; default data dir `C:\Users\[USERNAME]\AppData\Local\Docker\wsl` [S29] | images re-pull; containers/volumes NOT regenerable; reclaim = `docker system prune` inside (stopped containers, unused networks, dangling images, build cache; anonymous volumes only with `--volumes`) [S30] | Hands-off as a file (like macOS Docker.raw) | `.vhdx` backing a WSL distro named `docker-desktop`; logical size ≫ allocated (sparse-ish); backup/restore doc names the exact file [S29] |
| `%LOCALAPPDATA%\Packages\<PackageFamilyName>\LocalState\ext4.vhdx` (per WSL distro; resolved via `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss` `BasePath`) | WSL2 distro root filesystem (ext4); default VHD max 1 TB (512 GB before WSL 0.58.0) [S31] | distro content re-creatable only via `wsl --export`/`--import` (tar/vhdx) or `wsl --unregister` + reinstall [S32]; official warning: "do not modify, move, or access the WSL related files located inside of your AppData folder using Windows tools or editors" [S31] | Hands-off (user data) — space reclaim = shrink via diskpart compact / `wsl --manage --resize` only with distro down [S31] | `ext4.vhdx` naming; running distro holds the file exclusively (`ERROR_SHARING_VIOLATION`); registry `BasePath` lookup is the robust discovery path [S31] |

Notes: (a) cross-lane contracts from 03 carry over unchanged — lockfile-sibling logic, orphan promotion, mtime gate, content-addressable whole-store-only pruning; (b) every per-user dev path above sits under `%LOCALAPPDATA%`/`%USERPROFILE%` on `C:` by default ⇒ same-volume quarantine is satisfiable, but `%LOCALAPPDATA%` can be relocated — resolve via env, never hard-code `C:\Users\…`.

## Platform delta vs macOS

Carries over unchanged: the five-tier grading (safe / rebuildable / re-downloadable / expensive / hands-off per docs/SPEC.md), evidence-first classify engine, lockfile-sibling logic, orphan promotion, mtime < 10 min gate, content-addressable whole-store-only pruning (npm `_cacache`, pnpm store, uv buckets, NuGet global-packages, Yarn Berry cache), ledger/WAL crash-recovery design, TUI. npm cache layout is byte-identical to macOS (`_cacache/content-v2/sha512/…`, §4); Yarn zero-install `.yarn/cache` git check identical; Docker semantics identical (drive daemon commands, not files — §4).

Must be re-implemented:
- **Atomic quarantine move.** macOS `renamex_np(RENAME_EXCL)` → `SetFileInformationByHandle(FileRenameInfoEx, Flags)` with `FILE_RENAME_POSIX_SEMANTICS` (0x2) / `FILE_RENAME_REPLACE_IF_EXISTS` (0x1) — flag values verified from [MS-FSCC]. No-clobber = omit REPLACE_IF_EXISTS ⇒ rename fails when the target exists (the RENAME_EXCL analog); POSIX semantics additionally keep existing handles to a replaced file valid. Win10 1709+ gate per the note-11 platform decision ([MS-FSCC] itself states no explicit minimum version — enforce at runtime, see Risks). Fallback `MoveFileExW(MOVEFILE_WRITE_THROUGH)` for durability (XP+); `MOVEFILE_REPLACE_EXISTING` overwrites and is NOT no-clobber.
- **Same-volume check.** `BY_HANDLE_FILE_INFORMATION.dwVolumeSerialNumber` replaces `st_dev`; hardlink count `nNumberOfLinks` replaces `st_nlink` (§0).
- **File-lock evidence.** `CreateFileW` sharing-violation probe (§0) replaces lsof/flock; Restart Manager enumerates lockers when a locker must be named.
- **Walker.** Never follow reparse points (`IO_REPARSE_TAG_MOUNT_POINT` junctions/symlinks — cycles, other volumes, `\?\` escapes, §0); OneDrive subtree stat-only, never open placeholders (§3); manifest paths carry the `\\?\` form past 260 chars (§0). Do not traverse `\\wsl$` (virtual 9P view of ext4 — cross-volume moves are definitionally impossible, and the underlying ext4.vhdx is hands-off, §4).
- **Temp resolution.** `GetTempPathW` order (§0) — never assume a literal `%TEMP%`.
- **Scan roots.** `~/Library/Caches` → `%LOCALAPPDATA%`; `/Library/Caches` → `%ProgramData%`; plus OS-managed trees with no macOS analog (WinSxS, `C:\Windows\Installer`, `SoftwareDistribution\Download`, Delivery Optimization cache) that the verdict engine must treat as tool-invocation targets (`DISM /StartComponentCleanup`, cleanmgr handlers, `Delete-DeliveryOptimizationCache`), never as movable candidates (§1).

Impossible or weaker on Windows:
- No lsof-complete lock enumeration — Restart Manager/ETW approximates it.
- No APFS-clone CoW discount: NTFS has no per-file copy-on-write, so freed-byte estimates rely on hardlinks only.
- System-junk reclaim is inherently tool-gated; there is no safe file-level API for WinSxS/Installer contents — weaker than macOS, where similar caches are ordinary user files.
- Storage Sense race (§0): mtime evidence is a floor, not a guarantee.
- OneDrive placeholders must never be moved cross-volume — any cross-volume move forces a full recall download (§3).

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| tbillington/kondo (v0.9.0) | MIT (verified GitHub license field + crates.io, 2026-08-30) | known-project-structure junk discovery, Windows paths supported | `kondo-lib` |
| matthiaskrgr/cargo-cache (v0.8.3) | MIT/Apache-2.0 (verified in repo `Cargo.toml`; carried over from 03) | per-component `%USERPROFILE%\.cargo` size accounting — paths unchanged on Windows | `src/main.rs` |
| holmgr/cargo-sweep (v0.8.0) | MIT (verified in repo `Cargo.toml`; carried over from 03) | toolchain-keyed `target\` pruning rules | repo `src/` |

kondo's license was verified by this lane's GitHub/crates.io sweep on 2026-08-30; cargo-cache and cargo-sweep licenses are recorded in 03 and re-confirmed as path-identical on Windows. No other third-party cleanup code is reused.

## Recommended crates

Platform-gated usage: every item below is Windows-only surface — gate imports behind `#[cfg(windows)]` (the `windows`/`winreg` crates do not compile their Win32 modules on other targets; keep the cfg at the module boundary).

| Crate | Version | License | Purpose (Windows-gated) | Notes |
|---|---|---|---|---|
| `windows` | 0.62.2 | MIT OR Apache-2.0 | `SetFileInformationByHandle`/`FileRenameInfoEx`, `MoveFileExW`, `GetTempPathW`, `CreateFileW`, `BY_HANDLE_FILE_INFORMATION` | official microsoft-rs bindings; versions/licenses via crates.io API 2026-08-30 |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | raw-bindings alternative for the same calls | cheaper compile; same source of truth |
| `winreg` | 0.56.0 | MIT | enumerate `HKLM\…\Explorer\VolumeCaches` (cleanmgr handler discovery) and `HKCU\…\Lxss` `BasePath` (WSL vhdx locations) | thin registry wrapper |
| `same-file` | 1.0.6 | Unlicense/MIT | same-volume check for quarantine moves | wraps `BY_HANDLE_FILE_INFORMATION` volume-serial compare |
| `jwalk` | 0.9.0 | MIT | parallel walks with metadata | same as macOS lane |
| `walkdir` | 2.5.0 | Unlicense/MIT | serial walks with metadata | same as macOS lane |

## Risks & open questions

1. **FileRenameInfoEx availability is not explicitly documented.** [MS-FSCC] defines the flags but states no minimum OS version; the winbase `FILE_RENAME_INFO` page likewise omits it. Note 11's Win10 1709+ decision must therefore be enforced at runtime (probe once at startup; fall back to `MoveFileExW`), not assumed. Whether POSIX-semantics rename covers directory targets is also undocumented.
2. **VolumeCaches handler names are only partially official.** The registry mechanics (`HKLM\…\VolumeCaches`, `StateFlagsNNNN`, `DataDrivenCleaner`) are documented [S7][S8]; specific handler keys ("Windows Update Cleanup", "Delivery Optimization Files", "Thumbnail Cache") are corroborated via community/VDI sources but not enumerated in a formal spec. Enumerate the registry at runtime; never hard-code the list.
3. **OS-managed junk must stay tool-gated.** WinSxS and `C:\Windows\Installer` have no safe file-level API; the product may only wrap official commands (`DISM /StartComponentCleanup`, cleanmgr, `Delete-DeliveryOptimizationCache`). Windows.old deletion is rollback-destructive and irreversible — never auto-propose, only tool-gated invocation.
4. **`docker_data.vhdx` does not shrink after `docker system prune`.** Docker documents prune semantics and the vhdx path, but no official in-Docker compaction flow; diskpart `compact vdisk`/`Optimize-VHD` are not documented by Docker for this file. Treat compaction as an open question; surface "reclaim" only via daemon-side prune.
5. **WSL2 vhdx shrink paths are only partially documented.** `wsl --manage --resize` and export/import are official; diskpart compaction is not in the WSL docs. Shrinking requires the distro down (`ERROR_SHARING_VIOLATION` otherwise) — a live distro makes any plan invalid.
6. **cleanmgr/Storage Sense race.** Storage Sense may delete `%TEMP%` content between scan and apply [S5]; re-verify evidence at apply time, and treat its cleanup of `Windows.old` as a competing writer to any rollback-affecting manifest.
7. **Thumbnail-cache file paths** (`thumbcache_*.db`/`iconcache_*.db` under `%LOCALAPPDATA%\Microsoft\Windows\Explorer`) are widely observed but have no fetched primary source; the official cleanup path is the cleanmgr "Thumbnails" handler [S8]. File-level targeting needs in-house verification.
8. **Prefetch has no official purge command.** Any "prefetch cleaner" behavior is unsupported folklore [S13]; classify as delete-discouraged rather than proposing deletion.

## Sources

All fetched/verified 2026-08-30. Numbered to match the [S#] markers used in Findings.

1. S1 — GetTempPathW (fileapi.h): TMP → TEMP → USERPROFILE → Windows-dir resolution order — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw (accessed 2026-08-30)
2. S2 — File locks: CreateFileW sharing modes / `ERROR_SHARING_VIOLATION` — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew; Restart Manager portal (locker enumeration) — https://learn.microsoft.com/en-us/windows/win32/rstmgr/restart-manager-portal (accessed 2026-08-30)
3. S3 — Manage the Component Store (WinSxS hard links; "isn't as large as it appears") — https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/manage-the-component-store?view=windows-11 (accessed 2026-08-30)
4. S4 — Naming Files, Paths, and Namespaces (MAX_PATH 260; long paths opt-in Win10 1607; `\\?\` = 32,767 wchar) — https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file (accessed 2026-08-30)
5. S5 — Storage Sense (temp cleanup, cadence, policies) — https://learn.microsoft.com/en-us/windows/client-management/manage-storage-sense-windows-10 and https://learn.microsoft.com/en-us/windows/configuration/storage/storage-sense (accessed 2026-08-30)
6. S6 — Windows Update common errors & mitigation (SoftwareDistribution reset sequence; DISM `/RestoreHealth` + `sfc /scannow`) — https://learn.microsoft.com/en-us/troubleshoot/windows-client/installing-updates-features-roles/common-windows-update-errors and https://learn.microsoft.com/en-us/troubleshoot/windows-client/installing-updates-features-roles/additional-resources-for-windows-update (accessed 2026-08-30)
7. S7 — What is Delivery Optimization? (client default DownloadMode 1) — https://learn.microsoft.com/en-us/windows/deployment/do/waas-delivery-optimization (accessed 2026-08-30)
8. S8 — Delete-DeliveryOptimizationCache cmdlet (DeliveryOptimization module; `-FileId`, `-Force`, `-IncludePinnedFiles`) — https://learn.microsoft.com/en-us/powershell/module/deliveryoptimization/delete-deliveryoptimizationcache?view=windowsserver2025-ps (accessed 2026-08-30)
9. S9 — Clean up the WinSxS folder ("may severely damage…" warning; `DISM /StartComponentCleanup` + `/ResetBase`; StartComponentCleanup task 30-day grace / 1 h timeout) — https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/clean-up-the-winsxs-folder?view=windows-11 (accessed 2026-08-30)
10. S10 — Missing Windows Installer cache requires a computer repair ("If the installer cache is compromised…"; MSI errors 1612/1620/1635/1636/1642/1706/1714) — https://learn.microsoft.com/en-us/troubleshoot/windows-client/application-management/missing-windows-installer-cache (accessed 2026-08-30)
11. S11 — Delete your previous version of Windows (Windows.old; Settings → Cleanup recommendations; 10-day auto-delete) — https://support.microsoft.com/en-us/windows/deployment/install-upgrade/delete-your-previous-version-of-windows; KB5012334 (Storage Sense / Cleanup recommendations flow) — https://support.microsoft.com/en-us/topic/kb5012334-delete-the-windows-old-folder-using-storage-sense-in-the-settings-app-e12f9d84-ad7f-4780-9406-465670157f8e (accessed 2026-08-30)
12. S12 — cleanmgr (windows-commands): `/autoclean` "files left behind after you upgrade Windows"; "safely delete temporary files that haven't been modified within the last week" — https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cleanmgr (accessed 2026-08-30)
13. S13 — SuperFetch / scenario `.db` records under `%SystemRoot%\Prefetch` (Technet Magazine archive) — https://learn.microsoft.com/en-us/previous-versions/technet-magazine/cc162480(v=msdn.10) (accessed 2026-08-30)
14. S14 — OperationStart (winbase.h): `.pf` prefetch recording, LRU cap, Superfetch service on Win8 — https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-operationstart (accessed 2026-08-30)
15. S15 — Collecting User-Mode Dumps (LocalDumps: `DumpFolder` default `%LOCALAPPDATA%\CrashDumps`, `DumpCount` 10, `DumpType`; `%WINDIR%\System32\Config\SystemProfile`, `%WINDIR%\ServiceProfiles`) — https://learn.microsoft.com/en-us/windows/win32/wer/collecting-user-mode-dumps (accessed 2026-08-30)
16. S16 — Automating Disk Cleanup (VolumeCaches registry key + `StateFlagsNNNN`) — https://learn.microsoft.com/en-us/troubleshoot/windows-server/backup-and-storage/automating-disk-cleanup-tool; Disk Cleanup handler model (`DataDrivenCleaner`) — https://learn.microsoft.com/en-us/windows/win32/lwef/disk-cleanup (accessed 2026-08-30)
17. S17 — Save disk space with OneDrive Files On-Demand (online-only placeholders; "Free up space"; Win10 1709+; default-on since build 23.066) — https://support.microsoft.com/en-US/onedrive/save-disk-space-with-onedrive-files-on-demand-for-windows (accessed 2026-08-30)
18. S18 — npm cache (`cacache`; Windows default `%LocalAppData%\npm-cache`; `verify`/`clean --force`; `npm cache npx`) — https://docs.npmjs.com/cli/v11/commands/npm-cache (accessed 2026-08-30)
19. S19 — pnpm store settings (Windows default `~/AppData/Local/pnpm/store`; per-disk `.pnpm-store`) — https://pnpm.io/settings/store (accessed 2026-08-30)
20. S20 — pnpm store CLI (`prune`: "not harmful and has no side effects on your projects") — https://pnpm.io/cli/store (accessed 2026-08-30)
21. S21 — Yarn Berry `yarnrc.yml` (`enableGlobalCache`, `globalFolder` default `${HOME}/.yarn/berry`) — https://yarnpkg.com/configuration/yarnrc (accessed 2026-08-30)
22. S22 — pip caching (Windows `%LocalAppData%\pip\Cache`; http-v2 + wheels; `pip cache purge/remove`) — https://pip.pypa.io/en/stable/topics/caching/ (accessed 2026-08-30)
23. S23 — uv cache (buckets; "never safe to modify the cache directly"; `prune`/`clean`; `CACHEDIR.TAG`) — https://docs.astral.sh/uv/concepts/cache/ (accessed 2026-08-30)
24. S24 — Cargo Book, Build Cache reference (`target/` layout; `CARGO_TARGET_DIR` / `build.target-dir`) — https://doc.rust-lang.org/cargo/reference/build-cache.html (accessed 2026-08-30)
25. S25 — Cargo Book, Cargo Home (`CARGO_HOME` default; registry/git cache vs `bin/`, `.crates.toml`, `.crates2.json`, `credentials.toml`) — https://doc.rust-lang.org/cargo/guide/cargo-home.html (accessed 2026-08-30)
26. S26 — Gradle User Home & project dirs (caches; self-cleaning 30 d / 7 d retention; `GRADLE_USER_HOME`) — https://docs.gradle.org/current/userguide/directory_layout.html (accessed 2026-08-30)
27. S27 — Maven settings (`<localRepository>` default `${user.home}/.m2/repository`) — https://maven.apache.org/settings.html; local repository cleaning guidance — https://maven.apache.org/guides/introduction/introduction-to-repositories.html (accessed 2026-08-30)
28. S28 — NuGet: managing the global packages and cache folders (`%userprofile%\.nuget\packages`, `%localappdata%\NuGet\v3-cache`, `%temp%\NuGetScratch`; `dotnet nuget locals --clear`; `.nupkg.metadata` first) — https://learn.microsoft.com/en-us/nuget/consume-packages/managing-the-global-packages-and-cache-folders (accessed 2026-08-30)
29. S29 — Docker Desktop on WSL 2 (default data root `C:\Users\[USERNAME]\AppData\Local\Docker\wsl`) — https://docs.docker.com/desktop/features/wsl/; backup & restore (names `docker_data.vhdx` at `%LOCALAPPDATA%\Docker\wsl\data\`) — https://docs.docker.com/desktop/settings-and-maintenance/backup-and-restore/ (accessed 2026-08-30)
30. S30 — `docker system prune` CLI reference (stopped containers, unused networks, dangling images, build cache; anonymous volumes only with `--volumes`) — https://docs.docker.com/reference/cli/docker/system/prune/ (accessed 2026-08-30)
31. S31 — WSL disk space (`ext4.vhdx`; 1 TB default max, 512 GB before WSL 0.58.0; "do not modify, move, or access the WSL related files…"; `wsl --manage --resize`) — https://learn.microsoft.com/en-us/windows/wsl/disk-space (accessed 2026-08-30)
32. S32 — WSL basic commands (`--export`/`--import` incl. `--vhd`; `--unregister`) — https://learn.microsoft.com/en-us/windows/wsl/basic-commands (accessed 2026-08-30)

Additional API/tooling sources (Platform delta, Recommended crates, Risks):

33. [MS-FSCC] FileRenameInformationEx — `FILE_RENAME_POSIX_SEMANTICS` 0x00000002 / `FILE_RENAME_REPLACE_IF_EXISTS` 0x00000001 flag semantics — https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fscc/4217551b-d2c0-42cb-9dc1-69a716cf6d0c (accessed 2026-08-30)
34. FILE_RENAME_INFO (winbase.h; `FileRenameInfoEx` usage; no explicit min-version stated) — https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info (accessed 2026-08-30)
35. SetFileInformationByHandle (fileapi.h) — https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle (accessed 2026-08-30)
36. MoveFileExW (winbase.h; `MOVEFILE_WRITE_THROUGH`, `MOVEFILE_REPLACE_EXISTING`; XP+; Remarks reference the MAX_PATH / 1607 opt-in) — https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw (accessed 2026-08-30)
37. crates.io API per-crate records (versions/licenses, fetched 2026-08-30): https://crates.io/api/v1/crates/windows · /windows-sys · /same-file · /winreg · /jwalk · /walkdir
38. kondo repo (License: MIT, GitHub license field) — https://github.com/tbillington/kondo; crates.io record (v0.9.0, MIT) — https://crates.io/crates/kondo (accessed 2026-08-30)
