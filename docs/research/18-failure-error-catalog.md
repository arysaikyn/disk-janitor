# Failure & Error Catalog — Research Note
Every failure mode a disk-cleanup app hits on macOS (plus Windows/Linux deltas), how competitors surface errors, and a paste-ready E-#### catalog with CN/EN copy in P1 format (verdict + evidence + alternative) · researched 2026-09-01

> Binds to SPEC Part B §3.4 (execute/ failure handling column), Part C §5.2 (拒绝+证据+替代
> refusal format), Part A §7 (open questions). Prior notes: 05 (FS edge cases), 07 (agent/
> network), 08 (WAL/ledger), 09–11 (platform taxonomies/primitives), 02 + 13 (competitor
> error surfaces). This note supplies the catalog rows; SPEC integration is a later decision.

## Findings

### 1. How competitors surface errors (from notes 02 + 13, verified sources)

| Tool | Error-surfacing pattern | Lesson for us |
|---|---|---|
| CleanMyMac [02 §1] | scan errors collapse into a warning chip ("Some items skipped"); repair prompts route to System Settings; no error codes exposed | surface the *skipped set* as a refusal list (P1), never silently drop |
| DevCleaner [13 §E] | permission gaps show as per-toolchain disabled rows with a "Grant Access" CTA | per-category degraded state, not a global error banner |
| DaisyDisk [02 §1][13 §C] | read-denied dirs render "hidden space" with an admin-scan CTA | TCC denial is a *first-class state* (SPEC Part C §4.1 pre-flight), not an error popup |
| Windows Storage Sense [09 §1] | silently skips what it cannot clean; no per-item disclosure | we refuse loudly instead (P1) |
| AppCleaner [02 §1] | blocked system apps show a lock glyph with "protected" reason | matches our 保护/Protected RefusalRow (note 16 §10) |

### 2. Failure-mode inventory (macOS baseline + platform deltas)

Sources: SPEC Part B §3.4 failure-handling column (root/op-dir/pre-checks/move/post/restore/
cross-volume/WAL rows), notes 05 (EXDEV, dataless EDEADLK, UF_IMMUTABLE/SF_RESTRICTED/
SF_NOUNLINK flags, ELOOP), 08 (WAL disk-full, corrupt tail, flock), 07 (endpoint/key/rate/
budget), 09–11 (Win/Linux deltas), SPEC Part C §5.2 (existing refusal rows).

### 3. The catalog (paste-ready appendix for SPEC Part C)

Legend: **recoverable** = product can resume with user action; **fatal** = operation aborts
cleanly, WAL-consistent (SPEC Part B §5.2). Copy is CN primary / EN secondary (D-02), P1
shape (判定 verdict / 证据 evidence / 替代 alternative) [SPEC Part C §5.2].

| Code | Failure / syscall | Detect at | Severity | 判定 verdict + 证据 evidence + 替代 alternative (CN) | EN |
|---|---|---|---|---|---|
| E1001 | TCC denial (~/Library, external vol) — `EPERM` on probe | plan pre-flight / scan [SPEC C §4.1] | recoverable | ⚠ 无法读取 cannot read · 证据: probe EPERM · 替代: [打开系统设置] 或 排除该根 | grant Full Disk Access or exclude root |
| E1002 | Volume unmounted mid-apply — `ENOENT` on quarantine root | execute, per entry | fatal (batch aborts, WAL intact) | ⚠ 卷已卸载 volume unmounted · 证据: mntonname resolved empty · 替代: 重新挂载后重试或 恢复 restore | remount and retry, or restore |
| E1003 | Permission denied / read-only tree — `EACCES`/`EROFS` | scan / pre-check | recoverable | ⚠ 无法隔离 cannot quarantine · 证据: EACCES on path · 替代: 只读分析 analyze read-only | analyze read-only |
| E1004 | Disk FULL while writing WAL/ledger — `ENOSPC` (worst case: quarantine move fails mid-batch) | execute, before each `wal.append` [08 §8] | fatal (refuse-safely: never move without journal) | ⚠ 磁盘已满，日志写入失败 disk full, journal write failed · 证据: ENOSPC on <wal> · 替代: 释放 ≥ 1 GiB 后重试，未移动任何文件 | free ≥1 GiB, nothing was moved |
| E1005 | Dataless iCloud file — `EDEADLK` under IOPOL_MATERIALIZE_DATALESS_FILES_OFF | pre-check [05 §5] | recoverable | ⚠ 云占位文件 dataless · 证据: SF_DATALESS flag · 替代: 标记跳过，不下发下载 | skip; never triggers download |
| E1006 | Symlink loop — `ELOOP` | walker / pre-check [05 §1] | recoverable | ⚠ 符号链接循环 symlink loop · 证据: ELOOP at path · 替代: 已跳过，显示为链接而非目录 | skipped; shown as link |
| E1007 | EXDEV — cross-volume move refused (even APFS-within-container) | pre-check `statfs` both ends [05 §2][SPEC B §3.4] | recoverable | ⚠ 跨卷隔离不支持 cross-volume quarantine unsupported · 证据: st_dev mismatch · 替代: 该卷只读分析 | analyze this volume read-only |
| E1008 | Quarantine root creation failure — `EACCES`/`EXDEV` on `<mnt>/.Trash-janitor/` mkdir | execute open | fatal | ⚠ 无法创建隔离区 cannot create quarantine · 证据: errno on mkdir 0700 · 替代: 检查卷权限后重试 | check volume permissions |
| E1009 | Purge on vanished volume — quarantine root gone | purge | fatal (typed-confirm already given; abort cleanly) | ⚠ 隔离区不存在 quarantine root missing · 证据: ENOENT · 替代: 重新挂载卷，未销毁任何数据 | volume missing; nothing destroyed |
| E1010 | Corrupt ledger/WAL tail — CRC32 mismatch | startup recovery [08 §4][SPEC B §5.2] | recoverable (skip-and-continue) | ⚠ 日志尾部损坏，已跳过 corrupt tail skipped · 证据: CRC mismatch at seq N · 替代: 查看导出 恢复报告 | see recovery report |
| E1011 | EEXIST at apply — occupant appeared | move `RENAME_EXCL` [05 §1] | recoverable | ⚠ 目标已存在 target exists · 证据: EEXIST · 替代: 自动改名 -1（不覆盖） | auto-rename -1, never overwrite |
| E2001 | Agent endpoint down — connect error / timeout | agent loop (note 07) | recoverable | ⚠ 模型服务不可达 model unreachable · 证据: connect failed after N retries · 替代: 继续本地流程 continue local flow | continue offline |
| E2002 | Bad API key — HTTP 401 | agent loop | recoverable | ⚠ 密钥无效 invalid key · 证据: 401 · 替代: [设置 密钥] 或 继续本地流程 | fix key in Settings |
| E2003 | Rate limit — HTTP 429 | agent loop | recoverable | ⚠ 限流 rate limited · 证据: 429 + Retry-After · 替代: 等待后重试或 本地继续 | wait and retry |
| E2004 | Token budget exceeded — usage vs config prices | agent loop [SPEC C §5.2 Budget row][09 budget] | recoverable | ⚠ 预算已达上限 budget reached · 证据: $X / Y tok / Z turns · 替代: 提高上限或本地继续 | raise cap or continue locally |
| E3001 | Config error — missing key/model/endpoint | config load | recoverable | ⚠ 配置缺失 config incomplete · 证据: field absent in config.toml · 替代: [打开设置] | open Settings |
| E3002 | Windows delta: pre-1709 MoveFileExW fallback engaged [09 §delta][SPEC B §5.5] | execute probe | recoverable | ⚠ 兼容模式：恢复由 WAL 保证 compatibility mode: restore via WAL · 证据: OS < 1709 · 替代: 已自动启用，详见说明 | see docs |
| E3003 | Linux delta: `RENAME_NOREPLACE` on NFS/fuse — `EINVAL` [11 §1] | execute probe | recoverable | ⚠ 该卷不支持原子隔离 atomic quarantine unsupported · 证据: EINVAL on renameat2 · 替代: 只读分析 | analyze read-only |

