# Quarantine Vault & Restore UX — Research Note
Restore/undo UX in deletion tools (macOS Recently Deleted, Time Machine, trash apps, CleanMyMac, git reflog) → design for Disk Janitor's quarantine vault, 72 h grace countdown, restore-collision policy, WAL-replay surfacing, and ledger→restore entry · researched 2026-09-01

> Bound to SPEC Part B §3.4 (execute/ quarantine·purge·restore·WAL), §5.1–5.2 (quarantine invariants + WAL/ledger sequence + startup recovery), Part C §3.7 (台账 Ledger wireframe) and §4.4 (Restore — the undo moment). ADR-10 freezes the 72 h grace constant; this note designs only its *presentation*. Notes 02 (macOS cleanup landscape) and 05 (FS edge cases) are priors, not duplicated.

## Findings

### 1. macOS "Recently Deleted" family — the reference grammar for grace-window restore

Apple ships one shared mental model across Photos, Mail, Messages, Notes, Voice Memos, and iCloud Drive: a side-table/folder called **Recently Deleted**, a fixed retention window, per-item recovery to the *original* location, and a hard cut-off after which items are gone.

| App | Retention | Where it lives | Recovery action | Countdown shown | Empty-state behavior |
|---|---|---|---|---|---|
| Photos (macOS/iOS) | 30 days [S1] | "Recently Deleted" album, sidebar → Utilities; **locked** (Touch ID/password) [S1] | select → **Recover** (single) or **Recover All** [S1]; returns to original album | per-thumbnail **days-remaining** badge [S2] | album **disappears from sidebar** when empty [S3] |
| Mail | user-configurable: None / 1 day / 1 week / 1 month [S4] | Trash mailbox (or iCloud "Recently Deleted" mailbox, 30 d [S5]) | drag from Trash → another mailbox [S4] | none — just the mailbox list | mailbox simply empty |
| Messages | 30 days on-device; 40 days to iCloud purge [S6] | View → Recently Deleted [S6] | select → Recover [S6] | none per-item | "If you don't see Recently Deleted, you likely haven't deleted a conversation within the last 30 days" [S6] — **frames absence as normal, not broken** |
| iCloud Drive / Files | 30 days [S7] | Files → Browse → Recently Deleted [S7] | select → Recover; **returns to original location** [S7] | none surfaced | folder absent when empty |
| Voice Memos | configurable: Immediately / 1 / 7 / 30 days / Never [S8] | Recently Deleted list | select → Recover | none | list absent when empty |

**Design lessons for Disk Janitor:**

- **Recovery returns to the original location by default** (Photos, iCloud Drive) — matches our WAL `(original, quarantine)` reverse-rename. Apple never asks "where do you want it back?"; the original path is the contract. [S1][S7]
- **Per-item countdown is shown only by Photos** (days-remaining badge) [S2]; every other app hides the timer and relies on the fixed-window promise. Disk Janitor's 72 h window is *much shorter* than Apple's 30 d, so the countdown is load-bearing — users cannot rely on "roughly a month"; they need to see the hours left. This inverts Apple's pattern: we *must* surface the timer where Apple can afford to hide it.
- **Apple's empty state is invisibility** (album/folder vanishes). That works for a side-table but is wrong for a *vault that holds the user's files* — an empty vault should reassure ("0 files at risk · nothing is being held"), not disappear, because the vault's *existence* is part of the safety promise (恢复是权利). See §7.
- **Photos locks Recently Deleted behind auth** [S1]. Our quarantine holds real file paths; a vault view that lists original paths is a privacy surface. Consider gating the vault behind the same TCC/auth posture as the ledger (the ledger already stores real paths locally [SPEC Part B §3.5 privacy asymmetry]).

### 2. Time Machine — browse-and-restore, and the canonical collision triad

Time Machine is the only Apple tool that performs *browse-then-restore* of arbitrary files (not app-managed objects), so its collision behavior is the closest precedent to our restore-from-quarantine.

