# OSS Fork Viability for a Cross-Platform Disk-Cleanup Desktop App — Research Note
Re-testing every candidate from notes 01/02 plus new discoveries as a *fork base* vs *component donor* vs *reject*, with license-virality analysis · researched 2026-09-13

Scope: whether any open-source desktop disk-cleanup project is a viable **fork base** for
our cross-platform Rust+Tauri app (or failing that, a **component donor**); examined
Czkawka 12.0.2 (core/gui/cli/krokiet/cedinia), `czkawka-tauri` 1.1.0, `phpfc/cleanser`
0.7.5, fclones 0.35.0, BleachBit, rmlint, QDirStat, dupeGuru, Filelight/Baobab/WinDirStat/
K4DirStat, FluentCleaner, MacSlim 0.2.2, `codechu/disk-cleaner`,
`kandikoushik/disk-cleaner`, and the note-01 set (ncdu/gdu/dua-cli/dust/diskus/kondo/
cargo-sweep/npkill/node-prune). Every license string and version below was read from a
primary source (repo `Cargo.toml` / `LICENSE` / `COPYING` / `package.json` / `README.md`
or the project's own docs) on **2026-09-13**; the source list records what was read at
each URL and flags everything that would not load. Findings already in note 01 or note 02
are labelled "covered in note NN" — their tables are not restated here.

Additivity map against note 01 (`01-oss-disk-analyzers.md`): note 01 owns the landscape
tables and *borrow-worthy mechanics* for ncdu, gdu, dua-cli, dust, diskus, kondo,
cargo-sweep, npkill, node-prune. This note adds (a) **fork verdicts** for each of those,
(b) candidates note 01 never covered (the Czkawka family, cleanser, czkawka-tauri,
fclones, the GPL desktop cleaners, FluentCleaner, MacSlim, and two new GitHub finds),
(c) the **license-virality** argument, and (d) a symbol-mapped port backlog.

## Findings

### Czkawka family (qarmin/czkawka) — the license split is the whole story

**NEW (not in note 01).** Czkawka is not one license; across its five workspace members it
splits into **two**, and the split is deliberate (MIT for the core and the desktop/CLI
frontends upstream ships with the core; GPL-3.0-only for the two frontends that embed it in
other distribution channels):

| Member | Version | `license` field read at source | Verdict |
|---|---|---|---|
| `czkawka_core` | 12.0.2 | `MIT` (`czkawka_core/Cargo.toml:9`, restated as `## License\nMIT` in `czkawka_core/README.md`) | **donor** |
| `czkawka_gui` (GTK4) | 12.0.2 | `MIT` (`czkawka_gui/Cargo.toml:7`) | donor (stack mismatch) |
| `czkawka_cli` | 12.0.2 | `MIT` (`czkawka_cli/Cargo.toml:7`) | donor |
| `krokiet` (Slint GUI) | 12.0.2 | `GPL-3.0-only` (`krokiet/Cargo.toml:7`) | **reject** |
| `cedinia` (Android GUI) | 12.0.2 | `GPL-3.0-only` (`cedinia/Cargo.toml:7`) | reject |

The core README states the design intent in one line: "It has no UI dependency and is
**designed to be embedded in any frontend**." `czkawka_core/README.md` also documents the
dependency posture we care about: "The base build has no mandatory native dependencies —
it is pure Rust"; `heif`/`libraw`/`libavif` are opt-in features, and `similar_videos`
needs **ffmpeg at runtime** only. A third signal that the authors expect vendoring sits in
`czkawka_core/src/helpers/mod.rs:1`: "Helper modules: generic utilities, traits, structs,
**ready to copy/paste to other projects**." Czkawka 12.0 is also the last released version
(root `README.md`: "Version 12.0 is the last released version - no new binaries will be
provided"), which removes the *upstream-moves-under-you* risk for a frozen vendoring but
also means no upstream fixes.

Mechanically the most interesting pieces for us — all MIT — are the **cancellation +
progress** pair and the **emptiness proof**:

- `czkawka_core/src/common/progress_stop_handler.rs`: `pub const LOOP_DURATION: u32 = 20;`
  and `pub const SEND_PROGRESS_DATA_TIME_BETWEEN: u32 = 200;`. A dedicated progress thread
  samples `ProgressStatus { items_counter: Arc<AtomicUsize>, size_counter: Arc<AtomicU64> }`
  and emits `ProgressData { stage, entries_checked, entries_to_check, bytes_checked,
  bytes_to_check }` **at most every 200 ms**, with a `Instant::now().checked_sub(Duration::from_secs(10))`
  trick to force the first message out immediately. Scoring uses the same `Arc<AtomicUsize>`
  counters with relaxed reads; the stop flag is a single `Arc<AtomicBool>` checked by a
  free function `check_if_stop_received(&stop_flag)` from inside long loops. This is the
  exact shape our scan loop needs for "cancel mid-batch, keep completed batches" — and it
  is *decoupled from the data channel*, which our current `ScanStats` is not.
- `czkawka_core/src/helpers/delayed_sender.rs`: `DelayedSender<T: Send + 'static> { slot:
  Arc<Mutex<Option<T>>>, stop_flag: Arc<AtomicBool> }` — a **drop-oldest coalescing
  sender**. `send(value)` merely overwrites the slot; a worker thread sleeps `wait_time/5`
  and ships whatever is in the slot. The doc comment is honest about the cost ("the mutex
  … in the future, a more efficient channel could be used") — useful as a contrast to our
  bounded crossbeam channel, not as a replacement.
- `czkawka_core/src/common/fs_ops.rs::check_if_folder_contains_only_empty_folders(path)`:
  an **explicit-stack** emptiness proof. It seeds `entries_to_check` from the root, then
  `pop()`s until empty, returning `Err(_)` the moment any entry `!file_type.is_dir()`.
  Because the stack (not the call stack) holds the frontier, a 40-deep `node_modules` tree
  cannot blow up the recursion limit, and the error message names the offending entry
  (`core_folder_contains_file_inside`). This is a genuine improvement over the recursive
  emptiness test and maps directly onto our "is this whole tree a cache?" pre-check.
- `czkawka_core/src/common/items.rs`: **platform-constant exclusion defaults**. On macOS,
  `DEFAULT_EXCLUDED_ITEMS = "*/.git/*,*/node_modules/*,*/lost+found/*,*/Trash/*,*/.Trash-*/*,/Users/*/Library/Caches/*"`,
  and `DEFAULT_EXCLUDED_DIRECTORIES` on unix is `["/proc","/dev","/sys","/snap"]`. The
  macOS-specific entry `/Users/*/Library/Caches/*` and the `*/.Trash-*/*` alternation are
  exactly the two presets a naive walker gets wrong; the other platforms' lists are
  restated per-OS in the same file (`/home/*/.cache/*`, `/home/*/.var/app/` on Linux;
  `*:\$RECYCLE.BIN\*`, `*:\OneDriveTemp\*`, `*:\hiberfil.sys` on Windows). `DEFAULT_EXCLUDED_DIRECTORIES`
  is a `&[&str]` of *absolute* prefixes on unix but a Windows-shaped glob on Windows —
  i.e. upstream already encodes the "prefix vs glob" distinction we need in the deny-glob
  compiler.
- Cancellation semantics live in `CommonToolData` (`czkawka_core/src/common/tool_data.rs:17`):
  one struct carrying `stop_flag`, `progress_sender`, `directories`, `excluded_items`,
  `extensions`, `dry_run`, `move_to_trash`, `delete_method: DeleteMethod`, plus
  `DirTraversalBuilder` (`common/dir_traversal.rs:42`) that owns
  `stop_flag: Option<Arc<AtomicBool>>`, `progress_sender: Option<&Sender<ProgressData>>`,
  `minimal_file_size`/`maximal_file_size`, `collect: Collect` and `recursive_search`. The
  builder is the cleanest "one traversal, many tools" abstraction in the landscape —
  worth studying even though we will not adopt it wholesale.
- Cache discipline (`czkawka_core/src/common/cache.rs`): versioned bincode
  (`CACHE_VERSION: u8 = 100`, …) plus a 7-day cleaning pass — the pattern that a *derived*
  cache carries an explicit version and is *discardable*, never authoritative. Note the
  contrast with our ledger, which is authoritative and append-only.
- `DeleteMethod` (`tool_data.rs:38`) is a small enum that encodes **selection policy**, not
  deletion mechanics: `OneOldest`, `OneNewest`, `AllExceptNewest`, `OneBiggest`,
  `AllExceptSmallest`, `HardLink`. It is the closest thing in the landscape to our
  policy-driven selection (`src/plan.rs::build()`), but it selects by *rank within a
  duplicate group*, not by evidence grade — so it is a naming precedent, not an algorithm.

**Bottom line:** `czkawka_core` is the **strongest component donor** in the entire
survey; `krokiet`/`cedinia` are **rejects on license alone** (pulling them into a
distributed desktop binary would GPL the whole app).

### `czkawka-tauri` (shixinhuang99/czkawka-tauri) — MIT, and a frontend we cannot reuse

**NEW.** `tauri/Cargo.toml` reads `name = "czkawka-tauri"`, `version = "1.1.0"`,
`edition = "2024"`, `license = "MIT"`, `publish = false`; `LICENSE` is the plain MIT text
with "Copyright (c) 2025 Shixin Huang". So this is a **Tauri 2 + Czkawka-core** desktop
app under a permissive license — precisely the shape we would be forking.

The blockers are all on the frontend side (note 12 is the Tauri+Svelte stack note; this
conflict is new):

- It pins `czkawka_core = "10.0.0"` while upstream is at **12.0.2** — two majors of drift,
  with the core's `license` and module layout substantially rearranged in between.
- Its UI is **React 18.3.1 + Vite 7.1.3 + Tailwind 3.4.17** with **18 `@radix-ui/react-*`
  packages**, `@tanstack/react-table`, `@tanstack/react-virtual`, `jotai`, `i18next`, and
  `babel-plugin-react-compiler` (`ui/package.json`). Our `app/` is **Svelte 5**. Adopting
  this frontend means throwing away our entire view layer.
- Deletion goes to the **recycle bin by default** (its README: "Files are moved to the
  recycle bin by default when deleted"), i.e. the opposite of our quarantine-then-purge
  contract, and there is no typed-byte gate.
- Its `tauri/src/main.rs` sets a config-cache path via
  `set_config_cache_path("Czkawka", "Krokiet")` and manages a single
  `Mutex<AppState>`; there is no typed IPC channel, no per-command state machine, and no
  recovery on launch.

**Verdict: REJECT as a fork** (wrong frontend framework + stale core + no reversibility
contract), but it is a useful existence proof that *an MIT Tauri wrapper around
`czkawka_core` is legally and technically ordinary*. It also carries a stale-looking
`[lints.clippy] uninlined_format_args = "allow"` we should not copy.

### `phpfc/cleanser` 0.7.5 — MIT, Tauri, and the closest OSS analogue to our reclaim/restore loop

**NEW (note 01 never covered it; note 02 covers only macOS vendor apps).** `Cargo.toml`
(root) is a real workspace: `resolver = "2"`, members `cleanser-core`, `cleanser-cli`,
`cleanser-gui`; `[workspace.package] version = "0.7.5"`, `edition = "2021"`,
`license = "MIT"`. `crates/cleanser-gui/Cargo.toml` is a **Tauri 2** app using
`license.workspace = true`. `crates/cleanser-cli` uses ratatui/crossterm/indicatif. The
README's caution is unambiguous and worth quoting in the design document: "permanent
deletion of files; use at your own risk."

The deletion subsystem is the part that matters, and it is split
`crates/cleanser-core/src/deletion/{mod,journal,secure,strategy,trash}.rs` with a docs
comment naming three strategies (Standard / Secure DoD 5220.22-M / Trash). Two files are
direct prior art:

- **`deletion/journal.rs`** — a **versioned JSON journal**: `const JOURNAL_VERSION: u32 = 1`,
  `TrashEntry { id: String /* uuid v4 */, original_path, trash_path, size, is_directory,
  deleted_at: DateTime<Utc>, description, category: Option<CleanCategory> }`, and
  `TrashJournal { version, entries, last_cleanup: Option<DateTime<Utc>> }`. Crucially it
  takes **`fs2` shared/exclusive locks**: `load()` takes `lock_shared()`, `save()` takes
  `lock_exclusive()`, and the lock path is `journal_path.with_extension("lock")`. A version
  mismatch only emits a `warn!` — **no migration**, which is precisely the durability hole
  our `src/ledger.rs::replay()` closes with `FORMAT_V` + `skipped_unknown` accounting.
- **`deletion/trash.rs`** — `TrashConfig { use_system_trash, custom_trash_path,
  auto_empty_days: Option<u32> /* default Some(30) */, max_trash_size }`, and macOS lands
  files in **`~/.cleanser-trash`**, *not* the system Trash. `move_to_trash()` tries
  `fs::rename` first and falls back to **recursive copy + `remove_dir_all`** for
  cross-filesystem moves. `restore(entry_id)` refuses if `trash_path` is gone and, when
  `original_path` already exists, appends `_{YYYYMMDD_HHMMSS}` to the filename.

The contrast is the finding: cleanser has the *shape* of our contract (quarantine dir,
journal, restore, grace) but none of the *hardening* — no `renamex_np(RENAME_EXCL)`
existence check, no `EEXIST` detection, no per-entry WAL intent/result pair, no CRC chain,
no torn-tail handling, no seq-gap accounting. Meanwhile it has two things we do not:
a persisted `auto_empty_days` default (30 d) and an explicit cross-volume copy fallback.

Its CLI surface is a good sanity check on ours: `clean --risk safe|moderate|risky`,
`--dry-run`, `clean --interactive`, `scan --speed quick|normal|thorough`,
`scan --older-than 90d`, `scan --min-size 500`, `--json`. The API
(`crates/cleanser-core/src/lib.rs`) is deliberately frontend-agnostic: "It can be used as
a library by CLI tools, GUI applications, or other Rust programs", and it exposes a
`ProgressCallback` trait with `on_scan_progress(ScanProgress)` / `on_clean_progress(CleanProgress)`
— i.e. a **callback trait instead of a channel**, the opposite of Czkawka's sender model.
`types.rs` shows the same idea one level down: `RiskLevel { Safe, Moderate, Risky }` with
`#[default] Safe` and `Ord`, and `CleanableItem { path, size, category, risk_level, description }`.
Their risk tiers are *coarse* (three buckets, hand-assigned per rule); ours are per-item
evidence grades. That is the design difference to state, not to copy.

**Verdict: top FORK candidate on paper, REJECT-for-fork on facts.** It is MIT, layered,
Tauri-based and small enough to absorb — but forking it means adopting a deletion path we
must rewrite anyway, and abandoning `src/execute.rs`, `src/ledger.rs`, `src/plan.rs`
(all already written, tested, and 91-tests green). Fork viability is not "could we", it is
"would it be net-negative now"; here it is.

### rmlint (sahib/rmlint) — GPL-3.0, and structurally incapable of losing data

**NEW.** `COPYING:1-2` is "GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007" — so
rmlint is a **reject for any code borrowing into our MIT/Apache codebase**, and it is
ironically the safest tool in the survey: its own docs say *"rmlint itself WILL NOT DELETE
ANY FILES. It does however produce executable output (for example a shell script)"*.
Every finding below is therefore a **design precedent, not a code source**.

The `-o` output-formatter split is the direct ancestor of our quarantine/purge separation:
`-o json`, `-o sh`, `-o py`, `-o uniques`, `-o stamp`, `-o summary`, `-o fdupes`. The `sh`
formatter takes a *handler list* — `clone|reflink|hardlink|symlink|remove|usercmd` — where
`clone` uses the `FIDEDUPERANGE` ioctl, `hardlink` explicitly notes it "Fails if both files
are not on the same partition", and `symlink`/`remove`/`usercmd` are documented as
**"never fail"**. That is exactly the same volume-boundary argument our
`src/execute/macos.rs::rename_no_replace` + `EXDEV` handling embodies: cross-device
recovery actions degrade to a *copy*, and the tool must know which class of action it is
issuing. rmlint's answer — separate the *analysis* from the *action script* — is the same
choice we made with `Plan` + `apply`.

Three more rmlint facts worth citing:

- `-o json` supports `oneline=[true|false]` with the doc "Print one JSON document per line
  … useful if you plan to parse the output line-by-line, e.g. while rmlint is still
  running". This is the **streaming-first** framing our `src/plan.rs::write_manifest()`
  (pretty-printed whole-file JSON) does not have, and it is the right shape for a
  progressive GUI.
- `--size "min-max"` accepts multipliers `C,W,B,K,KB,M,MB,G,GB,T,TB,P,PB,E,EB` with a `d`
  flag, and — the interesting default — **empty files are excluded from duplicate search
  unless you pass `-T df --size 0`**. A cleanup tool that defaults to *not* surfacing
  zero-byte matches is a useful precedent for our minimum-bytes default.
- `-l --hardlinked` is the **default**, with the stated reason "Removing hardlinks to
  originals will not allocate any free space" — i.e. rmlint defaults to *not offering*
  actions that cannot deliver the user's goal. Our `PlanTotals.snapshot_caveat` is the same
  instinct applied to APFS snapshots.

**Verdict: REJECT** (GPL-3.0). Keep as the design precedent for analyse-then-script.

### fclones (pkolaczk/fclones) 0.35.0 — MIT, and the best "inspectable result then act" model

**NEW.** `fclones/Cargo.toml`: `version = "0.35.0"`, `license = "MIT"`, `edition = "2021"`,
`rust-version = "1.74"`; deps `bincode 1.3`, optional `blake3 1.3`, `byteorder 1.4`. The
README's framing is the part to cite: *"it speaks JSON, so you have a lot of control over
the search and cleanup process"*, and *"You can inspect and modify the list of duplicate
files before removing them. There is also a `--dry-run` option that can tell you exactly
what changes on the file system would be made."* Feature list also names "finding files
with more than N replicas", "finding unique files", min/max size filters and "proper
handling of symlinks and hardlinks" plus **path prefix compression** — a compact way to
serialise a large hit list that our JSONL ledger could borrow if entry counts grew.

The design lesson: **the result set is a first-class, user-editable artifact**. Our
equivalent is `Plan` + `write_manifest()`/`load_manifest()`; fclones' extra is that the
file is *round-trippable through the CLI* (edit the list, feed it back to `remove`). A GUI
that lets the user hand-edit the plan and re-load it is a genuine product idea we have not
shipped — the plumbing (`load_manifest`) already exists.

**Verdict: COMPONENT-ONLY** (algorithm ideas; MIT makes even code copying legal, but the
duplicate-finding code is not on our critical path).

### QDirStat 2.0 — GPL V2, and the smoking gun for "free reclaim without restore"

**NEW.** `src/Version.h` says `RELEASE_VERSION "2.0"`; `src/Trash.h:3` and
`src/StdCleanup.cpp` both carry `License: GPL V2`; `LICENSE` is the GNU GPL Version 2,
June 1991 text. GPL V2 is **viral on distribution**, so QDirStat is a reject as a code
donor — but it supplies the single strongest argument for this project's existence.

`src/Trash.h` declares an XDG-Trash implementation and then admits:

```cpp
    /**
     * Restore a file or directory from the trash to its original location.
     * Return 'true' on success, 'false' on error.
     *
     * This is currently not implemented; this function does nothing.
     **/
    static bool restore( const QString & path );
```

and, for `empty()`: *"This does not just affect files or directories moved to the trash
with this class, but everything in all known trash directories … This is currently not
implemented; this function does nothing."* Meanwhile the shipped cleanup actions in
`src/StdCleanup.cpp` are raw shell:

| Action | Command string | Confirmation |
|---|---|---|
| `hardDelete` | `rm -rf %p` — titled **"&Delete (no way to undelete!)"** | `setAskForConfirmation(true)`, `Ctrl+Delete` |
| `clearDirContents` | `rm -rf %d/*` | true |
| `gitClean` | `git clean -dfx` | true |
| `deleteJunk` | `rm -f *~ *.bak *.auto core` (recurse, `/bin/bash`) | false |
| `compressSubtree` | `cd ..; tar cjvf %n.tar.bz2 %n && rm -rf %n` | false |
| `checkFileType` | `file %n \| sed -e 's/[:,] /\n  /g'` | n/a |

Note the title string. A mature, widely packaged GUI disk tool ships a button that says,
in its own label, **there is no way to undelete** — and its own restore API is a stub.
Two of the six shipped actions (`deleteJunk`, `compressSubtree`) don't even ask before
running `rm -rf`. Our typed-byte purge gate and 72 h grace window are the direct answer to
this, and this table is the citation for that claim.

`src/Cleanup.h` also contains a warning that reads like it was written about our own
confirm dialogs: on `askForConfirmation()` — *"The default is 'false'. Use with caution -
not only can this become very annoying, people also tend to automatically click on 'OK'
when too many confirmation dialogs pop up!"* Our answer (confirm only once, at the typed
byte count, and make the review list do the rest) is the correct resolution of that
tradeoff, and QDirStat's own experience is the evidence that a confirmation-per-action
model degrades to muscle memory.

**Verdict: REJECT as code** (GPL V2), **highest-value reference** for the pain-point
section of the design document.

### The GPL desktop set — BleachBit, dupeGuru, Filelight, Baobab, K4DirStat, WinDirStat

**NEW** (note 02 covered macOS vendor apps only). All verified by reading the license file
itself on 2026-09-13:

| Project | Version | License file read | Copy-risk |
|---|---|---|---|
| BleachBit | (master, py3 + GTK) | `COPYING` = GNU GPL **Version 3**, 29 June 2007 | viral on distribution |
| dupeGuru | `core/__init__.py` → `__version__ = "4.3.1"` | `LICENSE` = GNU GPL **Version 3**, 29 June 2007 | viral on distribution |
| Filelight | master | `src/main.cpp:1-7` SPDX header = `GPL-2.0-only OR GPL-3.0-only OR LicenseRef-KDE-Accepted-GPL` | viral |
| Baobab | master | `COPYING:1-2` = GNU GPL **Version 2**, June 1991 | viral |
| K4DirStat | master | `COPYING:1-2` = GNU GPL **Version 2**, June 1991 | viral |
| QDirStat | 2.0 | `LICENSE` = GPL **V2** (see above) | viral |
| WinDirStat | master | **UNVERIFIED** — both `LICENSE` and `COPYING` return HTTP 404 at the `master` raw URL; the README documents the feature set but not the license; do not cite a license for it | unknown |

BleachBit's README is worth one line because it describes the *default interaction model*
we rejected: "select some options, and click Preview. Review the files, toggle options
accordingly, and click Delete" — i.e. opt-in rules with a global preview. And it is the
project codechu/disk-cleaner names in its acknowledgements, alongside Filelight, ncdu and
Czkawka. WinDirStat's README is useful as a **feature checklist for a mature Windows
cleaner**: "All Files, Largest Files, Duplicate Files, Search Results, File Watcher,
Extension, and Treemap views", "logical versus physical size", "cloud-file safeguards,
hardlink deduplication … reparse-point exclusions", "save or reload scans", and built-in
actions for "moving files, showing properties, deleting files, and emptying folders or the
Recycle Bin" — a superset of what a cross-platform product eventually needs, and a useful
gap table for our part-B parity claims (note 19).

**Verdict for all six: REJECT as forks and as code donors** (license), **KEEP as
feature/UX references**. Note the practical consequence: the entire *mature* GUI
disk-cleanup landscape is GPL, so any permissive fork base must come from the
young/small end (Czkawka core, cleanser, czkawka-tauri) — which is why "fork" loses to
"build on our own core + donate from `czkawka_core`" here.

### FluentCleaner (builtbybel) — MIT/Windows-only, plus a config-format idea

**NEW, and a correction to the earlier session's read.** The resume note recorded the
license as MISSING because `LICENSE.txt` and `LICENSE.md` both 404 — but a file named
exactly `LICENSE` exists and is **plain MIT**, "Copyright (c) 2026 A Belim app creation".
So FluentCleaner is permissive; the earlier "UNVERIFIED" is now resolved. Findings that
matter:

- Two flavours share one engine: FluentCleaner (.NET 10 + WinApp SDK, self-contained
  ~140 MB / 243 files) and FluentCleaner Classic (.NET Framework 4.8 WinForms, ~3.57 MB /
  20 files), with the shared `FluentCleaner.Core` targeting `netstandard2.0` and containing
  a **Winapp2.ini parser** (`FluentCleaner.Core.csproj`: `TargetFramework netstandard2.0`,
  `LangVersion latest`, `Nullable enable`; packages `System.Text.Json 8.0.5`,
  `IndexRange 1.0.3`, `Microsoft.Win32.Registry 5.0.0`).
- `/AUTO` and `/AUTO /SHUTDOWN` unattended modes log **every deleted path grouped by entry
  plus the total cleaned size** to `%AppData%\FluentCleaner\auto.log`. That is a
  per-invocation receipt, the same artifact class as our ledger, and it is the argument for
  our receipts being machine-readable.
- The README contains explicit **anti-feature essays**: no secure-delete (SSD wear-levelling
  and TRIM make overwriting meaningless) and deliberately no registry cleaner ("risk/reward
  is completely inverted"). Our `PolicyConstraintsShape` refusing `ImmutableFlag`/`Dataless`
  and never offering a "secure erase" is the same conclusion reached independently.
- Winapp2.ini is the closest thing the Windows ecosystem has to a shareable rule database.
  Our MTA policy DSL (`src/policy.rs::compile()`) is the analogous user-facing rule layer;
  a *declarative external rule file* for the GUIted side is a plausible future step, and
  Winapp2.ini is the precedent that such files must be versioned and scoped per app.

**Verdict: REJECT as a fork** (Windows/.NET, no relevance to a Rust+Tauri macOS-first
build); **COMPONENT-ONLY at the design level** (unattended-mode receipt log; anti-feature
documentation; external rule-file format).

### MacSlim (edwin-hao-ai) — BSL-1.1, and a three-layer safety model worth copying

**NEW.** `LICENSE:1-20` is the **Business Source License 1.1**, Licensor "Edwin Hao /
Beijing VGO Co., Ltd.", Licensed Work **"MacFlow"**, with an Additional Use Grant that
forbids competing hosted/embedded production use, **Change Date 2030-05-01**, Change
License Apache-2.0. Its README badge says `license-MIT` — **the badge is wrong; the
LICENSE file governs.** (This is one of two badge-vs-LICENSE contradictions found; see
the risks section.) The BSL means MacSlim is not usable as a fork base or a code donor
until 2030-05-01, and even then only under Apache-2.0 with the historic restriction
attached.

Its engineering is nonetheless the most directly comparable work in the survey: a Rust +
Tauri macOS cleaner (`src-tauri/Cargo.toml`: `name = "macslim"` 0.2.2, edition 2021,
rust-version 1.80, **no `license` field**, lib `macslim_lib` as staticlib/cdylib/rlib, bins
`macslim` + `macslim-cli`, `tauri 2.1` with `["macos-private-api","tray-icon","image-png"]`,
`tauri-plugin-os 2.2`, release profile `panic="abort", codegen-units=1, lto=true,
opt-level="s", strip=true`) with a SolidJS frontend and modules `scanner, cache_scanner,
cache_cleaner, process_ops, process_safety, applications, app_scanner, residue_scanner,
dev_tool_rules, uninstaller, docker, storage`.

The **three-layer safety model** is the borrowable idea, in its own README's words:
1. a **tool-busy check** — refuse to touch a cache whose owning process is running;
2. a **path allowlist** — reject `/`, `$HOME`, `~/Documents` and similar;
3. a **pre-execution re-check** — "right before deletion, the tool-busy check runs again
   to guard against race conditions".

Layer 3 is a genuine gap in our design: our `Executor::apply` pre-checks run inside the
apply loop, but the *plan* was built possibly minutes earlier, so a project that started
building in between is caught only by `Plan`-time evidence. MacSlim's answer is to hoist a
cheap liveness re-check to the moment of each move. Our equivalent would be re-running the
MTA/hot-file predicate per entry just before `renamex_np`. Combined with `codechu`'s
"the destructive control-API path is *blocked* (only the GUI can clean)" (below), this
gives two independently-published precedents for the same invariant: **the destructive verb
must be re-authorised at the point of no return, not only at plan time.**

**Verdict: REJECT** (BSL-1.1 for 4½ more years), **COMPONENT-ONLY at the design level**
(three-layer pre-execution safety; a CLI twin of the GUI; SQLite history DB as an
alternative to an append-only file — rejected in favour of our WAL, but note it exists).

### `codechu/disk-cleaner` — badge says MIT, LICENSE says GPL-3.0

**NEW.** This is the second badge-vs-LICENSE contradiction and a stronger one, because the
README's licence badge says MIT while `LICENSE:1-8` is the GNU GPL v3 header ("disk-cleaner
— Disk space cleaner with GTK 3 GUI / Copyright (C) 2026 Codechu — Onur Barlık … either
version 3 …"). **The LICENSE file wins; record the contradiction rather than picking the
convenient one.**

Two product ideas in it are unusually well-aligned with ours:

- **Process-aware scoring** — README: "won't touch a cache your browser is currently
  using", i.e. the same busy-check as MacSlim's layer 1.
- **"active-project protection from git mtime"** — do not clean inside a project that has
  been touched recently, judged by git metadata rather than by file mtime. This is
  *exactly* our `src/plan.rs::ancestor_git_head_mtime()` rule (the MTA 「别动我这两周在跑的项目」
  constraint), arrived at independently by an unrelated project — strong external
  validation for the design.
- It defaults to **trash mode** (`gio trash`) rather than unlink, ships an explicit
  `--dry-run`, and — the structural choice we should note — its control API is a
  **line-delimited JSON unix socket** at
  `~/.local/state/...`/`$XDG_RUNTIME_DIR/codechu/disk-cleaner/control.sock` with the
  destructive path *disabled* for socket clients, so only the GUI can clean.
- Output auto-switches table (TTY) vs JSON (pipe); custom JSON cleaner definitions live in
  `~/.config/codechu/disk-cleaner/cleaners/` — an external rule-file precedent again.
- Limits: **Python-only**, Linux/GTK, macOS and Windows explicitly unsupported.

**Verdict: REJECT as fork** (GPL-3.0, Python/GTK, Linux-only); **COMPONENT-ONLY at the
design level** (git-mtime activity heuristic — already ours; blocked destructive IPC path).

### `kandikoushik/disk-cleaner` — Apache-2.0, four platforms, none of them Rust

**NEW.** `LICENSE` is the full Apache License 2.0 ("Copyright 2026 Koushik Kandi"); the
README badge agrees (Apache-2.0), so this is a *clean* permissive claim — the one candidate
with no licence ambiguity. Version 4.0.0 is a multiplatform rewrite: macOS Swift 5.10 /
SwiftUI (~30 targets) with `TaskGroup` parallel sizing, three-pass SHA-256 duplicate
hashing, and a menu-bar extra; Windows C#/.NET; iOS SwiftUI; Android Kotlin. Features:
JSON plugin targets in `~/.config/disk-cleaner/plugins/*.json`, Docker/Podman volume purge,
Homebrew cask orphan cleanup, a LaunchAgent weekly auto-clean daemon, and "zero background
telemetry".

**Verdict: REJECT as a fork** — there is not a single line of Rust, and four native
platform rewrites is an anti-goal for a Tauri app. **COMPONENT-ONLY at the idea level**:
the JSON *plugin target* format (third independent instance of the external-rule-file
pattern after Winapp2.ini and codechu's cleaners dir) and the "weekly scheduled auto-clean
as a LaunchAgent" packaging idea for a later milestone.

### Explicit negatives

- **No project named "bleeding-edge-cleaner" exists.** A search on 2026-09-13 returned
  only cleanser, codechu/disk-cleaner, kandikoushik/disk-cleaner, FluentCleaner and
  `anh9000/diskclean` as near-neighbours; there is no repository by that name. Recorded so
  no future session treats it as a missed candidate.
- **No OSS cleaner ships a working reclaim-restore loop.** The closest are cleanser (MIT,
  custom trash + journal, but no rename-exclusive hardening — see above) and QDirStat
  (GPL, `restore()` is a stub). Every mature GUI tool either deletes irreversibly
  (QDirStat `rm -rf`, WinDirStat "deleting files") or delegates to the OS Trash, where the
  OS owns retention and the app cannot report bytes reclaimed or guarantee a grace window.
  **This is the project's differentiator, and it is verified by absence rather than by
  assertion.**
- **The entire mature GUI cleaner landscape is GPL.** Any fork base must be young and
  permissively licensed; the three real candidates (Czkawka family, czkawka-tauri,
  cleanser) are all ≤2 majors old or unmaintained.

### Fork decision table

| Candidate | Version | License (source-verified) | Fork? | Why |
|---|---|---|---|---|
| `czkawka_core` | 12.0.2 | MIT | **COMPONENT-ONLY** | UI-free, pure-Rust, embeddable, MIT — but it is an analyser, not a cleaner; adopting it as a base would import 14 tools and a Czkawka-shaped architecture we would then have to strip |
| `czkawka_gui` | 12.0.2 | MIT | REJECT | GTK4, not Tauri |
| `krokiet` / `cedinia` | 12.0.2 | **GPL-3.0-only** | REJECT | license |
| `czkawka-tauri` | 1.1.0 | MIT | REJECT | React 18 + 18 Radix vs our Svelte 5; core pinned at 10.0.0 (two majors stale); recycle-bin default; no reversibility contract |
| `phpfc/cleanser` | 0.7.5 | MIT | **best FORK candidate — still REJECT-for-fork** | right shape (workspace core/cli/gui, Tauri 2, journal + trash + risk tiers + dry-run), wrong depth (no WAL, no rename-exclusive, no typed gate); forking discards our tested `execute`/`ledger`/`plan` |
| `fclones` | 0.35.0 | MIT | COMPONENT-ONLY | duplicate engine is off critical path; JSON result list + `--dry-run` model is the idea |
| MacSlim | 0.2.2 | **BSL-1.1** (README badge wrongly says MIT) | REJECT | license until 2030-05-01 |
| `codechu/disk-cleaner` | master | **GPL-3.0** (README badge wrongly says MIT) | REJECT | license + Python/GTK Linux-only |
| `kandikoushik/disk-cleaner` | 4.0.0 | Apache-2.0 (badge agrees) | REJECT | zero Rust; 4 native platform rewrites |
| FluentCleaner | master | MIT (`LICENSE`; earlier `LICENSE.txt`/`.md` 404s) | REJECT | Windows/.NET |
| rmlint | master | GPL-3.0 | REJECT | license |
| QDirStat | 2.0 | GPL V2 | REJECT | license |
| BleachBit / dupeGuru / Filelight / Baobab / K4DirStat | — | GPL-2.0/3.0 (verified per file) | REJECT | license |
| WinDirStat | master | UNVERIFIED (LICENSE/COPYING 404) | REJECT | not established |
| ncdu / gdu / dua-cli / dust / diskus / kondo / cargo-sweep / npkill / node-prune | — | covered in note 01 | COMPONENT-ONLY (note 01 verdicts) | note 01 owns these; the fork answer stays no — none is a cleanup *product* |

**The decision in one line:** there is **no viable fork base**; the correct strategy is
**component donation from `czkawka_core` (MIT) into our own core**, whose safety layer
(`Executor`, `ledger`, `plan`) has no permissively licensed equal in the landscape.

## Reusable code & licenses

All licenses/versions below were read from the linked file on 2026-09-13 (see Sources).
"Copy-risk" is about *distributing a derived desktop app*, not about reading the code.

| Project | File (linked) | License | What is borrowable | Fit |
|---|---|---|---|---|
| qarmin/czkawka (`czkawka_core` 12.0.2) | [`czkawka_core/Cargo.toml`](https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/Cargo.toml) | **MIT** | whole core: traversal, progress/cancel, emptiness proof, exclusion presets, versioned cache | **best donor** — no UI dep, pure-Rust base build |
| qarmin/czkawka (`krokiet` / `cedinia` 12.0.2) | [`krokiet/Cargo.toml`](https://raw.githubusercontent.com/qarmin/czkawka/master/krokiet/Cargo.toml), [`cedinia/Cargo.toml`](https://raw.githubusercontent.com/qarmin/czkawka/master/cedinia/Cargo.toml) | **GPL-3.0-only** | — (read-only reference) | **do not link** — would GPL the app |
| shixinhuang99/czkawka-tauri 1.1.0 | [`tauri/Cargo.toml`](https://raw.githubusercontent.com/shixinhuang99/czkawka-tauri/main/tauri/Cargo.toml) | **MIT** | the *pattern* of an MIT Tauri shell around `czkawka_core`; tauri-plugin set (`dialog`/`opener`/`single-instance`) | legal precedent only — React frontend, stale core |
| phpfc/cleanser 0.7.5 | [`crates/cleanser-core/src/deletion/journal.rs`](https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/deletion/journal.rs) | **MIT** | versioned trash journal + `fs2` shared/exclusive locking + `restore_entry` collision suffix | strong *contrast*; our `ledger.rs` is strictly stronger |
| phpfc/cleanser 0.7.5 | [`crates/cleanser-core/src/types.rs`](https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/types.rs) | **MIT** | `RiskLevel{Safe,Moderate,Risky}` + `CleanableItem.description` | UX vocabulary — coarse vs our evidence grades |
| phpfc/cleanser 0.7.5 | [`crates/cleanser-core/src/lib.rs`](https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/lib.rs) | **MIT** | `ProgressCallback` trait (`on_scan_progress`/`on_clean_progress`) | alt to channel-based progress |
| sahib/rmlint | [`docs/rmlint.1.rst`](https://raw.githubusercontent.com/sahib/rmlint/master/docs/rmlint.1.rst) | **GPL-3.0** (`COPYING`) | `-o json oneline` streaming; `-o sh` handler taxonomy (`clone/reflink/hardlink/symlink/remove`) | design precedent only, no code |
| pkolaczk/fclones 0.35.0 | [`fclones/Cargo.toml`](https://raw.githubusercontent.com/pkolaczk/fclones/main/fclones/Cargo.toml) | **MIT** | round-trippable JSON result list + `--dry-run` + path prefix compression | legal to copy; off critical path |
| shundhammer/qdirstat 2.0 | [`src/StdCleanup.cpp`](https://raw.githubusercontent.com/shundhammer/qdirstat/master/src/StdCleanup.cpp), [`src/Trash.h`](https://raw.githubusercontent.com/shundhammer/qdirstat/master/src/Trash.h) | **GPL V2** (`LICENSE`) | `Cleanup::RefreshPolicy`/`OutputWindowPolicy` enums; the "no way to undelete" + unimplemented `restore()` evidence | reference only — the pain-point citation |
| builtbybel/FluentCleaner | [`LICENSE`](https://raw.githubusercontent.com/builtbybel/FluentCleaner/main/LICENSE) | **MIT** ("Copyright (c) 2026 A Belim app creation") | `/AUTO` per-run receipt log (every path + total); Winapp2.ini external rule-file format | design only (Windows/.NET) |
| edwin-hao-ai/MacSlim 0.2.2 | [`LICENSE`](https://raw.githubusercontent.com/edwin-hao-ai/MacSlim/main/LICENSE) | **BSL-1.1** (README badge wrongly says MIT) | three-layer safety: tool-busy check, path allowlist (`/`, `$HOME`, `~/Documents`), pre-execution re-check | not copyable until 2030-05-01; copy the *idea* |
| codechu/disk-cleaner | [`LICENSE`](https://raw.githubusercontent.com/codechu/disk-cleaner/main/LICENSE) | **GPL-3.0** (README badge wrongly says MIT) | git-mtime active-project protection; destructive IPC path blocked; `gio trash` default | reference only |
| kandikoushik/disk-cleaner 4.0.0 | [`LICENSE`](https://raw.githubusercontent.com/kandikoushik/disk-cleaner/main/LICENSE) | **Apache-2.0** (badge agrees) | JSON plugin-target format; LaunchAgent weekly auto-clean packaging | reference only (Swift/Kotlin/C#) |
| ArturKovacs/trash-rs 5.2.8 | [`Cargo.toml`](https://raw.githubusercontent.com/ArturKovacs/trash-rs/master/Cargo.toml) | **MIT** | OS Trash move/restore; used by `czkawka_core` and `czkawka-tauri` | optional platform fallback; note 01 listed 5.2.6 (stale) |
| danburkert/fs2-rs 0.4.3 | [`Cargo.toml`](https://raw.githubusercontent.com/danburkert/fs2-rs/master/Cargo.toml) | **MIT/Apache-2.0** | cross-platform `FileExt::lock_shared`/`lock_exclusive` — the primitive cleanser's journal uses | **precedent only**: our `ledger::open()` already uses std `File::try_lock` |
| BurntSushi/walkdir 2.5.0 | [`Cargo.toml`](https://raw.githubusercontent.com/BurntSushi/walkdir/master/Cargo.toml) | **Unlicense/MIT** | metadata-first recursive walk | covered in note 01/02 — restated only as a version check |
| BurntSushi/ripgrep `ignore` 0.4.33 | [`crates/ignore/Cargo.toml`](https://raw.githubusercontent.com/BurntSushi/ripgrep/master/crates/ignore/Cargo.toml) | **Unlicense OR MIT** | gitignore semantics, parallel walker, `simd-accel = []` proven no-op | covered in note 01 |
| tbillington/kondo `kondo-lib` 0.9.0 | [`kondo-lib/Cargo.toml`](https://raw.githubusercontent.com/tbillington/kondo/master/kondo-lib/Cargo.toml) | **MIT** | project-type detection for "rebuildable" evidence | covered in note 01 |
| bleachbit / dupeGuru 4.3.1 / Filelight / Baobab / K4DirStat / WinDirStat | [`bleachbit/COPYING`](https://raw.githubusercontent.com/bleachbit/bleachbit/master/COPYING), [`dupeguru/LICENSE`](https://raw.githubusercontent.com/arsenetar/dupeguru/master/LICENSE), [`filelight/src/main.cpp`](https://raw.githubusercontent.com/KDE/filelight/master/src/main.cpp), [`baobab/COPYING`](https://raw.githubusercontent.com/GNOME/baobab/master/COPYING), [`k4dirstat/COPYING`](https://raw.githubusercontent.com/jeromerobert/k4dirstat/master/COPYING) | **GPL-2.0 / GPL-3.0** (WinDirStat: UNVERIFIED, files 404) | feature/UX checklists only | **no code reuse possible** |

## Recommended crates

Only crates we do *not* already pin in the root `Cargo.toml` are actionable; the rest are
listed to record why they were **not** added. Versions/licenses read at the manifests
above on 2026-09-13.

| Crate | Version | License | Why | Alternative |
|---|---|---|---|---|
| `trash` | **5.2.8** | MIT | the only cross-platform, permissively licensed way to offer "move to the OS Trash" as a *fallback* action; already exercised by `czkawka_core` 12.0.2 (`trash = "5.1"` under `cfg(not(any(android, ios)))`) and by `czkawka-tauri` (`5.2.2`) | Apple `NSFileManager::trashItemAtURL` via `objc2-foundation`; manual Move to Trash via `NSWorkspace`. **Note 01 lists 5.2.6 — stale.** |
| `fs2` | 0.4.3 | MIT/Apache-2.0 | **precedent only** — the crate whose `lock_shared`/`lock_exclusive` discipline cleanser's journal relies on. We are already covered: `src/ledger.rs::open()` uses std `File::try_lock` (stabilised 1.89, `flock(2)` `LOCK_EX|LOCK_NB`), which is a *stronger* choice because it is std and non-blocking | std `File::try_lock` (current); `fs4` (successor, manifest 404 — UNVERIFIED) |
| `ignore` | **0.4.33** | Unlicense OR MIT | the only crate that implements full gitignore semantics (negation, `**`, per-directory files) — our deny-globs use the pinned `globset 0.4.20` directly | `globset` (current pin). Note its `simd-accel = []` is a documented no-op, so there is no perf reason to prefer it |
| `walkdir` | 2.5.0 | Unlicense/MIT | not needed: `dua-core = "=4.0.0"` already owns our traversal, and Czkawka's `DirTraversalBuilder` is a *pattern* not a crate | `dua-core` (current) |
| `bincode` | `<2.0` (as pinned by `czkawka_core`) | **UNVERIFIED** (repo `Cargo.toml`/`LICENSE.md` 404; crates.io API 403) | the czkawka cache format; **rejected** — our ledger must stay human-readable JSONL for auditability and CI diffing | `serde_json` (current pin) |
| — nothing else | — | — | The survey found **no crate we should add**. Every other borrowing is a *code pattern*, not a dependency | — |

## Port backlog

Ranked by value ÷ risk. Every target is a symbol that already exists in this repo.
"Borrow" means re-implementing the pattern in our own idiom — nothing here is a
code copy, and no GPL code is referenced.

| # | What to borrow | Target symbol in OUR repo | Risk | Size | Why |
|---|---|---|---|---|---|
| 1 | **Explicit-stack whole-tree emptiness proof** (`czkawka_core/src/common/fs_ops.rs::check_if_folder_contains_only_empty_folders`) — `Vec<DirEntry>` frontier instead of recursion, error naming the offending entry | `src/plan.rs::build()` (the "is this whole tree reconstructable / empty?" predicate) | **low** | **S** | A recursive emptiness check on a deep `node_modules`/`target` tree is the one place our walk can blow the stack or hide a file inside; the stack version also gives a *nameable* refusal reason (`refused` counter in `PlanTotals`) instead of a bare count |
| 2 | **Platform exclusion presets** (`common/items.rs`): macOS `*/Trash/*`, `*/.Trash-*/*`, `/Users/*/Library/Caches/*`, `*/.git/*`, `*/node_modules/*`; unix prefixes `/proc,/dev,/sys,/snap` | `src/policy.rs::compile()` (deny-globs defaults) + `src/scan.rs::start()` (`ScanOptions` ignore list) | **low** | **S** | These are upstream-tested presets that encode two things we hand-derived: `.Trash-*` (uid-suffixed trash dirs) and `Library/Caches`. Adopting them as *defaults* (never as hard denials) lowers our false-positive rate on first run |
| 3 | **Pre-execution safety re-check** (MacSlim README three-layer model, layer 3: "right before deletion, the tool-busy check runs again to guard against race conditions"; codechu independently blocks the destructive path for non-GUI callers) | `src/execute.rs::Executor::apply()` (the pre-check block before `renamex_np`) | **med** | **S** | Our plan can be minutes old; a build that started in between is currently only caught by plan-time evidence. Hoisting a cheap hot-file/MTA predicate to just before each move closes the largest TOCTOU window we have. Also feeds the D-32 `PolicyDeny` refusal path |
| 4 | **`-o json oneline` streaming records** (rmlint `docs/rmlint.1.rst`: "Print one JSON document per line … while rmlint is still running") | `src/plan.rs::write_manifest()` / `load_manifest()` | **low** | **M** | Makes the plan file appendable/incrementally readable so the GUI can render entries as they are planned instead of after a full `to_string_pretty`. Preserves `load_manifest`'s existing whole-file contract by reading line-by-line with a fallback to single-document parse |
| 5 | **Dedicated progress-sampling thread + drop-oldest coalescing** (`common/progress_stop_handler.rs`: `SEND_PROGRESS_DATA_TIME_BETWEEN = 200`, `LOOP_DURATION = 20`; `helpers/delayed_sender.rs::DelayedSender`) | `src/scan.rs::ScanEvent` / `ScanStats` sender | **low** | **M** | Decouples *how often we sample* from *how often we emit*; our `ScanProgress` throttle is already specified at ≥100 ms (SPEC §3.3) — do **not** change the interval, adopt the sampling-thread structure so a slow consumer never backpressures the walk |
| 6 | **Versioned, discardable derived cache** (`common/cache.rs`: `CACHE_VERSION = 100`, `CLEANING_TIMESTAMPS_FILE`, memory cap `8 GiB`) | `src/ledger.rs::replay()` / `record_iter()` | **med** | **M** | We currently re-scan from scratch. A version-stamped derived cache (never authoritative, always discardable on version mismatch) makes the second run fast while keeping the ledger as the only source of truth. The `CACHE_*_VERSION` discipline is the pattern; **do not** adopt bincode — keep JSONL |
| 7 | **Explicit action-class taxonomy for shell/privileged actions** (rmlint `-o sh` handlers: `clone` = `FIDEDUPERANGE`, `hardlink` fails cross-partition, `symlink`/`remove` never fail) | `src/plan.rs::Plan` + `Executor::apply` pre-checks | **med** | **M** | Gives refusal classes a principled name ("this action cannot deliver bytes on this volume") instead of ad-hoc `CrossVolume` checks, and maps directly onto our existing `CrossVolume` / `RO volume` refusals and `PlanTotals.snapshot_caveat` |
| 8 | **Per-run machine-readable receipt** (FluentCleaner `/AUTO … auto.log`: every deleted path grouped by entry + total cleaned) | `src/ledger.rs` `Kind::Apply` recs + `src/plan.rs::summarize()` | **low** | **S** | We already append `apply` recs; what is missing is a *one-call* human/agent-readable receipt (`summarize` output written alongside the manifest). Makes the "bytes freed" claim auditable without replaying the ledger |
| 9 | **Risk-tier UX vocabulary** (cleanser `RiskLevel{Safe,Moderate,Risky}` + `CleanableItem.description`) | `src/classify.rs::grade_for()` / `PathClass` + `src-tauri/src/commands.rs::start_scan` payloads | **low** | **S** | Our grades are finer, but the *labels users see* are currently the internal grade names. Mapping `PureCache→Safe`, `RebuildableFromSource→Moderate`, `Redownloadable→Risky` gives the GUI a familiar three-word axis without weakening the evidence model |
| 10 | **Restore-collision suffix** (cleanser `restore_entry()` appends `_{YYYYMMDD_HHMMSS}` when `original_path` exists) | `src/execute.rs::restore_keep_both()` + `Executor::restore()` | **low** | **S** | Independent confirmation that collision-on-restore is the norm; our `-<n>` suffix is monotonic and cheaper than a timestamp, but the *timestamp* form is what macOS Finder and Time Machine produce — surface both in the receipt so the renamed file is findable |
| 11 | **Journal lock discipline as a contrast case** (cleanser `TrashJournal::load/save` with `fs2` shared/exclusive + `with_extension("lock")`) | `src/ledger.rs::open()` | **low** | **S** | Documentation value only: our `Store::open` already holds a non-blocking exclusive `try_lock` for the `Store` lifetime and rebuilds seq/CRC chains by replay. Cite as the permissive-licensed baseline our implementation exceeds — **change nothing** |
| 12 | **External declarative rule files** (Winapp2.ini; codechu `~/.config/.../cleaners/*.json`; kandikoushik `~/.config/disk-cleaner/plugins/*.json`) | `src/policy.rs::compile()` (`PolicyError::UnrecognizedUnit`, `UNITS`) | **high** | **L** | Three independent projects converge on "rules live in a versioned user file". Natural extension of our sentence-DSL, but it is a *scope increase* (schema versioning, validation, precedence) with no acceptance criterion behind it — recommend documenting as future work, not building |


## Risks & open questions

1. **Two README-badge vs LICENSE contradictions (NEW, systemic).** MacSlim's badge says
   `license-MIT` while `LICENSE` is Business Source License 1.1; `codechu/disk-cleaner`'s
   badge says MIT while `LICENSE` is GPL-3.0. Czkawka's GitHub API record returns
   `NOASSERTION` for a repo whose own manifests state `MIT` four times. **Rule for this
   project: read the `LICENSE`/`COPYING` file, or the manifest `license` field; never a
   badge, never an API license field.** Any future candidate must be re-verified this way.
2. **Every maintenance-risk signal in this survey points the same way.** `czkawka_core` is
   frozen ("Version 12.0 is the last released version"), `czkawka-tauri` pins a core two
   majors behind, `cleanser` is at 0.7.5 with a `warn!`-only journal version check, and the
   whole mature GUI field is GPL. There is no upstream that would maintain our safety
   layer for us — which is the strongest argument that `src/execute.rs` + `src/ledger.rs`
   are the project's durable asset rather than a wheel to be re-bought.
3. **Code-copy risk is asymmetric and worth stating in the design document.** MIT/Unlicense
   code (Czkawka core, fclones, `trash`, `fs2`, `walkdir`, `ignore`, kondo) may be vendored
   with attribution; GPL-2.0/3.0 code (rmlint, QDirStat, BleachBit, dupeGuru, Filelight,
   Baobab, K4DirStat, codechu) must not be linked into a distributed binary; BSL-1.1
   (MacSlim) must not be used in production until 2030-05-01. Our root `Cargo.toml` is
   MIT/Apache-compatible today — pulling **any** GPL crate would break that.
4. **UNVERIFIED items that a reviewer could challenge** (do not present as verified):
   - **WinDirStat** version and license — `LICENSE` and `COPYING` both HTTP 404 at the
     `master` raw path. Everything said about it is from its README (features), not its
     license.
   - **FluentCleaner** — the earlier "license MISSING" reading is superseded: `LICENSE`
     (no extension) exists and is MIT; only `LICENSE.txt`/`LICENSE.md` 404.
   - **`bincode`** license and version — repo `Cargo.toml` and `LICENSE.md` 404, crates.io
     API 403. Irrelevant to a decision (we rejected bincode on format grounds), but not
     established.
   - **rmlint** version — no manifest, `meson.build`, `configure.ac`, `SConstruct` version
     field or version header resolved; only the GPL-3.0 `COPYING` and the man page were
     read.
   - **`fs4`** — successor to `fs2`, manifest 404; not needed, since std `File::try_lock`
     is what we use.
   - **MacSlim** — star count, last-push date and release history were not obtainable
     (GitHub API 403 rate limit); its Rust/Tauri facts come from `src-tauri/Cargo.toml` and
     the README only.
   - **`codechu/disk-cleaner`** — the cleaner-rule file path advertised in its README
     (`src/disk_cleaner/core/cleaners.json`) is a 404; the *format* is described in prose
     only, so treat the JSON-schema claim as unverified.
5. **Open question for the design document:** does any candidate's *unfinished* restore
   path deserve a compatibility target? Concretely — should `Executor::restore` ever read
   an existing OS-Trash or `.cleanser-trash` layout, so a user arriving from another tool
   can recover files through us? QDirStat's `restore()` stub and cleanser's
   `~/.cleanser-trash` both mean there are files on real machines in a recoverable-but-
   unowned state. This is a genuine scope question with no OSS precedent to copy.
6. **Open question:** `Plan` currently ships as pretty-printed whole-file JSON. Backlog #4
   (streaming JSONL plan) implies a format decision that affects `load_manifest()`'s
   compatibility contract. Decide before/with the GUI work, not after — a half-migrated
   manifest format is the one artifact class where we have no crash-recovery story.
7. **Non-risk, recorded to prevent re-litigation:** `czkawka_core`'s `license = "MIT"`
   for the core while `krokiet`/`cedinia` are `GPL-3.0-only` is **not** a trap — it is a
   deliberate separation by the upstream authors (core is "designed to be embedded in any
   frontend"; the Slint and Android frontends chose GPL). Vendoring `czkawka_core` alone is
   clean, and `czkawka-tauri` (MIT) is the existing precedent.

## Verdict

**There is no viable fork base.** The survey's top fork candidate on paper —
`phpfc/cleanser` 0.7.5 (**MIT**) — is the right shape (a real core/cli/gui workspace with a
Tauri 2 GUI, a trash journal, risk tiers, dry-run and whitelist) but the wrong depth: it
has no rename-exclusive move, no WAL intent/result pair, no CRC chain, no torn-tail
handling and no typed-byte gate, and forking it would mean discarding `src/execute.rs`,
`src/ledger.rs` and `src/plan.rs`, which are already written and tested. The runner-up,
`shixinhuang99/czkawka-tauri` 1.1.0 (**MIT**), fails earlier still: its frontend is React 18
with 18 Radix packages against our Svelte 5 `app/`, its `czkawka_core` pin is two majors
stale, and it moves files to the recycle bin by default with no reversibility contract of
its own. Every other mature candidate is disqualified by license (rmlint GPL-3.0, QDirStat
GPL V2, BleachBit/dupeGuru GPL-3.0, Filelight/Baobab/K4DirStat GPL-2.0/3.0, codechu
GPL-3.0) or by being BSL-1.1 (MacSlim until 2030-05-01) or by not being Rust at all
(kandikoushik Apache-2.0 Swift/Kotlin/C#; FluentCleaner MIT .NET).

**The component donor is `czkawka_core` 12.0.2 (MIT).** It is the only permissively
licensed, UI-free, pure-Rust cleanup core in the landscape — "designed to be embedded in
any frontend", with its own helpers module self-described as "ready to copy/paste to other
projects" — and there is an existing MIT Tauri embedding (`czkawka-tauri`) proving the
integration is ordinary. The borrowings worth taking are narrow and mechanical: the
explicit-stack emptiness proof, the platform exclusion presets, the sampling-thread +
drop-oldest progress pattern, the versioned discardable cache, and the `-o json oneline`
streaming precedent. The strategic value is the license split itself: it demonstrates that
a permissive core under GPL frontends is a normal, defensible arrangement, which is the
architecture this project should keep.

**What the landscape does not contain is the feature this project exists for.** No
OSS cleaner ships a working reclaim-restore loop: QDirStat's `Trash::restore()` is
documented as "currently not implemented; this function does nothing" while it ships
`rm -rf %p` under a menu item literally titled "&Delete (no way to undelete!)"; cleanser
implements restore but without any rename-exclusive or WAL hardening; fclones and rmlint
deliberately never delete at all. The verified absence of a permissively licensed
quarantine + WAL + typed-gate implementation is the evidence-backed justification for
having built `Executor::apply`/`recover()`, `ledger::Store`/`replay()` and `Plan::build()`
rather than forked one.

## Sources

1. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/Cargo.toml — `name = "czkawka_core"`, `version = "12.0.2"`, edition 2024, `rust-version = "1.94.1"`, **`license = "MIT"`**, deps (`humansize 2.1`, `rayon 1.10`, `crossbeam-channel 0.5`, `indexmap 2.11`, `blake3 1.5`, `crc32fast 1.4`, `tempfile 3.13`, `bincode "<2.0"`, `dunce 1.0.5`), features (`libraw-rs`/`libheif-rs` optional, `trash = "5.1"` under `cfg(not(any(android, ios)))`) · accessed 2026-09-13
2. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/README.md — "Core scanning library shared by Czkawka GUI, Czkawka CLI, Krokiet, and Cedinia."; "It has no UI dependency and is designed to be embedded in any frontend."; "The base build has no mandatory native dependencies - it is pure Rust."; `## License` / `MIT` · accessed 2026-09-13
3. https://raw.githubusercontent.com/qarmin/czkawka/master/krokiet/Cargo.toml — `version = "12.0.2"`, **`license = "GPL-3.0-only"`**, "Slint frontend of Czkawka Core"; same read for `czkawka_gui/Cargo.toml` and `czkawka_cli/Cargo.toml` — both `version = "12.0.2"`, both **`license = "MIT"`** · accessed 2026-09-13
4. https://raw.githubusercontent.com/qarmin/czkawka/master/cedinia/Cargo.toml — `version = "12.0.2"`, **`license = "GPL-3.0-only"`**, Android touch-friendly GUI · accessed 2026-09-13
5. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/items.rs — `DEFAULT_EXCLUDED_DIRECTORIES` (`["/proc","/dev","/sys","/snap"]`), macOS `DEFAULT_EXCLUDED_ITEMS` (`*/.git/*,*/node_modules/*,*/lost+found/*,*/Trash/*,*/.Trash-*/*,/Users/*/Library/Caches/*`), Linux/Windows variants, `ExcludedItems { expressions, connected_expressions }` · accessed 2026-09-13
6. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/fs_ops.rs — `check_if_folder_contains_only_empty_folders<P: AsRef<Path>>(path: P) -> Result<(), String>` with explicit `entries_to_check.push/pop` stack; `MAX_SYMLINK_HARDLINK_ATTEMPTS: u8 = 5`; `trash_delete<P>()` wrapper · accessed 2026-09-13
7. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/progress_stop_handler.rs — `LOOP_DURATION`, `SEND_PROGRESS_DATA_TIME_BETWEEN`, `ProgressStatus { items_counter: Arc<AtomicUsize>, size_counter: Arc<AtomicU64> }`, `check_if_stop_received` · accessed 2026-09-13
8. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/helpers/delayed_sender.rs — `DelayedSender<T> { slot: Arc<Mutex<Option<T>>>, stop_flag }` drop-oldest coalescing · accessed 2026-09-13
9. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/helpers/mod.rs — "Helper modules: generic utilities, traits, structs, **ready to copy/paste to other projects**." · accessed 2026-09-13
10. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/tool_data.rs — `CommonToolData {…}`, `DeleteMethod { OneOldest, OneNewest, AllExceptNewest, OneBiggest, AllExceptSmallest, HardLink, … }` · accessed 2026-09-13
11. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/cache.rs — `CACHE_VERSION: u8 = 100`, `CACHE_*_VERSION`, `MEMORY_LIMIT`, `CLEANING_TIMESTAMPS_FILE` · accessed 2026-09-13
12. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/dir_traversal.rs — `DirTraversalBuilder<'b, F>` fields, `Collect`, `EntryType` · accessed 2026-09-13
13. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/model.rs — `ToolType` (14 variants incl. `ExifRemover`, `VideoOptimizer`) · accessed 2026-09-13
14. https://raw.githubusercontent.com/qarmin/czkawka/master/czkawka_core/src/common/directories.rs — `Directories`, refusal to exclude `/` (`core_excluded_paths_pointless_slash`) · accessed 2026-09-13
15. https://raw.githubusercontent.com/qarmin/czkawka/master/README.md — "Version 12.0 is the last released version - no new binaries will be provided" · accessed 2026-09-13
16. https://raw.githubusercontent.com/shixinhuang99/czkawka-tauri/main/tauri/Cargo.toml — `name = "czkawka-tauri"`, `version = "1.1.0"`, edition 2024, **`license = "MIT"`**, `publish = false`, `czkawka_core = "10.0.0"`, `tauri 2`, `trash = "5.2.2"` · accessed 2026-09-13
17. https://raw.githubusercontent.com/shixinhuang99/czkawka-tauri/main/ui/package.json — `czkawka-ui` 1.1.0; `react ^18.3.1`, 18 `@radix-ui/react-*`, `@tanstack/react-table ^8.21.2`, `@tanstack/react-virtual ^3.13.2`, `jotai ^2.10.3`, `i18next ^24.2.3`, `vite ^7.1.3`, `tailwindcss ^3.4.17`, `babel-plugin-react-compiler 1.0.0` · accessed 2026-09-13
18. https://raw.githubusercontent.com/shixinhuang99/czkawka-tauri/main/LICENSE — MIT, "Copyright (c) 2025 Shixin Huang" · accessed 2026-09-13
19. https://raw.githubusercontent.com/shixinhuang99/czkawka-tauri/main/README.md and https://raw.githubusercontent.com/shixinhuang99/czkawka-tauri/main/tauri/src/main.rs — README: "Files are moved to the recycle bin by default when deleted", "Uses full file paths", default scan directory is the user directory; `main.rs`: 22 modules (`delete_files, empty_folders, ffmpeg, move_files, progress, save_result, scaner, settings, state, utils`), `set_config_cache_path("Czkawka", "Krokiet")`, `app.manage(Mutex::new(AppState::default()))` · accessed 2026-09-13
20. https://raw.githubusercontent.com/phpfc/cleanser/main/Cargo.toml — `[workspace] resolver = "2"`, members `cleanser-core`/`cleanser-cli`/`cleanser-gui`, `[workspace.package] version = "0.7.5"`, edition 2021, **`license = "MIT"`** · accessed 2026-09-13
21. https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/deletion/journal.rs — `JOURNAL_VERSION: u32 = 1`, `TrashEntry { id, original_path, trash_path, size, is_directory, deleted_at, description, category }`, `TrashJournal { version, entries, last_cleanup }`, `load()` = `lock_shared()`, `save()` = `lock_exclusive()` via `fs2` · accessed 2026-09-13
22. https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/deletion/trash.rs — `TrashConfig { use_system_trash, custom_trash_path, auto_empty_days: Some(30), max_trash_size }`, `TrashManager`, macOS `~/.cleanser-trash`, `fs::rename` + copy/remove fallback, `restore_entry()` `_{YYYYMMDD_HHMMSS}` suffix · accessed 2026-09-13
23. https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/deletion/mod.rs — module doc: strategies "Standard / Secure (DoD 5220.22-M) / Trash"; re-exports `TrashEntry, TrashJournal, SecureDeleter, DeletionMethod, TrashConfig, TrashManager` · accessed 2026-09-13
24. https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/types.rs — `ScanSpeed`, `RiskLevel { Safe, Moderate, Risky }`, `CleanableItem { path, size, category, risk_level, description }` · accessed 2026-09-13
25. https://raw.githubusercontent.com/phpfc/cleanser/main/crates/cleanser-core/src/lib.rs — module list; `ProgressCallback` trait (`on_scan_progress`, `on_clean_progress`); "can be used as a library" · accessed 2026-09-13
26. https://raw.githubusercontent.com/phpfc/cleanser/main/README.md — CLI surface (`--risk safe|moderate|risky`, `--dry-run`, `--speed quick|thorough`, `--older-than 90d`), "permanent deletion of files; use at your own risk", `brew tap phpfc/cleanser` · accessed 2026-09-13
27. https://raw.githubusercontent.com/sahib/rmlint/master/COPYING — "GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007" · accessed 2026-09-13
28. https://raw.githubusercontent.com/sahib/rmlint/master/docs/rmlint.1.rst — "rmlint itself WILL NOT DELETE ANY FILES…"; `-o json oneline`; `-o sh` handlers `clone|reflink|hardlink|symlink|remove|usercmd`; `-s/--size` multipliers; empty-files-excluded default; `-l --hardlinked` default with the "will not allocate any free space" rationale · accessed 2026-09-13
29. https://raw.githubusercontent.com/pkolaczk/fclones/main/fclones/Cargo.toml — `version = "0.35.0"`, **`license = "MIT"`**, edition 2021, `rust-version = "1.74"`, `bincode 1.3`, `blake3 1.3` optional · accessed 2026-09-13
30. https://raw.githubusercontent.com/pkolaczk/fclones/main/fclones/README.md — JSON result control, inspect-and-modify before removing, `--dry-run` · accessed 2026-09-13
31. https://raw.githubusercontent.com/shundhammer/qdirstat/master/LICENSE — GNU GPL **Version 2, June 1991** · accessed 2026-09-13
32. https://raw.githubusercontent.com/shundhammer/qdirstat/master/src/Trash.h — `License: GPL V2`; XDG trash spec; **`static bool restore(const QString &path)` doc: "This is currently not implemented; this function does nothing."**; `empty()` likewise; `TrashDirMap`, `TrashDir` / `path()` / `device()` · accessed 2026-09-13
33. https://raw.githubusercontent.com/shundhammer/qdirstat/master/src/StdCleanup.cpp — `deleteJunk` = `"rm -f *~ *.bak *.auto core"`, `hardDelete` = `"rm -rf %p"` titled `"&Delete (no way to undelete!)"` with `setAskForConfirmation(true)` + `Ctrl+Delete`, `clearDirContents` = `"rm -rf %d/*"`, `gitClean` = `"git clean -dfx"`; `RefreshPolicy`/`OutputWindowPolicy` usage · accessed 2026-09-13
34. https://raw.githubusercontent.com/shundhammer/qdirstat/master/src/Version.h — `QDIRSTAT_VERSION "2.0.01-git"`, `RELEASE_VERSION "2.0"`, `License: GPL V2` · accessed 2026-09-13
35. https://raw.githubusercontent.com/bleachbit/bleachbit/COPYING — GNU GPL **Version 3**, 29 June 2007 · accessed 2026-09-13
36. https://raw.githubusercontent.com/bleachbit/bleachbit/master/README.md — "select some options, and click Preview. Review the files, toggle options accordingly, and click Delete." · accessed 2026-09-13
37. https://raw.githubusercontent.com/arsenetar/dupeguru/master/LICENSE — GNU GPL **Version 3**, 29 June 2007 · accessed 2026-09-13
38. https://raw.githubusercontent.com/arsenetar/dupeguru/master/core/__init__.py — `__version__ = "4.3.1"`, `__appname__ = "dupeGuru"` · accessed 2026-09-13
39. https://raw.githubusercontent.com/KDE/filelight/master/src/main.cpp — SPDX header `GPL-2.0-only OR GPL-3.0-only OR LicenseRef-KDE-Accepted-GPL` · accessed 2026-09-13
40. https://raw.githubusercontent.com/GNOME/baobab/master/COPYING — GNU GPL **Version 2**, June 1991 · accessed 2026-09-13
41. https://raw.githubusercontent.com/jeromerobert/k4dirstat/master/COPYING — GNU GPL **Version 2**, June 1991 · accessed 2026-09-13
42. https://raw.githubusercontent.com/windirstat/windirstat/master/README.md — feature matrix (All Files / Largest Files / Duplicate Files / File Watcher / Extension / Treemap views; logical vs physical size; cloud-file safeguards; reparse-point exclusions; delete and Recycle Bin actions). **`LICENSE` and `COPYING` both HTTP 404 → license and version UNVERIFIED** · accessed 2026-09-13
43. https://raw.githubusercontent.com/builtbybel/FluentCleaner/main/LICENSE — **MIT**, "Copyright (c) 2026 A Belim app creation" (supersedes the earlier 404 on `LICENSE.txt` / `LICENSE.md`) · accessed 2026-09-13
44. https://raw.githubusercontent.com/builtbybel/FluentCleaner/main/FluentCleaner.Core/FluentCleaner.Core.csproj — `TargetFramework netstandard2.0`, `LangVersion latest`, `Nullable enable`, packages `System.Text.Json 8.0.5`, `IndexRange 1.0.3`, `Microsoft.Win32.Registry 5.0.0` · accessed 2026-09-13
45. https://raw.githubusercontent.com/builtbybel/FluentCleaner/main/README.md — two flavours, shared Core with Winapp2.ini parser, `/AUTO` + `/AUTO /SHUTDOWN` with per-path `auto.log`, anti-feature essays (no secure delete; no registry cleaner) · accessed 2026-09-13
46. https://raw.githubusercontent.com/edwin-hao-ai/MacSlim/main/LICENSE — **Business Source License 1.1**, Licensor "Edwin Hao / Beijing VGO Co., Ltd.", Licensed Work "MacFlow", Change Date 2030-05-01, Change License Apache-2.0 · accessed 2026-09-13
47. https://raw.githubusercontent.com/edwin-hao-ai/MacSlim/main/README.md — badge `![License](…license-MIT-green)` (**contradicts the LICENSE file**); "built with Rust and Tauri, not Electron"; three-layer safety (tool-busy check, path allowlist, pre-execution re-check) · accessed 2026-09-13
48. https://raw.githubusercontent.com/edwin-hao-ai/MacSlim/main/src-tauri/Cargo.toml — `name = "macslim"` 0.2.2, edition 2021, rust-version 1.80, **no `license` field**, `tauri 2.1` features `["macos-private-api","tray-icon","image-png"]`, release profile `panic="abort"`, `lto=true`, `strip=true` · accessed 2026-09-13
49. https://raw.githubusercontent.com/codechu/disk-cleaner/main/LICENSE — **GNU GPL v3** header, "Copyright (C) 2026 Codechu — Onur Barlık" (**contradicts the README's MIT badge**) · accessed 2026-09-13
50. https://raw.githubusercontent.com/codechu/disk-cleaner/main/README.md — Python 3.10+/GTK, process-aware scoring, `gio trash` default, dry-run, git-mtime active-project protection, control socket with the destructive path blocked, `~/.config/codechu/disk-cleaner/cleaners/`, macOS/Windows unsupported. Its advertised `src/disk_cleaner/core/cleaners.json` returns **HTTP 404** · accessed 2026-09-13
51. https://raw.githubusercontent.com/kandikoushik/disk-cleaner/main/LICENSE — **Apache License 2.0**, "Copyright 2026 Koushik Kandi" (README badge agrees) · accessed 2026-09-13
52. https://raw.githubusercontent.com/kandikoushik/disk-cleaner/main/README.md — v4.0.0 macOS Swift/SwiftUI, Windows C#/.NET, iOS SwiftUI, Android Kotlin; JSON plugin targets; Docker/Podman volume purge; LaunchAgent weekly auto-clean; "zero background telemetry" · accessed 2026-09-13
53. https://raw.githubusercontent.com/ArturKovacs/trash-rs/master/Cargo.toml — `name = "trash"`, **`version = "5.2.8"`**, **`license = "MIT"`**, `rust-version = "1.85.0"`, `default = ["coinit_apartmentthreaded", "chrono"]` · accessed 2026-09-13
54. https://raw.githubusercontent.com/danburkert/fs2-rs/master/Cargo.toml — `name = "fs2"`, **`version = "0.4.3"`**, **`license = "MIT/Apache-2.0"`** · accessed 2026-09-13
55. https://raw.githubusercontent.com/BurntSushi/walkdir/master/Cargo.toml — `version = "2.5.0"`, **`license = "Unlicense/MIT"`** · accessed 2026-09-13
56. https://raw.githubusercontent.com/BurntSushi/ripgrep/master/crates/ignore/Cargo.toml — `version = "0.4.33"`, **`license = "Unlicense OR MIT"`**, `rust-version = "1.88"`, `globset 0.4.18`, `crossbeam-deque 0.8.3` · accessed 2026-09-13
57. https://raw.githubusercontent.com/tbillington/kondo/master/kondo-lib/Cargo.toml — `version = "0.9.0"`, **`license = "MIT"`**, deps `ignore 0.4.25` + `walkdir 2` · accessed 2026-09-13
58. https://raw.githubusercontent.com/dundee/gdu/master/go.mod — `module github.com/dundee/gdu/v5`, `go 1.26.0` (version string `v5.37.0` previously resolved via the releases atom feed) · accessed 2026-09-13
59. https://raw.githubusercontent.com/kornelski/dunce/master/Cargo.toml and https://raw.githubusercontent.com/kornelski/dunce/main/Cargo.toml — **both HTTP 404**; `dunce 1.0.5` is a `czkawka_core` dependency but its license is **UNVERIFIED** · accessed 2026-09-13
60. https://crates.io/api/v1/crates/fs2 and https://crates.io/api/v1/crates/bincode — **HTTP 403 (rate limit)**; versions/licences read from upstream manifests instead · accessed 2026-09-13
