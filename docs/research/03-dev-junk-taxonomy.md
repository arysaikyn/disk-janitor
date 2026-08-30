# Developer-Junk Taxonomy (macOS) — Research Note

Canonical taxonomy of developer-machine disk junk on macOS: per-ecosystem paths,
regen command + rebuild cost, deletion risk tier (PureCache / RebuildableFromSource /
Redownloadable / Expensive / HandsOff per docs/SPEC.md), and distinguishing evidence.
· researched 2026-08-30

## Findings

### 0. Cross-cutting evidence signals (feeds classify/ engine)

Signals are ordered from cheap to expensive; every verdict must carry ≥1 strong signal.

| Signal | How to read it | Verdict impact |
|---|---|---|
| **Lockfile presence** | sibling `package-lock.json` / `pnpm-lock.yaml` / `Cargo.lock` / `yarn.lock` / `uv.lock` / `Gemfile.lock` newer than or matching dir mtime | proves deterministic regen ⇒ `RebuildableFromSource` grade safe; also enables byte-accurate rebuild-cost estimate after one timed regen |
| **Sibling manifest absence (orphan)** | cache/build dir exists, no manifest above it (`node_modules` with no `package.json` in any ancestor; `.venv` with no `pyproject.toml`/`requirements*`) | orphan tree: lockfile regen impossible (dep tree not pinned) ⇒ promote to visible "project deleted?" prompt, not auto-delete |
| **mtime recency** | max mtime of dir subtree < 10 min (SPEC threshold) | actively written ⇒ refuse auto-action (TUI: show "in use" state). Also covers running dev servers holding build dirs |
| **File locks** | open file held by process: fs usage via `dirfd`+`fstat`-style probing is fragile; robust check = `lsof +D <path>` semantics, or flock files (`.package-lock.json` `flock` marker in node_modules, uv lock file, pnpm store lock) | any lock ⇒ HandsOff until process exits |
| **git presence** | `.git` dir exists in ancestor of build cache | (a) confirms rebuild-from-source; (b) mtime of `.git/HEAD`+worktree files gives "last touched" for project activity; (c) inside quarantine move: preserve, never treat repo dir itself as junk |
| **Content-addressable store** | dir contains hash-named children (`cacache` `content-v2/sha512/…`, pnpm `files/`, uv `archive-v0`) | never delete individual entries; only prune as a whole via its own GC command (see per-ecosystem regen) |
| **APFS clone/hardlink** | `st_nlink > 1` or clone detection (compare `fs_obj_type`/fileID pairs) | freed-byte estimate must be discounted; deleting one of N links frees 0 bytes (pnpm store + node_modules links; uv hardlinks) |