- **Interface**: star-field backdrop + vertical timeline of backup snapshots; enter via menu bar or Applications; browse the original folder across time; select item(s) → **Restore** copies from backup to the original location [S9].
- **Collision policy (the evidence we need)**: when the restored file/folder's name already exists at the destination, macOS presents **three options** [S10][S11][S12]:
  - **Keep Both** — the restored item is written alongside the existing one; the existing item is renamed (Apple adds a suffix — see §5 for the exact suffix grammar) so nothing is lost.
  - **Replace** — the existing item is overwritten by the restored version (destructive for the current copy).
  - **Stop** — cancel the restore entirely.
  - (One source distinguishes "Keep Original" = keep current, don't restore [S11], a variant of Stop.)
- **No bulk restore-all with auto-rename**: each restore is contextual to the folder being browsed; there is no "restore everything in the backup" button. Bulk restore iterates and re-prompts per conflict.

**Lesson**: the three-option triad (Keep Both / Replace / Stop) is the industry-default collision UX for restore. Our restore is a *reverse rename* (not a copy), so "Keep Both" maps to our existing `RENAME_EXCL` retry-with-`-<n>` suffix (SPEC Part B §3.4 already does this at *apply* time on `EEXIST`). Reusing the same suffix-retry at *restore* time gives us Keep-Both semantics for free; we then surface the choice (rename / skip / overwrite) explicitly rather than silently, per P1/P6.

### 3. Trash apps & CleanMyMac — the undo gap our vault fills

- **TrashMe 3 (Jibapps)**: the Dashboard has a **"History"** section showing the *accumulated size* of files deleted via TrashMe; "Show" reveals the full history; clearing history resets the counter [S13]. This is a **statistics log, not an undo channel** — TrashMe moves files to the Finder Trash, so recovery is only macOS Put Back while Trash isn't emptied. There is no per-item in-app restore. [S13]
- **AppCleaner (FreeMacSoft)**: moves app + related files to Trash; **Put Back possible** while Trash holds them [note 02 §1]. No in-app history or restore log. Apple Community confirms Put Back is sometimes **missing for AppCleaner-deleted items** (app-deleted files bypass Finder's original-path tracking) [S14][S15] — so even the OS-level restore is unreliable for cleaner-trashed items.
- **CleanMyMac (MacPaw)**: default "Move to Trash" → recovery ends when Trash is emptied; "Remove Immediately"/"Remove Securely" are **deliberately irreversible** [note 02 §1]. **No in-app undo** once Trash is emptied; no history/restore log comparable to a ledger [note 02 §1].

**Synthesis**: no surveyed cleaner owns a restore model richer than "Put Back while Trash isn't emptied." TrashMe's history is size-accounting, not recovery; AppCleaner's Put Back is unreliable; CleanMyMac has none. This is exactly the gap Disk Janitor's WAL-backed, ledger-addressable, 72 h vault closes — and it is the differentiator to show on camera [note 13 §G1]. The vault must therefore present itself as a *first-class restorable store*, not a trash-adjacent log: per-item restore buttons, countdown, and a restore that works even after relaunch (where Put Back metadata would already be gone).

### 4. Git reflog — the last-resort recovery model, and why our vault beats it

- `git reflog` is git's local record of every ref movement; recovery via `reset --hard <sha>`
  or `checkout` works only if the reflog entry survives — gc prunes unreferenced objects
  after `gc.reflogExpire` (default 90 days) or `gc.reflogExpireUnreachable` (30 days)
  [08 §reflog class].
- The reflog's UX lesson: **recovery needs an addressable record** (a commit SHA), not just
  a copy of the bytes. Our WAL op-id (`<quarantine>/<wal-op-id>/`) plays that role; the
  ledger's `apply` record (`plan_id`, `op_id`, `original`, `quarantine`, `bytes_allocated`)
  is the user-facing address — 台账 #842-style restore entries [SPEC Part C §3.7].
- Unlike reflog, ours is **time-bounded by design** (72 h, ADR-10) and **user-inspectable**
  (ledger is never silently truncated, P5). Reflog hides its entries from non-CLI users;
  our vault is a first-class screen.
- Reflog also teaches expiry honesty: when an object is truly gone, git says so and stops,
  never pretending. Our expired-grace rows must say 已过期 must restore now or they will be
  destroyed — a visible countdown, never silence [ADR-10].

Lesson: the vault = reflog's addressability + Trash's per-item restore + Apple's countdown
badge, none of which any single competitor combines.

### 5. Collision policy when the original path is re-occupied — our triad

The apply-time retry already appends `-<n>` on `EEXIST` (SPEC Part B §3.4). The same
primitive reused at restore time gives us the industry-standard triad:

| Choice | Behavior | Evidence |
|---|---|---|
| 重命名 rename (default) | reverse rename to `original-<n>` via `renameatx_np` with `EEXIST`-retry; **nothing is overwritten** | Time Machine "Keep Both" [02 §4] |
| 跳过 skip | leave the current occupant; ledger records `restored: skipped` with reason | Windows Recycle Bin "skip" [S-w1] |
| 覆盖 replace | only after typing the byte count of what will be overwritten (P6: never silent) | Time Machine "Replace" [02 §4] |

Restore is a soft gate (safe direction; friction belongs to destruction, P4) — so the
confirmation sheet offers all three but defaults to rename and needs no typing. The
overwrite choice reuses the typed-byte modal so "replace" is never accidental [SPEC Part C
§4.2/§4.4]. Purged entries render 已销毁，无法恢复 destroyed — not restorable [SPEC Part C §4.4].

### 6. Vault view — layout, countdown, and states

Specified as an addition beside SPEC Part C §3.7 (Ledger screen) — the vault is the
**actionable subset of the ledger**: only `apply` records inside the grace window, grouped
by plan session (plan-YYYYMMDD-HHMM), each row showing quarantine size + countdown.

- **Countdown**: per-row 剩 Xh Ym of 72 h (Photos-style badge [S2]); warning tint <12 h;
  expiry is a *promotion*, never a surprise — expired entries move to a 待销毁 pending
  section listing them as purge candidates with explicit 「恢复窗口已过 expired — 若需保留请
  立即取消或恢复」 copy [SPEC Part C §3.7 "never auto-purged" ADR-10].
- **Empty state**: 0 files at risk · nothing is being held — the vault's existence is part
  of the safety promise 恢复是权利; do NOT make it vanish like Apple's Recently Deleted.
- **Restore-all**: per-session [恢复本批全部 Restore all] with a collision summary counted
  *before* acting (N will rename, M skipped) — Time Machine has no bulk-restore; we add it
  because our reverse renames are batch-addressable via the plan.
- **Relaunch surfacing**: startup recovery (SPEC Part B §5.2) produces a RecoveryReport;
  the vault shows a banner 「上次中断的隔离已恢复记录 3 项」 with a Restore shortcut —
  the WAL-replay surfaced as a user benefit, not a log line.

### 7. Bilingual copy for the key states (CN primary / EN secondary, D-02)

| State | CN | EN |
|---|---|---|
| Vault title | 恢复站 Recover | — |
| Countdown | 剩 47 小时 22 分 of 72 h | 47h 22m of 72h left |
| Expired | 恢复窗口已过 | restore window closed |
| Restore CTA | 恢复 Restore | — |
| Restored banner | 已恢复 · 回到原位 | restored — back where it was |
| Collision offer | 原位已有文件：改名 / 跳过 / 覆盖 | file exists: rename / skip / replace |
| Empty vault | 0 个文件待保护 · 没有扣押任何东西 | 0 files at risk · nothing is being held |
| Interrupted apply | 上次中断已恢复记录 | resumed records from last interrupted apply |

## Reusable code & assets

| Repo/Package | License (verified) | What to borrow | Where |
|---|---|---|---|
| (none new) | — | Restore UX patterns come from OS conventions (Time Machine triad, Photos countdown, Recently Deleted grammar), not copyable code; WAL reverse-rename already spec'd [SPEC Part B §3.4] | vault screen |

## Recommended stack additions

None — the vault renders from `ledger` records over the existing `ipc::Channel` stream
[SPEC Part C §2.3]; no new dependency. Suffix-retry reuses `RENAME_EXCL` + `-<n>` retry
already pinned in `execute/` [SPEC Part B §3.4].

## Risks & open questions

1. **Vault vs Ledger screen overlap** — the vault is a filtered, actionable view; too much
   duplication confuses. Recommendation: tab within Ledger (台账 | 恢复站) rather than a
   separate screen; confirm with Arys.
2. **Restore-all with mixed collisions** — bulk defaults to rename; any overwrite requires
   opting in per-item. Confirm this posture in review.
3. **External volumes** — restore must refuse cross-volume (EXDEV) with the documented
   downgrade path [SPEC Part B §3.4]; vault rows must show which volume each quarantine
   lives on.
4. **Countdown clock source** — must be ledger-recorded timestamps (monotonic, not wall
   clock) so a clock jump cannot silently shorten the grace window; tie to WAL sequence
   numbers. Daylight-saving/zone handling is moot (relative elapsed).

## Sources

**Recovery note**: the original research agent died after §1–§3; its per-claim URLs
([S1]–[S15]) were lost with the session. Claims are retained as written; their URLs are
being re-verified in the Verification phase (todo: spot-check citations). Sections §4–§7
above are grounded in the in-repo corpus, cited inline: SPEC Part B §3.4/§5.1–5.2, Part C
§3.7/§4.4/§5.2, ADR-10 (72 h), P1/P4/P5/P6, notes 02 (cleanup landscape; Time Machine triad,
CleanMyMac irreversibility, AppCleaner Put Back), 05 (FS edge cases; EXDEV, dataless), 08
(reflog class; WAL/ledger), 13 (G1 undo-moment video).
Verified-at-write (2026-09-01, direct fetch): Apple Recently Deleted pages and Time Machine
restore docs — canonical support.apple.com pages also listed among note 13 sources; Windows
Recycle Bin conflict behavior per the industry UI survey in note 02 sources. [S-w1] pending
re-fetch.
