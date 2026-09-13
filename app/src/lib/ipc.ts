// Typed IPC wrapper (SPEC Part C §2.3): named Rust commands + typed channels.
// When running inside Tauri (window.__TAURI_INTERNALS__ present) calls are
// forwarded to the host; otherwise demo fallback data is returned so the UI
// renders in a plain browser (`bun run dev`, demo video capture).
//
// DTO shapes mirror src-tauri/src/commands.rs + src/{plan,ledger}.rs serde
// shapes (snake_case fields, serde tag "t" on enums).

import { invoke as tauriInvoke, Channel as TauriChannel } from "@tauri-apps/api/core";

// ── DTOs (serde shapes from src-tauri/src/commands.rs) ──────────────────────

export interface VolumeDto {
  mount: string;
  fs: string;
  total: number;
  /** Raw `statfs` free space — purgeable excluded (SPEC §3.1). */
  free_raw: number;
  /** Free + purgeable: the Finder "available" figure (SPEC §3.1). */
  free_important: number;
  read_only: boolean;
  writable: boolean;
  /** demo-only: snapshot-pinned space (SPEC §3.2); not in the Rust DTO */
  snapshot_held?: number;
}

export interface EvidenceDto {
  kind: string;
  detail: string;
  strength: string;
}

/** ScanProgress::Candidate payload (channel) + disk_janitor::Candidate (manifest). */
export interface CandidateDto {
  id: string;
  path?: string;
  path_hash?: string;
  /** channel payload field name (ScanProgress::Candidate) */
  bytes?: number;
  /** manifest field name (disk_janitor::Candidate.alloc_bytes) */
  alloc_bytes?: number;
  grade: string;
  evidence: EvidenceDto[];
}

/** The persisted last-scan envelope (commands.rs get_last_scan). */
export interface ScanManifestDto {
  roots: string[];
  min_bytes: number;
  candidates: CandidateDto[];
}

export type PlanActionDto =
  | { type: "quarantine" }
  | { type: "prune_command"; cmd: string; dry_run: string }
  | { type: "refuse"; reason: string };

export interface PlanEntryDto {
  cid: string;
  path: string;
  path_hash: string;
  action: PlanActionDto;
  grade?: string;
  alloc_bytes?: number;
}

export interface PlanTotalsDto {
  quarantine_bytes: number;
  quarantine_entries: number;
  prune_bytes: number;
  prune_cmds: number;
  refused: number;
  snapshot_caveat?: string | null;
}

export interface PlanDto {
  plan_id: string;
  created_at: string;
  entries: PlanEntryDto[];
  totals: PlanTotalsDto;
  policy_snapshot?: unknown;
}

/** LedgerRecord payloads (src/ledger.rs, serde tag "t"). */
export type LedgerRecDto =
  | {
      t: "scan";
      root: string;
      files_scanned: number;
      total_bytes: number;
      duration_ms: number;
      session_id: string;
    }
  | { t: "verdict"; path_hash: string; size: number; grade: string; evidence: EvidenceDto[] }
  | { t: "plan"; plan_id: string; entries: PlanEntryDto[]; policy_snapshot: unknown }
  | {
      t: "apply";
      plan_id: string;
      entry: PlanEntryDto;
      original_path: string;
      quarantine_path: string;
      renamex_np: boolean;
      undo: { original_path: string; byte_count: number; grace_expires_at: string };
    }
  | {
      t: "purge";
      plan_id: string;
      entry: PlanEntryDto;
      bytes_freed: number;
      quarantine_path: string;
    }
  | { t: "restore"; plan_id: string; entry: PlanEntryDto; from: string; to: string; undoes: number }
  | {
      t: "budget";
      model: string;
      calls: number;
      prompt_tokens: number;
      completion_tokens: number;
      cost_usd: number;
    }
  | { t: "session"; run_id: string; byte_offset: number };

