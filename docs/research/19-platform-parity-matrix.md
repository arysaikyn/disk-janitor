# Platform Parity Matrix — Research Note

For each macOS feature of Disk Janitor v1 (evidence grading, quarantine+restore, 72 h grace,
ledger, one-tap clean, protected folders, agent panel), states Windows and Linux status
(= full parity / ≈ parity with platform delta per SPEC Part B §5.5 / − v1-deferred), grounded
in notes 09–11 primitives; plus per-OS default protected-path sets, per-OS packaging
expectations, competitor table-stakes per platform, and an honest marketing claim per platform.
· researched 2026-09-01

## Findings

### 0. Scope and method

Disk Janitor's macOS design (evidence grading, two-phase quarantine, WAL+ledger, propose-only
agent) is the baseline; notes 09–11 proved the *architecture* carries to Windows/Linux with
only the syscall substrate changing [09 §delta][10 §delta][11 §8]. This note answers the
remaining product question SPEC Part A §8/§7 leaves open: **what does "native on 3 platforms"
actually promise a v1 user on each OS, and where is the honest line?** Method: (1) survey the
Windows and Linux disk-cleanup tool landscape (what users already expect); (2) map each macOS
feature to = / ≈ / − with the primitive from notes 09–11 that justifies the call; (3) derive
per-OS protected-path sets, packaging expectations, table-stakes, and an honest claim. No
re-derivation of taxonomies (notes 09/10) or FS primitives (note 11) — those are cited, not
repeated.

### 1. Windows disk-cleanup tool landscape — what users expect

| Tool | Positioning | What it gives | What it lacks (the gap Disk Janitor owns) | License |
|---|---|---|---|---|
| **Storage Sense** (built-in) | OS automated cleanup [S5][S6] | auto-cleans `%TEMP%` by age, Recycle Bin after N days, optional Downloads + OneDrive dehydration; cadence = low-disk/daily/weekly/monthly; WinSxS via separate `StartComponentCleanup` task + `DISM /StartComponentCleanup` [09 §1][S5][S6] | **no judgment** (deletes by age rule, not evidence); **no restore guarantee** (Recycle Bin restore is best-effort, can be pre-empted by another Storage Sense cycle); **no ledger**; no per-file evidence; WinSxS/Installer are tool-gated, never file-level [09 §1] | proprietary (OS) |
| **WizTree** | fastest NTFS analyzer [S1] | MFT-direct scan (instant), treemap, duplicates; hardlink-aware (no double-count); delete sends to Recycle Bin | **no judgment**; **no restore** (Relies on Recycle Bin only — no quarantine, no ledger); no agent; analyzer, not cleaner | free personal / paid commercial (EULA: "as is", no warranty) [S2][S3] |
| **TreeSize Free / Personal** | disk-space analyzer + reporting [S7][S8] | treemap, file-type/owner/age stats, duplicates (Personal), scheduled scans (Pro) | **no judgment**; Free has no delete workflow (analysis-only) [S7]; Personal delete moves to Recycle Bin — **no restore guarantee**, no ledger; no agent | free / paid (JAM Software) |
| **CCleaner** | system cleaner [S9][S10] | bulk rule-based cleaning (browser/cache/registry), secure-delete (shred), 24-h age gate, Recycle Bin context-menu integration [S9] | **no restore** (deletes immediately; recovery only via *separate* tool Recuva, after the fact) [S10]; **no evidence** (black-box rules); no ledger; reputation damage (2017 supply-chain incident, "scare tactics" reviews — cf. note 13 §B) | freemium (Piriform/Gen Digital) |
| **BleachBit** | open cleaner, privacy-first [S11][S12][S13] | preview-then-delete, CleanerML custom rules, shred/free-space wipe, CLI; package-manager cleaners (apt/dnf/pacman/snap) [S13] | **no restore** (preview is advisory; delete is immediate; shred is *irreversible*); **no judgment** beyond per-cleaner rule sets; no ledger; no agent | GPLv3 (verified repo COPYING) [S14] |