### 4. Catalog rules (how the table stays honest)

1. **Every fatal row must leave the system WAL-consistent** (SPEC Part B §5.2 invariant) —
   a code is "fatal" only when the recovery path is a clean abort + journal replay.
2. **Never surface a raw errno alone** — always 判定 + 证据 + 替代 (P1). The evidence field
   may carry the syscall name; the verdict never does.
3. **E-series are stable identifiers** — once shipped, a code's meaning never changes
   (ADR-17 format versioning); new failures get new codes, not repurposed ones.
4. **Platform deltas are explicit codes** (E30xx), not hidden behavior — the honest-guarantee
   posture of SPEC Part A §8 / Part B §5.5.
5. **Toast vs row**: transient recoverable states (network) render as toasts; persistent
   refusals render as RefusalRows in context (P1 shape) [SPEC C §5.2]. Disk-full (E1004) is
   the one fatal that deserves a modal — the only action is user-side space reclamation.

## Reusable code & assets

| Repo/Package | License (verified) | What to borrow | Where |
|---|---|---|---|
| (none new) | — | The catalog is copy/UX design, not code; errno mapping already lives in execute/ (nix/libc, [05 §reusable]) | error.rs / catalog |

## Recommended stack additions

None — error mapping rides on existing deps (nix Errno, thiserror-style enums already
planned in core [SPEC Part D §1e]); no crate carries error *UX*. Optional future: an
`i18n` string table for the CN/EN pairs (svelte-i18n already pinned for GUI [12 §npm]).

## Risks & open questions

1. **E1004 behavior under test** — "refuse before moving" is a hard invariant; needs an
   explicit fixture test with a near-full tmpfs (course R1/R4 mapping).
2. **Toast vs banner taxonomy** needs a visual spec row in SPEC Part C (severity → surface
   mapping) before implementation.
3. **E2002/E2003 retry policy** — backoff parameters (backon crate) must be configurable
   and counted; a runaway retry loop is a cost-meter bug.
4. **Windows-only codes enumerated?** — E3002 is v1-known; Storage Sense-style silent-skip
   (E3004 candidate) should be added when the Windows execution path lands (note 19).

## Sources

Grounded in verified in-repo corpus (accessed 2026-09-01): SPEC Part B §3.4/§5.1–5.2/§5.5,
Part C §4.1/§5.2, Part A §7–8, ADR-10/12/15/17; notes 02 (§4 safety models), 05 (syscall
edge cases §1/§2/§5/§6/§7), 07 (provider/failures §network), 08 (WAL §4/§6/§8), 09–11
(platform deltas), 12 (GUI stack), 13 (competitor surfaces §B/§C/§E). No fresh web fetches
were possible 2026-09-01 (all search providers TLS-down); competitor rows cite notes 02/13,
whose own source lists are verified. [INFERENCE] flag: E1004's "refuse before moving" is a
design decision derived from the WAL-first invariant, not a competitor practice.