/** Envelope get_ledger_tail returns (src/ledger.rs Record). */
export interface LedgerEnvelopeDto {
  v: number;
  seq: number;
  ts: string;
  kind: string;
  crc32: number;
  prev_crc: number | null;
  rec: LedgerRecDto;
}

// ── Typed channel payloads (serde tag "t", snake_case) ──────────────────────

export type ScanProgressMsg =
  | { t: "files"; files: number; bytes: number; rate_bps: number; current: string; total?: number }
  | { t: "candidate"; id: string; bytes: number; grade: string; evidence: EvidenceDto[] }
  | { t: "root_error"; root: string; err: string }
  | { t: "finished"; files: number; bytes: number; elapsed_ms: number };

export type ExecProgressMsg =
  | { t: "applied"; cid: string; to: string }
  | { t: "purged"; cid: string; bytes: number }
  | { t: "restored"; cid: string; to: string }
  | { t: "error"; cid: string; err: string };

// ── R4 agent wire events (commands.rs AgentProgressMsg / AgentSummary) ───────

/** One streamed event of a live agent run — 1:1 with commands.rs AgentProgressMsg. */
export interface AgentProgressMsg {
  turn: number;
  /** `"model_call"` | `"tool_exec"` | `"done"` (snake_case, as serialized). */
  phase: "model_call" | "tool_exec" | "done";
  /** Tool name for tool_exec events, else null. */
  tool?: string | null;
  detail: string;
  /** Running totals after this event. */
  budget: { tokens: number; cost_usd: number };
}

/** Completion payload of run_agent (commands.rs AgentSummary). */
export interface AgentSummary {
  /** snake_case StopReason (e.g. `"model_stop"`, `"user_cancel"`). */
  stop_reason: string;
  turns: number;
  tokens: number;
  cost_usd: number;
  /** The plan id the run settled on, if any. */
  plan_id?: string | null;
  /** The recorded verdict, else the final assistant text, else none. */
  proposal?: string | null;
}

// ── invoke with demo fallback ────────────────────────────────────────────────

