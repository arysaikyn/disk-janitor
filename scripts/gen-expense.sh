#!/usr/bin/env bash
# gen-expense.sh — regenerate docs/expense-sheet.md (D-20 开销明细表, M5).
#
# Usage:
#   ./scripts/gen-expense.sh [LEDGER_PATH] > docs/expense-sheet.md
#   DISK_JANITOR_HOME=<home> ./scripts/gen-expense.sh > docs/expense-sheet.md
#   ./scripts/gen-expense.sh --demo        # 演示数据 fixture (tempdir ledger)
#
# Runs the `expense-report` example (cargo run --example), which rolls up
# Kind::Budget records from the given ledger via disk_janitor::cost and prints
# the stable D-20 table (日期/模型/调用/tokens/USD/CNY + TOTAL) inside a fenced
# code block. No ledger arg → default home ledger (DISK_JANITOR_HOME or
# ~/.disk-janitor/ledger.jsonl); an absent ledger prints the empty table.
#
# --demo seeds a throwaway tempdir ledger with demo Budget records — frozen v1
# envelopes, CRC-valid (crc32fast over canonical serde_json rec bytes ++ seq LE,
# see src/ledger.rs crc_of) — so the sheet's 演示数据 table regenerates on demand.
# The fixture is NOT real API spend.
set -euo pipefail
cd "$(dirname "$0")/.."

LEDGER="${1:-}"

if [[ "${LEDGER}" == "--demo" ]]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    LEDGER="$TMP/ledger.jsonl"
    python3 - "$LEDGER" <<'PY'
import json, struct, sys, zlib

# Frozen v1 envelope (src/ledger.rs): {"v":1,"seq":N,"ts":…,"kind":"budget",
# "crc32":C,"prev_crc":null,"rec":{…}} with C = crc32fast(rec bytes ++ seq LE).
# rec serializes exactly as serde_json emits the internally-tagged
# LedgerRecord::Budget: tag "t" first, then fields in declaration order.
def budget(seq, ts, model, calls, prompt_tokens, completion_tokens, cost_usd):
    rec = {
        "t": "budget",
        "model": model,
        "calls": calls,
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "cost_usd": cost_usd,
    }
    rec_bytes = json.dumps(rec, separators=(",", ":")).encode()
    crc32 = zlib.crc32(rec_bytes + struct.pack("<Q", seq)) & 0xFFFFFFFF
    env = {
        "v": 1, "seq": seq, "ts": ts, "kind": "budget",
        "crc32": crc32, "prev_crc": None, "rec": rec,
    }
    return json.dumps(env, separators=(",", ":")).encode() + b"\n"

lines = [
    budget(1, "2026-09-08T12:00:00Z", "deepseek-chat", 2, 1000, 300, 0.012),
    budget(2, "2026-09-08T14:30:00Z", "deepseek-chat", 1, 500, 150, 0.004),
    budget(3, "2026-09-09T09:00:00Z", "gpt-4o-mini", 3, 1200, 400, 0.003456),
]
with open(sys.argv[1], "wb") as f:
    f.writelines(lines)
PY
fi

exec cargo run --quiet --example expense-report -- "$LEDGER"
