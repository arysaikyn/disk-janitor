# Fixture trees — `tests/fixtures/`

Synthetic junk-shape trees for tests and demos (SPEC §7). **Synthetic data only —
never point this at a real home directory or project.** The generator refuses
roots outside `/tmp` and existing non-empty roots unless `--force` is passed;
building at `/` is always refused.

## Regenerate

    tests/fixtures/gen.sh /tmp/fx-demo              # all 10 fixtures, scale 1
    tests/fixtures/gen.sh /tmp/fx-demo --scale 10   # demo sizing (bigger trees)
    tests/fixtures/gen.sh /tmp/fx-demo --set demo-app  # one fixture only

`gen.sh` is a thin wrapper: `exec cargo run --quiet --bin fixture-gen -- "$@"`.

| Flag | Meaning |
|---|---|
| `--set <name>` | one of `demo-app`, `orphan-node`, `venv`, `derived-data`, `cacache`, `pnpm-store`, `sparse`, `symlink-chain`, `trash-janitor`, `hot-project` (default: all) |
| `--scale N` | multiply per-fixture **file counts** (default 1; byte sizes are fixed per shape) |
| `--force` | override the `/tmp`-only and non-empty-root refusals (still never deletes anything, still never `/`) |

## Fixture → evidence mapping (SPEC §3.2)

| Fixture | Shape | Evidence signals | Expected grade |
|---|---|---|---|
| `demo-app/` | `package.json` + `package-lock.json` (lockfile **newer** than the `node_modules/` dir), `node_modules/` ≈ 1 MiB (4 × 256 KiB × scale) | `LockfilePresent` (manifest + lockfile in ancestors) | `RebuildableFromSource` (regen `npm ci`) |
| `orphan-node/` | bare `orphan-node/node_modules/` with files; **no** `package.json` anywhere above | `OrphanTree` (no manifest in any ancestor) | `Expensive` — prompt only, never auto-delete |
| `.venv/` | `pyvenv.cfg` marker, `bin/`, `lib/python3.12/site-packages/*.py` (8 × scale); no `pyproject.toml`/`requirements.txt` above | `OrphanTree` (orphan venv) | `Expensive` |
| `DerivedData/` | `Info.plist` + `DemoApp-1A2B3C4D/Build/Products/Debug/*.o` (6 × scale), `Logs/` | XcodeDerivedData path class | `RebuildableFromSource` (regen `xcodebuild build`) |
| `_cacache/` | `content-v2/sha512/aa/bb/<64-hex>/data` (≥ 8 × scale, 256 KiB each) + `index-v5/<hex>` entries | CasStore (hash-named children) | `PureCache` — whole-store action only |
| `pnpm-store/` | `files/` content store; `files/a` hardlinked into `pnpm-linked/node_modules/foo/a` (`nlink == 2`) | `CasStore("pnpm")` + `Hardlinked` (deleting one of N frees 0) | `PureCache` |
| `sparse.img` | 64 MiB apparent, ~8 KiB allocated (`set_len` + two 1 KiB writes) | `Sparse` (apparent ≫ allocated) | `HandsOff` as a file |
| `symlink-chain/` | `a → b → c → missing` (broken tail) + `loop-x ↔ loop-y` (cycle); relative targets | walker must not descend symlinks / loops (`Kind::Symlink`) | — |
| `.Trash-janitor/` | mode `0700`, `UF_HIDDEN` (`chflags`, macOS) | quarantine-root shape for M2 execute tests; generator verifies `st_flags` in-program | — |
| `hot-project/` | `package.json` + `node_modules/hot.js` with mtime = **now** (everything else cold) | `MtimeHot` (subtree written within the 600 s window) | `HandsOff` |

Cold mtimes (`now − 2h`) on every other tree keep them outside the 600 s
hot-mtime window, so they are *not* `MtimeHot`.

## Classify interaction notes

- **Below-min gate at scale 1.** `Thresholds.min_bytes` is 50 MiB; most scale-1
  fixtures are ~0.5–2 MiB, so a default `janitor scan` over the fixtures yields
  no candidates — by design (SPEC: size gates are classify's job, fixtures build
  shape). Demos should pass `--scale` (e.g. `--scale 10` for ~10–40 MiB trees;
  push higher to cross the 50 MiB gate).
- **Orphan promotion.** M1 classify grades a recognized-class dir `Expensive`
  when no build manifest exists in any ancestor (promotion rule [03 §0]). Only
  `demo-app/` and `hot-project/` carry their own `package.json`, so scanning the
  bare fixture root surfaces the others as orphan ⇒ `Expensive` — the intended
  rule in action, because those trees genuinely have no manifest ancestor. To
  demo the class grades, scan `demo-app/` or `hot-project/` directly, or wrap a
  fixture in a project dir with a manifest.
- **`UF_HIDDEN` is not yet a classify signal in M1.** The M1 flag mask is
  `UF_IMMUTABLE | SF_RESTRICTED | SF_NOUNLINK | UF_DATAVAULT | SF_DATALESS`, so
  `.Trash-janitor`'s `UF_HIDDEN` does not surface as `FlagBlock` evidence yet.
  It is the M2 quarantine-root shape; fixture-gen prints its `st_flags` so M2
  syscall tests can assert the flag survives rename into the quarantine.
- **`SF_DATALESS` cannot be synthesized in userspace** (kernel-synthetic,
  read-only [05 §6]); unit tests mock the flag on metadata-like values. The
  fixture set therefore has no dataless entry — expected, not a gap.

## Notes

- Allocated-bytes figures in the summary use `st_blocks * 512` (approximate;
  a hardlinked file's blocks are attributed per path, matching the walker).
- Deterministic content: every file is written with a fixed filler byte
  pattern, so byte sizes are exact and re-runs are byte-identical.
- `--force` still never deletes anything; it only permits building into an
  existing non-empty root or outside `/tmp`.