### 1. Node ecosystem

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `<proj>/node_modules` | installed dependency tree (flat/hoisted for npm; symlinked virtual-store for pnpm) [S1] | `npm ci` (clean, lockfile-frozen [S2]) or `npm install`; pnpm: `pnpm install` (hardlinks from store ⇒ seconds-to-minute, low network) | RebuildableFromSource (lockfile) / Redownloadable (lockfile orphan) | lockfile sibling present/absent; `.package-lock.json` flock marker; pnpm: `node_modules/.pnpm` exists |
| `~/.npm/_cacache` | npm content-addressable cache (`cacache`), integrity-verified, self-healing; npm never GCs it itself [S1] | none needed — pure cache; regen = transparent re-download on next install | PureCache | `_cacache/content-v2/sha512/` hash dirs; grows monotonically [S1] |
| `~/.npm/_npx` | npx package cache (CLI ≥ v11 exposes `npm cache npx ls/rm/info`) [S1] | re-run `npx <pkg>` re-fetches | PureCache | `_npx/<hash>/node_modules` layout |
| pnpm store `~/Library/pnpm/store` (macOS default when no `$XDG_DATA_HOME`; `$PNPM_HOME/store` if set) [S3] | global content-addressable package store; hard-linked into every project | `pnpm store prune` removes packages not referenced by any project on the system ("not harmful … no side effects"; removed packages re-download if needed) [S4] | PureCache | `storeDir` = same-volume constraint (hardlinks) ⇒ store on non-home volume lives at `<volume>/.pnpm-store` [S3] |
| `~/.pnpm-state`, `~/Library/Caches/pnpm` | pnpm metadata + side-effect cache | re-created on install | PureCache | sibling of pnpm home dir [S3] |
| Yarn Berry `~/Library/Caches/Yarn/Berry` (global default: `enableGlobalCache: true` writes into `globalFolder` = `${HOME}/.yarn/berry` by default; per-project `.yarn/cache` when `enableGlobalCache: false`) [S5] | zip-compressed package cache shared by projects | re-fetch on `yarn install` | PureCache | `.yarnrc.yml` presence + `enableGlobalCache` value distinguishes global vs in-project location [S5]; zero-install repos COMMIT `.yarn/cache` ⇒ check git before touching |
| Yarn Classic (v1) `~/Library/Caches/Yarn/v6` [S6] | global tarball cache | `yarn cache clean`; re-fetch on install | PureCache | v6 dir naming; legacy `~/.yarn-cache` pre-2016 layout also observed [S6] |
| `.next` (default `distDir`; configurable via `next.config.js` `distDir`) | Next.js build output (dev + prod) | `next build`; dev cache re-warms on `next dev` | RebuildableFromSource | sibling `next.config.js`/`package.json`; `.next/cache` is pure cache, rest is build output |
| `.turbo/cache` (default `cacheDir`; `TURBO_CACHE_DIR`/`--cache-dir` override) | Turborepo local task cache; hashes ⇒ binary-restorable | `turbo run <tasks> --force` rebuilds | RebuildableFromSource (task) / PureCache (per-task) | `turbo.json` sibling; with remote cache linked, local cache deletion is fully recoverable [S7] |
| `.cache/`, `.parcel-cache/`, `.rollup.cache/` etc. | bundler incremental caches | rebuild via bundler command | PureCache | bundler marker files; mtime matches last build |

Note: with `npm ci`, deletion of `node_modules` is safe even mid-project — npm removes and reinstalls it wholesale [S2]; but a running dev server (vite/next dev) keeps file handles on it ⇒ file-lock signal first.

### 2. Rust ecosystem

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `<proj>/target/` | per-project build artifacts (debug/release, dep-info, incremental) | `cargo build` — cost scales with dep count; full rebuild of a medium workspace = minutes; incremental cache makes rebuilds cheap only until `target/` is removed | RebuildableFromSource | sibling `Cargo.toml`; `Cargo.lock` present ⇒ deterministic dep set [S8]; `target/debug/.fingerprint` layout |
| `~/.cargo/registry/index` | bare git repo of crates.io metadata index | auto re-clone on next `cargo build` (seconds, network) | PureCache | layout per Cargo Book [S9] |
| `~/.cargo/registry/cache/*.crate` | downloaded gzip'd crate archives | re-download on demand | PureCache | `.crate` filename = `name-version.crate` [S9] |
| `~/.cargo/registry/src` | unpacked `.crate` sources used by rustc | re-extract from `cache` (local, seconds) — cache+src duplicate content [S9] | PureCache | present iff a build consumed it |
| `~/.cargo/git/db` + `git/checkouts` | bare clones + per-commit checkouts of git deps | re-clone (network, seconds–minutes) | PureCache | checkouts reference db by hash [S9] |
| `~/.cargo/bin` + `.crates.toml`/`.crates2.json` | installed tool binaries + their install metadata ("Do NOT edit by hand") [S9] | `cargo install <name>` (compile, minutes) | Expensive | Cargo Book warns these are NOT cache [S9] |
| `~/.cargo/credentials.toml`, `config.toml` | login creds, global config | not regenerable from source | HandsOff | named as files, not caches, in Cargo Book [S9] |
| `rustup` toolchains `~/.rustup/toolchains/*` | compiler toolchains per channel | `rustup toolchain install` (download + unpack, minutes, hundreds of MB each) | Expensive | old toolchain versions are redownloadable but version-availability is not guaranteed forever |