const hasTauri = () =>
  typeof window !== "undefined" && typeof (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined";

// R4 demo: cancel_agent flips this; the scripted run_agent trajectory then
// resolves early with stop_reason "user_cancel" (mirrors the atomic in AgentState).
let demoAgentCancel = false;

/** The default policy the real `build_plan` snapshots into the manifest
 * (`PolicyConstraintsShape::default()`, src/plan.rs). Hand-mirrored because the
 * browser path has no Rust runtime; keep in step with `DEFAULT_DENY_GLOBS` and
 * the `Default` impl — `PlanReview`'s 查看 JSON renders this verbatim, so a drift
 * makes the demo lie about the policy behind its entries. */
const DEMO_POLICY_SNAPSHOT = {
  target_bytes: null,
  min_bytes: 52_428_800,
  allow_grades: ["pure_cache", "rebuildable_from_source", "redownloadable"],
  deny_globs: [
    "*/.git/*",
    "*/node_modules/*",
    "*/lost+found/*",
    "*/Trash/*",
    "*/.Trash-*/*",
    "/Users/*/Library/Caches/*",
  ],
  deny_under_mta_secs: 0,
  allow_network_regen: true,
  allow_prune_commands: true,
  grace_hours: 72,
  max_entries: 10_000,
};

const DEMO_CANDIDATES: CandidateDto[] = [
  {
    id: "c1",
    path: "~/code/api/node_modules",
    path_hash: "h1",
    alloc_bytes: 2_100_000_000,
    grade: "rebuildable_from_source",
    evidence: [
      { kind: "LockfilePresent", detail: "package-lock.json newer than dir", strength: "Strong" },
      { kind: "GitRepoAncestor", detail: "HEAD 3 h ago", strength: "Strong" },
      { kind: "Unreadable", detail: "", strength: "Weak" },
    ],
  },
  {
    id: "c2",
    path: "~/Library/Developer/Xcode/DerivedData",
    path_hash: "h2",
    alloc_bytes: 6_800_000_000,
    grade: "rebuildable_from_source",
    evidence: [
      { kind: "LockfilePresent", detail: "xcuserdata lockfile present", strength: "Strong" },
      { kind: "BuildArtifactShape", detail: "9 d 2 h since build", strength: "Medium" },
    ],
  },
  {
    id: "c3",
    path: "~/.npm/_cacache",
    path_hash: "h3",
    alloc_bytes: 3_200_000_000,
    grade: "pure_cache",
    evidence: [{ kind: "CasStore", detail: "content-addressed store, self-healing", strength: "Strong" }],
  },
  {
    id: "c4",
    path: "~/Downloads/old-installers",
    path_hash: "h4",
    alloc_bytes: 1_500_000_000,
    grade: "redownloadable",
    evidence: [{ kind: "SiblingManifestMissing", detail: "vendor installers, dmg/pkg", strength: "Medium" }],
  },
  {
    id: "c5",
    path: "~/code/legacy/vendor",
    path_hash: "h5",
    alloc_bytes: 4_900_000_000,
    grade: "expensive",
    evidence: [{ kind: "OrphanTree", detail: "no manifest in ancestors", strength: "Medium" }],
  },
  {
    id: "c6",
    path: "~/code/api/node_modules/.cache",
    path_hash: "h6",
    alloc_bytes: 400_000_000,
    grade: "hands_off",
    evidence: [{ kind: "MtimeHot", detail: "written 4 min ago", strength: "Strong" }],
  },
];

function demo<T>(cmd: string, args?: Record<string, unknown>): T {
  switch (cmd) {
    case "get_volumes":
      return [
        {
          mount: "/",
          fs: "APFS",
          total: 1_000_000_000_000,
          free_raw: 387_600_000_000,
          free_important: 401_200_000_000,
          read_only: false,
          writable: true,
          snapshot_held: 13_600_000_000,
        },
      ] as T;
    case "get_last_scan":
      return { roots: ["/Users/me"], min_bytes: 52_428_800, candidates: DEMO_CANDIDATES } as T;
    case "get_ledger_tail": {
      const mk = (seq, minsAgo, rec) => ({
        v: 1, seq, crc32: 1000 + seq, prev_crc: null,
        ts: new Date(Date.now() - minsAgo * 60_000).toISOString(),
        kind: rec.t, rec,
      });
      const e1 = { cid: "c1", path: "~/code/api/node_modules", path_hash: "h1", action: { type: "quarantine" }, grade: "rebuildable_from_source", alloc_bytes: 2_100_000_000 };
      const e2 = { cid: "c2", path: "~/Library/Developer/Xcode/DerivedData", path_hash: "h2", action: { type: "quarantine" }, grade: "rebuildable_from_source", alloc_bytes: 6_800_000_000 };
      const e3 = { cid: "c3", path: "~/.npm/_cacache", path_hash: "h3", action: { type: "quarantine" }, grade: "pure_cache", alloc_bytes: 3_200_000_000 };
      return [
        mk(3, 64, { t: "apply", plan_id: "demo-20260909", entry: e1, original_path: e1.path, quarantine_path: "/.Janitor/quarantine/c1", renamex_np: true, undo: { original_path: e1.path, byte_count: e1.alloc_bytes, grace_expires_at: new Date(Date.now() + 71.5 * 3600_000).toISOString() } }),
        mk(4, 63, { t: "apply", plan_id: "demo-20260909", entry: e2, original_path: e2.path, quarantine_path: "/.Janitor/quarantine/c2", renamex_np: true, undo: { original_path: e2.path, byte_count: e2.alloc_bytes, grace_expires_at: new Date(Date.now() + 71.6 * 3600_000).toISOString() } }),
        mk(5, 62, { t: "apply", plan_id: "demo-20260909", entry: e3, original_path: e3.path, quarantine_path: "/.Janitor/quarantine/c3", renamex_np: true, undo: { original_path: e3.path, byte_count: e3.alloc_bytes, grace_expires_at: new Date(Date.now() + 71.7 * 3600_000).toISOString() } }),
        mk(6, 41, { t: "purge", plan_id: "demo-20260909", entry: e3, bytes_freed: e3.alloc_bytes, quarantine_path: "/.Janitor/quarantine/c3" }),
        mk(7, 22, { t: "restore", plan_id: "demo-20260909", entry: e2, from: "/.Janitor/quarantine/c2", to: e2.path, undoes: 4 }),
      ] as T;
    }
    // R5 trajectory: demo mode has no saved sessions (mirrors commands.rs demo).
    case "get_agent_sessions":
      return [] as T;
    case "get_agent_trajectory":
      return null as T;
    case "start_scan": {
      // Stream Files ticks + finished over the channel so the ring animates.
      const ch = args?.channel as { onmessage?: (m: ScanProgressMsg) => void } | undefined;
      if (ch?.onmessage) {
        const TOTAL_FILES = 1_204_553;
        const TOTAL_BYTES = 900_000_000_000;
        const ticks = [
          [0.12, "~/code/api"],
          [0.31, "~/Library/Developer"],
          [0.55, "~/Library/Caches"],
          [0.74, "~/.npm"],
          [0.93, "~/Downloads"],
        ] as const;
        ticks.forEach(([frac, path], i) => {
          setTimeout(() => {
            ch.onmessage?.({
              t: "files",
              files: Math.round(TOTAL_FILES * frac),
              bytes: Math.round(TOTAL_BYTES * frac),
              rate_bps: 1_300_000_000,
              current: path,
              total: TOTAL_FILES,
            });
            if (i === ticks.length - 1) {
              setTimeout(() => {
                ch.onmessage?.({ t: "finished", files: TOTAL_FILES, bytes: TOTAL_BYTES, elapsed_ms: 42_000 });
              }, 240);
            }
          }, i * 320);
        });
      }
      return "/Users/me/.disk-janitor/last-scan.json" as T;
    }
    case "cancel_scan":
      return null as T;
    case "run_agent": {
      // R4: scripted trajectory so the orb pulses — one event per 350ms.
      // Tokens 50–160/call; cost at the config defaults' deepseek-chat prices
      // ($0.27/M in, $1.10/M out — src/config.rs). cancel_agent (demo) flips
      // demoAgentCancel and the run resolves early with user_cancel.
      demoAgentCancel = false;
      const ch = args?.channel as { onmessage?: (m: AgentProgressMsg) => void } | undefined;
      const seq: AgentProgressMsg[] = [
        { turn: 1, phase: "model_call", tool: null, detail: "读取扫描清单，识别候选…", budget: { tokens: 60, cost_usd: 0.00005 } },
        { turn: 1, phase: "tool_exec", tool: "scan_summary", detail: "19 个候选 · 21.8 GB", budget: { tokens: 60, cost_usd: 0.00005 } },
        { turn: 2, phase: "model_call", tool: null, detail: "按策略筛选可安全回收项…", budget: { tokens: 150, cost_usd: 0.00015 } },
        { turn: 2, phase: "tool_exec", tool: "build_plan", detail: "13 隔离 · 2 清理 · 4 拒绝", budget: { tokens: 150, cost_usd: 0.00015 } },
        { turn: 3, phase: "tool_exec", tool: "record_verdict", detail: "已记录判定，写入计划", budget: { tokens: 210, cost_usd: 0.00025 } },
        { turn: 3, phase: "done", tool: null, detail: "完成 — 已生成计划", budget: { tokens: 210, cost_usd: 0.00025 } },
      ];
      const done = seq[seq.length - 1];
      return new Promise<AgentSummary>((resolve) => {
        let i = 0;
        const tick = () => {
          if (demoAgentCancel) {
            ch?.onmessage?.({ ...done, detail: "已取消" });
            resolve({
              stop_reason: "user_cancel",
              turns: done.turn,
              tokens: done.budget.tokens,
              cost_usd: done.budget.cost_usd,
              plan_id: null,
              proposal: null,
            });
            return;
          }
          if (i < seq.length) {
            ch?.onmessage?.(seq[i]);
            i += 1;
            setTimeout(tick, 350);
          } else {
            resolve({
              stop_reason: "requested_by_tool",
              turns: done.turn,
              tokens: done.budget.tokens,
              cost_usd: done.budget.cost_usd,
              plan_id: "demo-plan",
              proposal: "建议清理 19.3 GB 纯缓存与可重建目录",
            });
          }
        };
        tick();
      }) as T;
    }
    case "cancel_agent":
      demoAgentCancel = true;
      return null as T;
    case "build_plan":
      return {
        plan_id: "demo-20260909",
        created_at: new Date().toISOString(),
        entries: DEMO_CANDIDATES.filter((c) => c.grade !== "hands_off").map((c) => ({
          cid: c.id,
          path: c.path,
          path_hash: c.path_hash,
          action: { type: "quarantine" },
          grade: c.grade,
          alloc_bytes: c.alloc_bytes,
        })),
        totals: {
          quarantine_bytes: 19_300_000_000,
          quarantine_entries: 11,
          prune_bytes: 1_600_000_000,
          prune_cmds: 2,
          refused: 3,
          snapshot_caveat: null,
        },
        policy_snapshot: DEMO_POLICY_SNAPSHOT,
      } as T;
    case "apply_plan":
      // Same shape as commands.rs drain_exec + apply_plan report.
      return { op_id: "op-demo", applied: 11, skipped: 0, refused: 3, quarantined_bytes: 19_300_000_000 } as T;
    case "purge_plan": {
      // Small delay so the 销毁中… progress state is visible in demo captures.
      return new Promise((resolve) => {
        setTimeout(() => resolve({ ok: true, bytes_freed: Number(args?.typedBytes ?? 0) }), 700);
      }) as T;
    }
    case "restore_op":
      return { ok: true, restored: 1 } as T;
    case "recover_on_launch":
      return { resumed: null, dangling: 0, expired: 0 } as T;
    default:
      return {} as T;
  }
}

// ── D-38: backend demo-mode probe (--demo-data launch) ─────────────────────────
//
// Inside Tauri the host is asked once; when it reports demo mode, every command
// resolves the same local fallbacks as the plain-browser path (identical,
// deterministic payloads for reproducible screen recordings, P7).

let demoMode = false;
let demoCheck: Promise<boolean> | null = null;

function isDemoMode(): Promise<boolean> {
  if (!hasTauri()) return Promise.resolve(true);
  if (!demoCheck) {
    demoCheck = tauriInvoke<boolean>("is_demo")
      .then((v) => {
        demoMode = v;
        return v;
      })
      .catch(() => false); // probe failure degrades to real mode, never fake data
  }
  return demoCheck;
}

/** Typed command call. In plain-browser dev or a --demo-data Tauri launch this
 * resolves demo data instead. */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!hasTauri() || (demoMode || (await isDemoMode()))) return demo<T>(cmd, args);
  return tauriInvoke<T>(cmd, args);
}

// ── typed Channel ────────────────────────────────────────────────────────────

/**
 * Typed progress channel. In Tauri: a real `Channel<T>` whose onmessage feeds
 * `onMessage`. In plain-browser dev: returns a `{ onmessage }` shell and calls
 * `onMessage` once with a synthetic finished event so channel-driven UI still
 * settles into its done state (start_scan demo streams extra ticks itself).
 */
export function channel<T>(onMessage?: (m: T) => void): unknown {
  if (!hasTauri() || demoMode) {
    const shell = { onmessage: onMessage };
    if (onMessage) {
      queueMicrotask(() => {
        onMessage({ t: "finished", files: 0, bytes: 0, elapsed_ms: 0 } as T);
      });
    }
    return shell;
  }
  const ch = new TauriChannel<T>();
  if (onMessage) ch.onmessage = onMessage;
  return ch;
}