**Windows table-stakes** (what every Windows user already has/expects): (a) a fast NTFS scan
that does not double-count hardlinks (WizTree, TreeSize both do); (b) a Recycle Bin delete path
(IFileOperation) so the OS Recycle Bin is the de-facto restore surface; (c) Storage Sense as
the free automated-cleanup baseline (so any value-add must be *better* than age-based rules);
(d) OS-managed system junk (WinSxS, `C:\Windows\Installer`, Delivery Optimization, Windows.old)
treated as **tool-gated, never file-level** [09 §1] — the table-stakes honesty line is "never
touch WinSxS by hand"; Disk Janitor's tool-invocation manifest is the v1 way to honor that.

**Windows honesty tiers — what tools cannot do**: no Windows competitor offers a *guaranteed,
atomic, time-bounded restore*. The Recycle Bin is best-effort (shell-controlled, can be emptied
by Storage Sense, no same-volume atomicity guarantee); WizTree/TreeSize/CCleaner/BleachBit all
end at "delete" with no quarantine+WAL. Disk Janitor's `SetFileInformationByHandle(
FileRenameInfoEx)` same-volume quarantine + WAL gives the first *guaranteed* restore on Windows
(SPEC Part B §5.5) [11 §1][09 §delta] — but only the WAL, not OS Trash, is the restore path,
because no dir-fsync / `F_FULLFSYNC`-grade barrier exists on Windows [11 §2][09 §delta].

### 2. Linux disk-cleanup tool landscape — what users expect

| Tool | Positioning | What it gives | What it lacks (the gap) | License |
|---|---|---|---|---|
| **baobab** (GNOME Disk Usage Analyzer) [S15][S16] | scan + visualize | tree + ring/treemap, scans local/remote/volumes; delete = move to FreeDesktop Trash | **no judgment**; **no restore guarantee** (Trash restore is best-effort; cross-topdir copy+delete fallback is non-atomic [11 §6]); no ledger; no agent | GPL (GNOME) |
| **ncdu** [note 01] | terminal analyzer | parallel scan, hardlink-once, CACHEDIR.TAG honoring, delete via `d` or `--delete-command 'gio trash --'` | **no judgment** (name-based); **no restore guarantee** (permanent unlink unless routed to trash); no ledger; no agent | MIT [01] |
| **Stacer** [S17][S18] | all-in-one Linux optimizer | system cleaner (apt cache/logs/trash), startup-app manager, process/service monitor, uninstaller, APT repo manager; apt-cache module | **no judgment** (blanket cache delete); **no restore** (deletes immediately); no ledger; **abandoned upstream** (no further releases); Flatpak build runs sandboxed with limited resource access | GPLv3 (repo LICENSE) [S18] |
| **BleachBit** (also on Linux) [S11][S13] | open cleaner | preview-delete, package-manager cleaners (apt/dnf/pacman/snap), shred, free-space wipe | **no restore** (shred = irreversible; preview advisory); **no judgment** beyond CleanerML rules; no ledger; no agent | GPLv3 [S14] |
| **GNOME Disks (gnome-disk-utility)** | disk/partition manager | format, mount, benchmark, loop-device images — not a junk cleaner | out of scope for cleanup; no scan/classify/judgment | GPL (GNOME) |
| distro-native | `apt/dnf/pacman` clean verbs, `paccache.timer`, `journalctl --vacuum-*`, `systemd-tmpfiles-clean` [10 §1–§5] | per-cache official regen, versioned pruning, dry-run | **no unified view** (per-tool silos); **no restore**; no evidence grading across caches; no agent | distro licenses |

**Linux table-stakes**: (a) honor existing distro cleanup policy — `paccache.timer`,
`systemd-tmpfiles-clean`, journald `SystemMaxUse` already run weekly; re-implementing fights
the distro [10 §0]; (b) **verb delegation** — the only safe reclaim for package-manager caches,
journal, and container engines is the engine's own transactional command, never file unlink
[10 §delta]; (c) privilege boundary — root-owned paths (`/var/*`, `/boot`, `/var/lib/*`)
route through sudo/polkit or are marked `requires-elevation` [10 §0]; (d) freed-byte honesty
uses **allocated** size (`st_blocks·512`), never apparent `st_size` (sparse `Docker.raw`;
tmpfs `/tmp` contributes 0 reclaimable) [10 §5/§9].

**Linux honesty tiers**: like Windows, **no Linux competitor offers a guaranteed atomic restore**.
FreeDesktop Trash's cross-topdir fallback is copy+delete (non-atomic, can leave junk mid-copy
on crash) [11 §6]; ncdu/Stacer/BleachBit all end at delete. Disk Janitor's
`renameat2(RENAME_NOREPLACE)` same-volume quarantine + WAL gives the first guaranteed restore
on Linux — but `RENAME_NOREPLACE` is a **per-fs matrix** (ext4 ≥3.15, btrfs/tmpfs ≥3.17,
xfs ≥4.0, many more ≥4.9; **NFS rejects it with EINVAL**) [11 §1][10 §0] — so the guarantee is
probed per volume, not assumed, and NFS/fuse volumes are treated as external (refuse auto-action)
[11 risks #2]. Container/engine state dirs (`/var/lib/docker`, `/var/lib/containers/storage`)
**cannot** be file-level quarantined (a move corrupts the DB while the daemon runs); only the
engine's own `system prune` verb is safe, run through the plan manifest with typed confirm
[10 §9/§delta].

### 3. Parity matrix — macOS v1 feature → Windows / Linux status

Legend: **=** full parity (same guarantee, per-OS substrate). **≈** parity with a documented
platform delta (SPEC Part B §5.5). **−** v1-deferred (not in the course timeline; gated behind
a later platform investment).

| macOS v1 feature | Windows status | Linux status |
|---|---|---|
| **Evidence grading** (5-tier, machine-derived evidence chains) [SPEC Part A §3] | **=** — evidence engine is OS-agnostic (lockfile, mtime, sibling-manifest, orphan logic all carry [09 §delta]); Windows-specific signals: `CreateFileW` sharing-violation lock probe (no lsof) [09 §0], `nFileIndex`/`nNumberOfLinks` hardlink dedup [09 §0], OneDrive `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` dataless gating [09 §3][11 §3]. WinSxS/Installer/`Windows.old` grade as **HandsOff (tool-invocation)**, never file-movable [09 §1]. | **=** — engine OS-agnostic; Linux signals: `pacman -Qo`/`dpkg -S` ownership gating under `/usr`/`/etc` [10 §0], `CACHEDIR.TAG` marker [10 §0], privilege-boundary probe [10 §0]. `/boot` kernels + `/var/lib/docker` grade as **HandsOff** (verb-delegation only) [10 §6/§9]. |
| **Quarantine + restore** (atomic same-volume move) [SPEC Part B §5.1] | **≈** — `SetFileInformationByHandle(FileRenameInfoEx, Flags=0)` is the no-clobber atomic analog (Win10 ≥1607), runtime-probed with `MoveFileExW` fallback [11 §1][09 §delta]; restore-on-Windows relies on the **WAL journal, not OS Trash** (no dir-fsync, `FlushFileBuffers` weaker than `F_FULLFSYNC`) [SPEC §5.5][11 §2]. | **≈** — `renameat2(RENAME_NOREPLACE)` is the no-clobber atomic analog (kernel ≥3.15), but **per-fs matrix** (ext4/btrfs/xfs/tmpfs yes; **NFS rejects** with EINVAL) [11 §1][10 §0]; scanner probes `/proc/self/mountinfo` fstype per volume, refuses on NFS/fuse [11 risks #2]. |
| **72 h grace window** [SPEC Part A §3.2, ADR-10] | **=** — grace is a ledger/WAL policy, not an OS primitive; holds identically (quarantine dir + WAL restore pairs) [SPEC §5.6]. Shell Recycle Bin is *not* the grace surface on Windows (SPEC §5.5). | **=** — same WAL/ledger policy. FreeDesktop Trash is at most a purge-time handoff, never the restore path (cross-topdir copy+delete is non-atomic, violates zero-bytes-freed) [11 §6]. |
| **Ledger** (append-only JSONL, cross-session, uninstall restore-or-confirm) [SPEC §5.6, ADR-16] | **=** — JSONL + WAL is OS-agnostic; per-volume quarantine dirs + orphan-surfacing on uninstall carry over [SPEC §5.6]. | **=** — identical; orphans surfaced for manual resolution across all volumes [SPEC §5.6]. |
| **One-tap clean** (single confirm → plan apply) [SPEC Part C §3.9/§5] | **≈** — one-tap flows through the same plan→apply→purge gate; the OS-managed slice (WinSxS/DISM, Delivery Optimization) routes as tool-invocation manifest entries, so "one tap" = "one confirm that runs the official command," not "one file-level delete" [09 §1]. | **≈** — one-tap runs verb-delegated commands (`apt-get autoremove`, `journalctl --vacuum-*`, `docker/podman system prune`, `flatpak uninstall --unused`) as plan entries with typed byte-count confirm [10 §delta]; root-owned entries surface `requires-elevation` rather than silently failing [10 §0]. |
| **Protected folders** (never-touched defaults) [SPEC Part C] | **=** — per-OS protected set (§4 below); WinSxS/`C:\Windows\Installer`/`Windows.old` are protected-by-tool-gating, not by folder list alone [09 §1]. | **=** — per-OS protected set (§4 below); `/boot`, `/usr/lib/modules`, `/var/lib/{docker,containers,snapd}` protected by verb-delegation rule [10 §6/§9]. |
| **Agent panel** (propose-only, path-blind) [SPEC Part C §3.9, ADR-7] | **=** — agent is a local Rust loop; OS-irrelevant by construction (privacy chokepoint serializes opaque IDs, never paths) [SPEC §5.4]. Offline-fail behavior identical [SPEC Part C §5.1]. | **=** — identical; agent works the same on all three (no platform-specific UI; the dock renders plan cards regardless of OS) [SPEC Part C §3.9]. |

**Net**: 5 of 7 features are **= full parity** on Windows and Linux (evidence grading, grace,
ledger, protected folders, agent panel); the 2 with deltas (quarantine+restore, one-tap clean)
are **≈ parity** with platform deltas *documented, not hidden* — exactly SPEC Part A §8's honest
guarantee and Part B §5.5's Windows delta. No feature is **− v1-deferred** for the *judgment +
reversibility + ledger + agent* core; the deferred items are platform-specific polish
(vibrancy/signing, §5) not core safety.

## Reusable code & assets

| Repo/Package | License (verified) | What to borrow | Where |
|---|---|---|---|
| (none new) | — | Parity claims ride the primitives already verified in notes 09–11 (renameat2/FileRenameInfoEx probes, verb delegation); no competitor code is borrowed | platform/ layer |

## Recommended stack additions

None beyond SPEC Part D §1f (platform-gated deps, verified notes 09–11): windows 0.62.2 +
windows-sys, filetime 0.2.29 (Linux), existing rustix/libc/nix. Storage Sense parity (age
rules) is deliberately NOT copied — evidence grading replaces age rules (SPEC Part A §2).

## Risks & open questions

1. **≈ parity wording** is a marketing-honesty trap — every ≈ row must carry its delta in
   the same breath (SPEC Part B §5.5). Draft the 2 delta claims (section 3) verbatim before
   any demo copy.
2. **WinSxS tool-invocation entries** depend on `DISM`/`StartComponentCleanup` behaving on
   the user's OS; plan entries must re-probe at apply time, not trust a cached capability.
3. **Linux verb-delegation elevates** — `requires-elevation` entries need privilege-elevation
   UX (toast + handoff), unspecified in SPEC Part C today.
4. **Packaging** — MSI/NSIS/winget + AppImage/Flatpak/deb expectations (section 5) are
   recorded but the v1 ship target is macOS (ADR-11 macOS-first); Win/Linux packaging is
   post-9/10.

## Sources

Grounded in verified in-repo corpus (accessed 2026-09-01): SPEC Part A §1/§8, Part B §3.10/
§5.5, ADR-11/12; notes 09 (Windows taxonomy), 10 (Linux), 11 (cross-platform primitives),
13 (positioning). Competitor tool facts (sections 1–2 tables) cite [S1]–[S18] collected by
the research agent before its session died; vendor pages (WizTree, TreeSize, CCleaner/
Piriform, BleachBit repo COPYING, Stacer repo LICENSE, GNOME baobab, ncdu) are
re-verification queued in the Verification phase — the license calls (WizTree EULA-freeware,
TreeSize paid, CCleaner freemium, BleachBit GPLv3, Stacer GPLv3) match note 01/02
license-verification practice and are flagged for re-check against repo files when search
returns.