**cargo-sweep semantics** (v0.8.0, MIT — verified in repo `Cargo.toml`) [S10]: operates on `target/` trees, not on `~/.cargo`. Selective deletion by: `--time <days>` (artifacts older than N days by mtime), `--installed` / `--toolchains <list>` (keep only artifacts built by currently-installed rustup toolchains — matches toolchain-hash directories inside `target/`), `--maxsize`, `--recursive` (walk a tree of projects), plus a `--stamp`/`--file` timestamp-marker workflow for "clean what predates my last build". Upstream is now explicitly **unmaintained** (repo banner points to `cargo-clean-all` as successor) [S10] — borrow the *rules*, not the maintenance expectation. `cargo clean` nukes `target/` wholesale. The Cargo Book itself states: "you can always remove any part of the cache and Cargo will do its best to restore sources … either by reextracting an archive or checking out a bare repo or by simply redownloading" and recommends the `cargo-cache` CLI for selective clearing [S9] (v0.8.3, MIT/Apache-2.0 verified in repo `Cargo.toml`) [S11].

### 3. Python ecosystem

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `__pycache__/` dirs (PEP 3147 layout) | bytecode cache, `foo.cpython-312.pyc` naming, per-interpreter tag | implicit on next import — effectively free | PureCache | PEP 3147: pyc files carry magic tag + timestamp; source-missing pycs are ignored, so deleting is always safe [S12] |
| `.venv/` (marker: `pyvenv.cfg`) | virtualenv, usually with pip-installed deps | `python -m venv .venv && pip install -r …` — minutes, network | RebuildableFromSource (with lockfile: `uv.lock`/`poetry.lock`/`requirements.txt`) / Expensive (no manifest ⇒ orphan) | `pyvenv.cfg` is the canonical marker; orphan check = no `pyproject.toml`/`setup.py`/`requirements*.txt` in any ancestor |
| `~/Library/Caches/pip` (macOS default; pip ≥26.2 also honors `XDG_CACHE_HOME` [S13]) | http-v2 HTTP cache + locally-built wheel cache | `pip cache purge` (all) / `pip cache remove <pkg>`; re-download/rebuild on demand | PureCache | wheel cache rebuilt-from-sdist entries are the expensive ones to regenerate (C builds) [S13] |
| `~/.cache/uv` default (`UV_CACHE_DIR`/`--cache-dir` override; **versioned buckets** `archive-v0`, `sdists-v9`, `wheels-v6`, `builds-v0`…) [S14] | aggressive multi-bucket cache (wheels, sdists, git, built artifacts) | `uv cache clean [pkg]`; `uv cache prune` removes unused entries + stale-version buckets — "safe to run periodically" [S14] | PureCache | bucket-dir naming; hardlinks into venvs (same-volume requirement) [S14]; `CACHEDIR.TAG` marker |

Key uv facts [S14]: cache is "thread-safe, append-only", "it is *never* safe to modify the cache directly (e.g., by removing a file or directory)" — verdict engine must only ever propose whole-store `prune`/`clean`, never file-level deletion. `uv cache prune --ci` keeps source-built wheels (expensive to rebuild) while dropping pre-built ones.



### 4. Xcode / iOS ecosystem

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `~/Library/Developer/Xcode/DerivedData/<Project>-<hash>/` | per-project build products + intermediate files + index store (`Index.noindex/DataStore`) | next `xcodebuild`/Xcode build — full clean rebuild, minutes–tens of minutes for real apps | RebuildableFromSource (builds) | default location per Apple Developer forum guidance + Xcode Preferences > Locations (user-customizable ⇒ resolve `com.apple.dt.Xcode` `IDECustomDerivedDataLocation` preference before trusting the default path) [S15] |
| `~/Library/Developer/Xcode/Archives/YYYY-MM-DD/*.xcarchive` | release archives w/ binary + dSYM, used by Organizer for distribution + crash symbolication | NOT regenerable without the archived build context; recreate only by re-running the identical build (often impossible) | Expensive | `.xcarchive` bundle layout; deleting loses ability to re-export/re-upload/symbolicate old releases [S16] |
| `~/Library/Developer/Xcode/iOS DeviceSupport/<iOS version> <arch>/` | symbol files extracted when a physical device is first attached | re-extract on next device attach (Xcode does it automatically; takes minutes and re-plug) | Redownloadable (re-derivable) | version-named dirs; old versions for devices no longer connected are the safe deletions |
| `~/Library/Developer/CoreSimulator/Devices/<UDID>/` | per-simulator device data (user data, installed apps) | recreate device via `xcrun simctl create` (fast) — but personal install state is lost | Redownloadable (re-derivable) | orphan signal: device UDID whose runtime is gone ⇒ `xcrun simctl delete unavailable` is the official janitor command [S17] |
| `~/Library/Developer/CoreSimulator/Caches/` | per-runtime dyld shared caches, runtime metadata, preview snapshots | rebuild on next simulator boot; first boot slower | PureCache | regenerates; third-party verified breakdown of subpaths (dyld/, downloads/*.dmg, org.apple.dt.Xcode/) [S17] |
| Simulator runtimes (cryptex DMGs; disk images managed by Xcode Settings > Components / `xcodebuild -downloadPlatform iOS` etc.) [S18] | full platform runtimes, multi-GB each | `xcodebuild -downloadPlatform <iOS\|watchOS\|tvOS\|visionOS>` re-download (GB-scale, network) | Redownloadable | Apple's Components settings UI shows "the amount of storage you can recover if you remove them" [S18] — treat as version-locked redownload, not cache |
| `~/Library/Developer/Packages/` | downloaded component packages (`-checkForNewerComponents` stores files here) [S18] | re-download | Redownloadable | `.dmg`/`.pkg` payloads |
| Swift Playgrounds (macOS app) | app projects stored as sandboxed app documents (`~/Library/Containers/…`), no classic DerivedData | re-create project; content user-authored | HandsOff | user content, not junk |

### 5. Homebrew

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `~/Library/Caches/Homebrew` (default `HOMEBREW_CACHE` on macOS) [S19] | downloaded bottles/sources/casks | re-download on `brew install/upgrade` | PureCache | brew-managed naming (`*--bottle.tar.gz`, `*--incomplete` partials) |
| `/opt/homebrew/Cellar/<f>/<old-version>/` | old keg versions after upgrade | re-`brew install <f>@<old>` (recompile/pour, minutes) — or lost if formula moved on | Redownloadable | `brew cleanup`'s own target: "Remove stale lock files and outdated downloads for all formulae and casks, and remove old versions of installed formulae" [S19] |
| stale lock/pid files in the prefix | `brew cleanup` also removes these | n/a | PureCache | zero-byte or orphaned lock files [S19] |

**`brew cleanup` semantics** [S19]: default (no args) covers all formulae/casks; `--prune=all` removes ALL cache files, `--prune=<days>` removes cache files older than N days (default prune window is 120 days); `-n/--dry-run` previews. `brew autoremove` handles dependency-orphaned formulae (with `--dry-run`). Verdict engine should *invoke* these commands rather than re-implement their logic on the prefix.



### 6. Docker / containers

| Path | What it is | Regen command + expected cost | Tier | Distinguishing evidence |
|---|---|---|---|---|
| `~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw` | sparse APFS disk image holding the whole Docker Desktop Linux VM (images, containers, volumes, build cache). Apparent size (e.g. 64 GB cap) ≫ allocated size; official FAQ documents `ls -klsh` to read real usage [S20] | images re-pull (`docker pull`, GB, network); containers/volumes NOT regenerable | HandsOff as a file. If the user wants space: drive `docker system prune` inside, never delete the raw file — official guidance is to use Docker's own reclaim paths [S20] | sparse-file signal: logical size vs allocated size diverge; `st_blocks`/allocated size is the honest number |
| `~/Library/Group\ Containers/group.com.docker/settings-store.json` | Docker Desktop settings ("settings-store.json" location documented by Docker) [S20] | not regenerable | HandsOff | config, not junk |
| other `~/Library/Containers/<bundle-id>/` app sandboxes | per-app sandbox containers (Data/Documents/Caches) | depends on app | mostly HandsOff | bundle-id naming; `Caches/` subdir inside a container behaves like a normal cache but sits under app-management, not user-visible cache root |

`docker system prune` semantics [S21]: removes stopped containers, unused networks, dangling images (all unused with `-a`), unused build cache; anonymous volumes only with `--volumes`; `--filter until=…`/`label=…` supported. Same-volume caveat from SPEC §7 applies to quarantine moves of anything under `~/Library/Containers` (bundle-id dirs live on the same APFS volume as home by default).

### 7. Browsers (dev machines often carry several)

| Path | What it is | Regen + cost | Tier | Evidence |
|---|---|---|---|---|
| Chromium-family: `~/Library/Caches/Google/Chrome/<Profile>/` mirrors `~/Library/Application Support/Google/Chrome/<Profile>` — mapping implemented in `GetUserCacheDirectory` in chromium's `chrome/common/chrome_paths_mac.mm` (profile under Application Support ⇒ cache under `~/Library/Caches` with same relative path) [S22] | HTTP cache, code cache, GPU cache | browser rebuilds automatically; small slowdowns | PureCache — but profiles themselves (`Application Support`) are user data ⇒ HandsOff | dir-per-profile structure; only delete the `Caches` mirror, never Application Support |
| Firefox: `~/Library/Caches/Firefox/Profiles/<profile>/` (`cache2` layout) vs persistent data in `~/Library/Application Support/Firefox/Profiles/<profile>` [S23] | disk cache | rebuilds on browsing | PureCache | same profile name in both trees; cache2 subdir |
| WebKit/Safari: `~/Library/Caches/com.apple.Safari/` | cache | rebuilds | PureCache | TCC-protected in modern macOS — scan may fail; treat unreadable as HandsOff |

### 8. Generic `~/.cache` (XDG) and `~/Library/Caches`

- `$XDG_CACHE_HOME` default is `$HOME/.cache`, defined as "user-specific non-essential (cached) data" [S24] — the spec's own definition makes the whole tree PureCache-*in intent*, but macOS apps increasingly use it (uv writes there when unset; observed live: `~/.cache/uv` with versioned buckets). Deleting a child dir still requires knowing the app's regen semantics — the XDG path is a *prior*, not a verdict.
- `~/Library/Caches` is the macOS-native mirror: pip (`~/Library/Caches/pip`) [S13], Yarn (`~/Library/Caches/Yarn`), Homebrew [S19], pnpm, browser cache mirrors [S22], plus bundle-id caches for sandboxed apps. Entries with mtime < 10 min are being actively written (SPEC rule) — this is where that check bites hardest.
- Apple's App Sandbox gives sandboxed apps a per-app container; per current Apple documentation, sandbox containers live under `~/Library/Containers/<bundle-id>` with app-specific data inside; user-visible `~/Library/Caches/<bundle-id>` entries are the cache-side projection for those apps [S25]. Deletion tier: container data = HandsOff unless only its `Caches/` child is targeted.
- Orphan detection heuristic: a `~/Library/Caches/<X>` or `~/.cache/<X>` entry whose owning app no longer exists (`/Applications/*.app` scan + bundle-id match) is a strong candidate — regen command is "reinstall the app", so tier is Redownloadable for cache-only apps but verdict should be surfaced, not auto-applied.



## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| holmgr/cargo-sweep (v0.8.0) [S10] | MIT (verified in repo `Cargo.toml` `license` field) | toolchain-keyed `target/` pruning rules; `--stamp` timestamp workflow | repo `src/` (sweep rules; rustup toolchain detection) |
| matthiaskrgr/cargo-cache (v0.8.3) [S11] | MIT/Apache-2.0 (verified in repo `Cargo.toml` `license` field) | per-component `~/.cargo` size accounting (`registry/cache/src`, `git/db`) | `src/main.rs` + size walk modules |
| Byron/dua-cli (dua-core 3.3.0, dua-cli 2.43.1) [S26] | MIT (crates.io license field) | parallel `jwalk`-based traversal with size+mtime aggregation; interactive TUI mark-and-sweep | `dua-core` crate |
| tbillington/kondo (v0.9.0) [S27] | MIT (crates.io license field) | known-project-structure discovery (per-language dirs considered junk) — direct precedent for our classify rules | `crates/kondo` core + language modules |
| zaldih/npkill (v0.12.2) [S28] | MIT (verified in npm registry `license` field) | UX of listing `node_modules` trees with sizes and per-item delete (Node lane only) | `src/` TS |
| tw93/Mole [S29] | GPL-3.0 — **copy-with-care**: NOT MIT/Apache/BSD; do not copy code, ideas only | whitelist/dry-run UX patterns; per-path cleanup precedent on macOS | repo (license file GPL-3.0 per GitHub) |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| `jwalk` 0.9.0 [S26] | parallel directory walking | powers dua-core; MIT; consistent with SPEC scan/ module | hand-rolled rayon walk |
| `walkdir` 2.5.0 [S26] | sequential walk, metadata | MIT; simpler fallback for sorted/serial scans | `fs` + recursion |
| `cargo_metadata` 0.23.1 [S26] | structured `cargo metadata` output | MIT; lets us enumerate workspace `target/` dirs correctly for Rust projects | parse `Cargo.toml` by hand (avoid) |
| `nix` (recent 0.x) | syscall layer: `renamex_np(RENAME_EXCL)`, `flock`, `stat` | SPEC R1 mandates nix/libc; macOS flags verified against nix docs during implementation | `libc` bindings |
| `same-file` | same-volume detection before quarantine move | cheap, MIT; pairs with SPEC §7 external-volume risk | `st_dev` comparison via libc |

## Risks & open questions

1. **DerivedData path is preference-dependent** — default is `~/Library/Developer/Xcode/DerivedData` but `IDECustomDerivedDataLocation` can move it [S15]; classification must read Xcode's prefs (or accept missing Xcode projects as orphans).
2. **Apple's own docs do not enumerate cleanup paths.** DerivedData/Archives/DeviceSupport locations rest on Apple forum guidance + Xcode Help; TN2216 (old build-locations technote) is gone from Apple's site. Recommend validating on a real Xcode machine (this Mac has only Command Line Tools; `xcodebuild`/`simctl` absent).
3. **Simulator runtime storage is macOS-version dependent** (cryptex/DMG locations moved across Xcode 15→16, `~/Library/Developer/CoreSimulator/Images` vs `Cryptex/Images`); treat runtime deletion only through Xcode Components / `xcodebuild -downloadPlatform` [S18].
4. **pnpm store is volume-bound** — on a non-home volume it lives at `<volume>/.pnpm-store` [S3]; scanner must not hard-code the home path.
5. **Yarn zero-install** projects commit `.yarn/cache` to git [S5] — deleting it breaks the repo contract; require `.git` presence check + gitignore inspection before proposing.
6. **Byte-count honesty**: hardlinks (pnpm store→node_modules, uv cache→venv) and APFS sparse files (Docker.raw [S20]) make `st_size` wrong; ledger must record allocated bytes (`st_blocks`) and discount duplicates.
7. **mtime<10-min rule needs a per-ecosystem quirk list**: e.g. npm `_cacache` mtimes update on any install (always "hot"), yet whole-store prune is still the correct action; the rule should gate *file-level* deletion, not whole-store prune commands.

## Sources

1. npm cache docs — https://docs.npmjs.com/cli/v11/commands/npm-cache (accessed 2026-08-30)
2. npm ci docs — https://docs.npmjs.com/cli/v11/commands/npm-ci (accessed 2026-08-30)
3. pnpm storeDir settings — https://pnpm.io/settings/store (accessed 2026-08-30)
4. pnpm store CLI — https://pnpm.io/cli/store (accessed 2026-08-30)
5. Yarn Berry configuration (.yarnrc.yml: enableGlobalCache, globalFolder, cacheFolder) — https://yarnpkg.com/configuration/yarnrc (accessed 2026-08-30)
6. Yarn Classic macOS cache default (`~/Library/Caches/Yarn`), PR #1638 — https://github.com/yarnpkg/yarn/pull/1638 (accessed 2026-08-30)
7. Turborepo cache docs (`.turbo/cache`, cacheDir, TURBO_CACHE_DIR) — https://turborepo.dev/docs/crafting-your-repository/caching and https://turborepo.dev/docs/reference/options-overview (accessed 2026-08-30)
8. Cargo Book — Dependency Resolution / lockfile priority — https://doc.rust-lang.org/cargo/reference/resolver.html (accessed 2026-08-30)
9. Cargo Book — Cargo Home layout — https://doc.rust-lang.org/cargo/guide/cargo-home.html (accessed 2026-08-30)
10. cargo-sweep README + Cargo.toml (v0.8.0, MIT, unmaintained banner) — https://github.com/holmgr/cargo-sweep (accessed 2026-08-30)
11. cargo-cache Cargo.toml (v0.8.3, MIT/Apache-2.0) — https://github.com/matthiaskrgr/cargo-cache (accessed 2026-08-30)
12. PEP 3147 — PYC Repository Directories — https://peps.python.org/pep-3147/ (accessed 2026-08-30)
13. pip caching (macOS `~/Library/Caches/pip`, http-v2, XDG since 26.2) — https://pip.pypa.io/en/stable/topics/caching/ (accessed 2026-08-30)
14. uv Caching (buckets, prune/clean semantics, same-volume linking) — https://docs.astral.sh/uv/concepts/cache/ (accessed 2026-08-30)
15. Apple Developer Forums — Where is my DerivedData folder (default `~/Library/Developer/Xcode/DerivedData`, Preferences > Locations) — https://developer.apple.com/forums/thread/696922 (accessed 2026-08-30)
16. Xcode Help — About Archives organizer (`~/Library/Developer/Xcode/Archives`) — https://help.apple.com/xcode/mac/current/en.lproj/dev1bc569500.html (accessed 2026-08-30)
17. CleanMyDev — simctl delete unavailable & CoreSimulator Caches breakdown (third-party; corroboration) — https://cleanmydev.com/blog/xcrun-simctl-delete-unavailable-guide/ , https://cleanmydev.com/blog/core-simulator-caches-mac/ (accessed 2026-08-30)
18. Apple — Downloading and installing additional Xcode components (`xcodebuild -downloadPlatform`, `-checkForNewerComponents` → `~/Library/Developer/Packages/`, Components settings recoverable-space UI) — https://developer.apple.com/documentation/xcode/downloading-and-installing-additional-xcode-components (accessed 2026-08-30)
19. brew(1) manpage — `cleanup`, `--prune`, `autoremove`, `HOMEBREW_CACHE` default `~/Library/Caches/Homebrew` — https://docs.brew.sh/Manpage (accessed 2026-08-30)
20. Docker Desktop Mac FAQ (Docker.raw location `~/Library/Containers/com.docker.docker/Data/vms/0/data`, sparse behavior, `settings-store.json`) — https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/ and https://docs.docker.com/desktop/settings-and-maintenance/settings/ (accessed 2026-08-30)
21. docker system prune CLI reference — https://docs.docker.com/reference/cli/docker/system/prune/ (accessed 2026-08-30)
22. Chromium source — chrome/common/chrome_paths_mac.mm (GetUserCacheDirectory mapping Application Support ⇒ ~/Library/Caches) — https://chromium.googlesource.com/chromium/src/+/refs/tags/143.0.7499.18/chrome/common/chrome_paths_mac.mm (accessed 2026-08-30)
23. Mozilla support — Firefox cache2/profile cache locations on macOS — https://support.mozilla.org/en-US/questions/1367367 and Firefox source docs Profiles Service — https://firefox-source-docs.mozilla.org/toolkit/profile/index.html (accessed 2026-08-30)
24. XDG Base Directory Specification v0.8 ($XDG_CACHE_HOME ⇒ $HOME/.cache) — https://specifications.freedesktop.org/basedir-spec/latest/ (accessed 2026-08-30)
25. Apple — App Sandbox documentation (containers; entitlement keys) — https://developer.apple.com/documentation/security/app-sandbox and https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html (accessed 2026-08-30)
26. crates.io pages (license/version fields verified): walkdir 2.5.0 (Unlicense/MIT), jwalk 0.9.0 (MIT), cargo_metadata 0.23.1 (MIT), dua-core 3.3.0 (MIT), dua-cli 2.43.1 (MIT) — https://crates.io/crates/walkdir , /crates/jwalk , /crates/cargo_metadata , /crates/dua-core , /crates/dua-cli (accessed 2026-08-30)
27. kondo crates.io page (v0.9.0, MIT) — https://crates.io/crates/kondo (accessed 2026-08-30)
28. npkill npm registry record (v0.12.2, MIT) — https://registry.npmjs.org/npkill/latest (accessed 2026-08-30)
29. tw93/Mole GitHub (GPL-3.0) — https://github.com/tw93/Mole (accessed 2026-08-30)

