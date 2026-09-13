#!/usr/bin/env python3
"""Generate the AI 对话历史原始记录 — the redacted omp session transcripts (作业要求 item 3).

Source of truth: the omp session transcripts for this repo
(`~/.omp/agent/sessions/-Code-disk_janitor/*.jsonl`). One Markdown file is written per
session under `docs/ai-logs/raw/`, plus an `index.md`. Message text is copied verbatim and
timestamps are the record's own ISO-8601 string, unmodified; only the classes of identifier
listed in REDACTIONS below are replaced, and only oversized blocks are truncated (each
truncation is marked inline with how much was dropped).

Usage:
    python3 scripts/gen-ai-transcript.py                 # regenerate docs/ai-logs/raw/
    python3 scripts/gen-ai-transcript.py --json          # print mined figures, write nothing
    python3 scripts/gen-ai-transcript.py --check         # verify the emitted files (exit 1 on leak)
    python3 scripts/gen-ai-transcript.py --sessions DIR  # other corpus (used for dry runs)

Requires: stdlib only.

What is IN the record (and why):
  * human turns          verbatim. The machine-pasted `<attachment>` payloads some turns carry
                         (course pages, pasted harness system prompts — up to 0.5 MB each) are
                         removed, not truncated, so the human's own words on both sides of a
                         paste always survive.
  * assistant prose      verbatim, uncapped (the longest is ~15 kB).
  * assistant thinking   capped at CAP_THINKING — the reasoning trace is part of the AI record.
  * tool calls           one line each: tool name + the model's own declared intent.
                         Arguments are NOT emitted: they carry whole file bodies and personal
                         paths, which is tool noise rather than conversation.
  * tool results         excerpted to CAP_TOOL_RESULT with an explicit `[+N chars]` marker.
  * session metadata     model/thinking-level/mode/title changes, so the narrative is complete.

What is OUT, and stated in each file's header:
  * `custom` / `custom_message` records — `tool_execution_start` duplicates the toolCall, and
    the `ultrathink-notice` / system-notice messages are harness chrome, not conversation.
  * `compaction` summaries — model-written digests of earlier turns, already represented by
    the turns themselves; including them would double-count and they carry the most PII.
  * `model_usage` / `service_tier_change` / `ttsr_injection` — accounting and routing.
  * `image` blocks — emitted as a `[image: <mime>]` placeholder (they are 76-char blob refs,
    not inline data).
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import sys

SESSIONS = "~/.omp/agent/sessions/-Code-disk_janitor"
REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "docs", "ai-logs", "raw")

# Truncation budgets, chosen from the measured corpus (re-derive with --json):
#   thinking blocks   n=2598  median 1166  p90 8728
#   toolResult blocks n=6717  median ~700  max 51362
# Thinking is *conversation* — genuine first-person deliberation with no payload — so its cap
# sits above the median block and keeps ~62% of blocks whole. Tool results are machine I/O and
# the dominant byte class, so they are cut hard. Human turns are not capped in practice: their
# machine-pasted `<attachment>` payloads are stripped whole (see human_text), and CAP_USER only
# catches a malformed span.
CAP_USER = 8000
CAP_DEVELOPER = 4000
CAP_THINKING = 2000
CAP_TOOL_RESULT = 300
CAP_INTENT = 250

# Redaction rules, applied in order to EVERY emitted string: labels first, then vault
# personal trees, then the home path (so the later rules see the original prefixes).
# Everything here is defence-in-depth against new disclosure — the git author identity
# (Arys Aikyn <hi@arysaikyn.com>) is already public in the published history.
REDACTIONS: list[tuple[str, str, str]] = [
    # Email addresses first, so the bare-stem rules below cannot mangle a domain.
    ("email", r"hi@arysaikyn\.com", "<email>"),
    ("email", r"aaikyn1@swarthmore\.edu", "<email>"),
    ("email", r"6183139aersi@gmail\.com", "<email>"),
    ("email", r"naaikyn1@swarthmore\.edu", "<email>"),
    ("email", r"naes25@mails\.tsinghua\.edu\.cn", "<email>"),
    ("email", r"72486278\+arysaikyn@users\.noreply\.github\.com", "<email>"),
    ("email", r"arysaikyn@users\.noreply\.github\.com", "<email>"),
    # Bare local-part stems. The corpus contains *abbreviated* forms of these addresses
    # (`6183139aersi` alone, `naes25@mails...`) written by earlier analysis passes, which the
    # full-address patterns above do not catch — masking the stem closes that hole.
    # `aaikyn1` is the macOS account name; the public author identity is `hi@arysaikyn.com`
    # and `aes25@mails.tsinghua.edu.cn`, neither of which these stems touch.
    ("email", r"6183139aersi", "<email>"),
    ("email", r"naes25", "<email>"),
    # Personal vault trees. Path-anchored on `/vault/…` so product vocabulary is untouched:
    # `vault restore` / `vault browse` / `vault git` are Disk Janitor CLI subcommands, and
    # `journal` alone is overwhelmingly the WAL sense (sqlite journaling, JSONL ledger).
    (
        "vault_tree",
        r"/vault/(?:01-Identity|05-Journal|09-Rhythm|04-Health|06-Goals|People|personal)\b",
        "/vault/<personal>",
    ),
    ("vault_tree", r"/vault/Personal\.md", "/vault/<personal>"),
    ("vault_tree", r"(?<=/)night-think\b", "<personal>"),
    ("home_path", r"aaikyn1", "<user>"),
]

# Patterns that must NOT survive into the emitted files. `--check` enforces this over the
# generated tree; it is the leak gate, so it is deliberately stricter than a spot check.
FORBIDDEN: list[tuple[str, str]] = [
    ("personal identity", r"hi@arysaikyn\.com|aaikyn1|6183139aersi|naes25|naaikyn1@swarthmore"),
    ("personal vault tree", r"/vault/(?:01-Identity|05-Journal|09-Rhythm|04-Health|06-Goals|People|personal)\b"),
    ("private key", r"BEGIN (?:RSA|OPENSSH|EC|PGP) PRIVATE KEY"),
    ("api token", r"\b(?:ghp|gho|ghs|ghu)_[A-Za-z0-9]{20,}|\bsk-[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}\b"),
    ("bearer token", r"(?i)bearer\s+[A-Za-z0-9._\-]{20,}"),
]

FENCE_LINE = re.compile(r"^\s*(`{3,}|~{3,})")
COMPILED_MASK = [(n, re.compile(p), r) for n, p, r in REDACTIONS]

_stats: dict[str, int] = {}


def mask(text: str) -> str:
    """Apply every redaction rule, counting hits per rule for --json reporting."""
    if not text:
        return text
    for name, pat, repl in COMPILED_MASK:
        text, n = pat.subn(repl, text)
        if n:
            _stats[name] = _stats.get(name, 0) + n
    return text


def truncate(text: str, cap: int, label: str) -> str:
    """Cut `text` to `cap` chars, leaving an explicit, machine-greppable dropped-count."""
    if cap <= 0 or len(text) <= cap:
        return text
    return f"{text[:cap]}\n\n… [{label}: {len(text) - cap} of {len(text)} chars omitted]"


def truncate_mid(text: str, head: int, tail: int, label: str) -> str:
    """Keep both ends of `text`, dropping the middle with an explicit count.

    Human turns are a short question, a pasted `<attachment>` payload, then often a further
    instruction after it ("… ulttrathink", "orchestrate"). Truncating from the front alone
    silently discards that trailing instruction, so this keeps the head and the tail.
    """
    if len(text) <= head + tail:
        return text
    omitted = len(text) - head - tail
    return (f"{text[:head]}\n\n… [{label}: {omitted} of {len(text)} chars omitted] …\n\n"
            f"{text[-tail:]}")


def human_text(text: str, cap: int) -> str:
    """Drop machine-pasted `<attachment>` payloads, keeping everything the human typed.

    The human's own words are what this artifact is for — a lead-in question and any trailing
    instruction — while the attachments are pages pasted from the course site or the harness
    system prompt, occasionally half a megabyte of it. Attachments are removed, not truncated,
    so both human ends always survive; `cap` is only a backstop for the malformed case below.
    """
    if "<attachment>" not in text:
        return truncate(text, cap, "turn truncated")
    kept: list[str] = []
    dropped = 0
    if len(re.findall(r"<attachment>", text)) != len(re.findall(r"</attachment>", text)):
        # The harness truncated a half-megabyte paste and lost its closing tag, so pairing
        # cannot be trusted — one re-opened span leaves hundreds of KB of pasted prompt
        # "outside" it. Drop from the first open to the end: the human lead-in always precedes
        # it. Verified against all four such turns in the corpus, none of which carries human
        # text after the paste.
        cut = text.index("<attachment>")
        kept.append(text[:cut])
        dropped = len(text) - cut
    else:
        i = 0
        while True:
            m = re.search(r"<attachment>", text[i:])
            if not m:
                kept.append(text[i:])
                break
            start = i + m.start()
            kept.append(text[i:start])
            close = re.search(r"</attachment>", text[start:])
            if not close:
                dropped += len(text) - start
                break
            dropped += close.end()
            i = start + close.end()
    body = "\n".join(kept).strip()
    if dropped:
        body += f"\n\n… [attachment payload: {dropped} chars omitted]"
    return truncate_mid(body, cap, max(cap // 4, 200), "turn truncated")


def fence_for(text: str) -> str:
    """A backtick fence strictly longer than any run inside `text` — never breaks out."""
    longest = max((len(m.group()) for m in re.finditer(r"`+", text)), default=0)
    return "`" * max(3, longest + 1)


def balance(text: str) -> str:
    """Restore fence parity so an unbalanced ``` in one message cannot swallow the record."""
    marks = [m.group(1) for ln in text.split("\n") if (m := FENCE_LINE.match(ln))]
    if len(marks) % 2:
        return text.rstrip("\n") + f"\n{marks[0]}\n"
    return text


def blocks(message: dict) -> list:
    content = message.get("content")
    return content if isinstance(content, list) else []


def text_of(message: dict) -> str:
    return "\n\n".join(
        b.get("text") or "" for b in blocks(message)
        if isinstance(b, dict) and b.get("type") == "text"
    ).strip()


def emit_block(out: list[str], message: dict, head: str, cap: int, label: str,
               prep=None) -> None:
    """Emit one text-bearing message. `prep` shapes the raw text before the cap applies."""
    raw = text_of(message)
    body = prep(raw) if prep else truncate(raw, cap, label)
    if body:
        out.append(f"### {head}\n")
        out.append(balance(body))
        out.append("")


def render(path: str) -> tuple[str, dict]:
    """Render one session transcript. Returns (markdown, figures)."""
    lines = [ln for ln in open(path, errors="replace").read().split("\n") if ln.strip()]
    out: list[str] = []
    title = os.path.basename(path).rsplit(".", 1)[0]
    sid = cwd = ""
    stamps: list[str] = []
    counts = dict(records=0, user=0, assistant=0, developer=0, tool_call=0, tool_result=0,
                  thinking=0, image=0, compaction=0, omitted=0)
    body: list[str] = []       # transcript, buffered so the header can go out first

    for line in lines:
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        counts["records"] += 1
        kind = rec.get("type")
        ts = rec.get("timestamp") or rec.get("updatedAt")
        if isinstance(ts, str):
            stamps.append(ts)
        head = f"[{ts}]" if isinstance(ts, str) else "[no timestamp]"

        if kind == "title":
            title = rec.get("title") or title
            continue
        if kind == "session":
            sid = rec.get("id") or ""
            cwd = mask(rec.get("cwd") or "")
            continue
        if kind in ("title_change", "model_change", "thinking_level_change", "mode_change"):
            what = (rec.get("title") or rec.get("model") or rec.get("thinkingLevel")
                    or rec.get("mode") or "")
            body.append(f"- {head} *{kind.replace('_', ' ')}*: `{what}`")
            continue
        if kind == "compaction":
            counts["compaction"] += 1
            body.append(f"- {head} *compaction*: {rec.get('tokensBefore')} → "
                        f"{rec.get('tokensAfter')} tokens (summary omitted)")
            continue
        if kind != "message":
            counts["omitted"] += 1
            continue

        msg = rec.get("message")
        if not isinstance(msg, dict):
            counts["omitted"] += 1
            continue
        role = msg.get("role") or "?"

        if role == "user":
            counts["user"] += 1
            emit_block(body, msg, f"{head} **user**", CAP_USER, "turn truncated",
                       prep=lambda t: human_text(t, CAP_USER))
            continue
        if role == "developer":
            counts["developer"] += 1
            emit_block(body, msg, f"{head} **developer** (harness-injected instruction)",
                       CAP_DEVELOPER, "instruction truncated")
            continue
        if role == "fileMention":
            names = ", ".join((f.get("path") or "?") for f in (msg.get("files") or []))
            body.append(f"- {head} *file mention*: `{names}` "
                        f"(injected context, {sum(len(f.get('content') or '') for f in msg.get('files') or [])} chars)"
                        "\n")
            continue
        if role == "toolResult":
            counts["tool_result"] += 1
            name = msg.get("toolName") or "?"
            err = " · error" if msg.get("isError") else ""
            pieces = []
            for b in blocks(msg):
                if not isinstance(b, dict):
                    continue
                if b.get("type") == "text":
                    pieces.append(b.get("text") or "")
                elif b.get("type") == "image":
                    counts["image"] += 1
                    pieces.append(f"[image: {b.get('mimeType')}]")
            body.append(f"### {head} ← tool result `{name}`{err}\n")
            excerpt = truncate("\n".join(pieces).strip(), CAP_TOOL_RESULT, "truncated")
            fence = fence_for(excerpt)
            body.append(f"{fence}\n{excerpt}\n{fence}\n")
            continue
        if role != "assistant":
            counts["omitted"] += 1
            continue

        counts["assistant"] += 1
        for b in blocks(msg):
            if not isinstance(b, dict):
                continue
            kind_b = b.get("type")
            if kind_b == "text":
                text = (b.get("text") or "").strip()
                if text:
                    body.append(f"### {head} **assistant**\n")
                    body.append(balance(text))
                    body.append("")
            elif kind_b == "thinking":
                counts["thinking"] += 1
                thought = (b.get("thinking") or "").strip()
                if thought:
                    body.append(f"### {head} **assistant · reasoning**\n")
                    body.append(balance(truncate(thought, CAP_THINKING, "reasoning truncated")))
                    body.append("")
            elif kind_b == "toolCall":
                counts["tool_call"] += 1
                intent = truncate((b.get("intent") or "").strip(), CAP_INTENT, "intent truncated")
                intent = intent.replace("\n", " ")
                body.append(f"- {head} **tool call** `{b.get('name')}`"
                            + (f" — {intent}" if intent else ""))
            elif kind_b == "image":
                counts["image"] += 1
                body.append(f"- {head} *image*: [{b.get('mimeType')}]")
        body.append("")

    first, last = (stamps[0], stamps[-1]) if stamps else ("", "")
    header = [
        f"# AI 对话历史原始记录 — {title}",
        "",
        "> Generated by `scripts/gen-ai-transcript.py` from the omp session transcript "
        "for this repo. **Do not hand-edit** — re-run the script instead.",
        ">",
        f"> **Original text.** Human turns and assistant prose are verbatim — the machine-pasted "
        f"`<attachment>` payloads some turns carry are removed, not truncated, so the human's "
        f"own words survive. Timestamps are the record's own ISO-8601 string. Reasoning is capped "
        f"at {CAP_THINKING} chars and tool output at {CAP_TOOL_RESULT}, each cut marked inline. "
        "Tool call *arguments* are omitted (they carry file bodies, not conversation).",
        ">",
        "> **Redacted.** Personal emails → `<email>`; personal vault trees → `<personal>`; the "
        "home directory path → `/Users/<user>`. Nothing else is altered. The raw transcripts "
        "stay in harness session history; this file is a projection of them.",
        ">",
        f"> **Omitted record types.** `custom`/`custom_message` (duplicate tool-start events and "
        f"system notices), `compaction` summaries ({counts['compaction']}), `model_usage`, "
        f"`service_tier_change`, `ttsr_injection`.",
        "",
        "| field | value |",
        "|---|---|",
        f"| session id | `{sid}` |",
        f"| title | {title} |",
        f"| cwd | `{cwd}` |",
        f"| first timestamp | `{first}` |",
        f"| last timestamp | `{last}` |",
        f"| records | {counts['records']} |",
        f"| human turns | {counts['user']} |",
        f"| assistant messages | {counts['assistant']} |",
        f"| reasoning blocks | {counts['thinking']} |",
        f"| tool calls | {counts['tool_call']} |",
        f"| tool results | {counts['tool_result']} |",
        f"| images | {counts['image']} |",
        "",
        "---",
        "",
    ]
    figures = dict(id=sid, title=title, cwd=cwd, first=first, last=last, **counts)
    # Masking is applied to the FINISHED document rather than to individual fields, so no
    # emission path can bypass it — every byte that lands in the artifact passes the rules.
    return mask("\n".join(header + body).rstrip("\n") + "\n"), figures


def corpus(root: str) -> list[str]:
    root = os.path.expanduser(root)
    if not os.path.isdir(root):
        sys.exit(f"session directory not found: {root}")
    return sorted(glob.glob(os.path.join(root, "*.jsonl")))


def cmd_generate(root: str, out_dir: str) -> list[dict]:
    os.makedirs(out_dir, exist_ok=True)
    rows: list[dict] = []
    for path in corpus(root):
        stem = os.path.basename(path).rsplit(".", 1)[0]
        md, figures = render(path)
        target = os.path.join(out_dir, stem + ".md")
        with open(target, "w", encoding="utf-8") as fh:
            fh.write(md)
        rows.append(dict(figures, file=stem + ".md", bytes=len(md.encode())))
        print(f"  {stem}.md  {len(md.encode()) / 1e6:5.2f} MB  "
              f"{figures['user']:4d} human · {figures['assistant']:5d} assistant · "
              f"{figures['tool_call']:5d} calls")
    if rows:
        index = [
            "# Raw AI transcripts — index",
            "",
            "Generated by `scripts/gen-ai-transcript.py`; the readable per-milestone summaries "
            "are [`M1.md`](../M1.md) … [`M5.md`](../M5.md). Regenerate with "
            "`python3 scripts/gen-ai-transcript.py`.",
            "",
            "| session | title | first timestamp | human | assistant | calls | size |",
            "|---|---|---|---|---|---|---|",
        ]
        for r in rows:
            index.append(f"| [`{r['file']}`]({r['file']}) | {r['title']} | `{r['first']}` | "
                         f"{r['user']} | {r['assistant']} | {r['tool_call']} | "
                         f"{r['bytes'] / 1e6:.2f} MB |")
        total = sum(r["bytes"] for r in rows)
        index += ["", f"**{len(rows)} sessions · {total / 1e6:.2f} MB total.**", ""]
        with open(os.path.join(out_dir, "index.md"), "w", encoding="utf-8") as fh:
            fh.write(mask("\n".join(index)))
    return rows


def cmd_check(out_dir: str) -> int:
    paths = sorted(glob.glob(os.path.join(out_dir, "*.md")))
    if not paths:
        sys.exit(f"nothing to check in {out_dir}")
    bad = 0
    for name, pat in FORBIDDEN:
        rx = re.compile(pat)
        for path in paths:
            hits = rx.findall(open(path, encoding="utf-8").read())
            if hits:
                bad += 1
                print(f"  LEAK {os.path.basename(path)}: {name} x{len(hits)} — {hits[:3]}")
    print(f"checked {len(paths)} files · " + ("CLEAN" if not bad else f"{bad} LEAKS"))
    return 1 if bad else 0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--sessions", default=SESSIONS)
    ap.add_argument("--out", default=OUT)
    ap.add_argument("--json", action="store_true", help="print mined figures, write nothing")
    ap.add_argument("--check", action="store_true", help="verify emitted files, write nothing")
    args = ap.parse_args()

    if args.check:
        sys.exit(cmd_check(args.out))

    if args.json:
        rows = []
        redactions: dict[str, int] = {}
        for path in corpus(args.sessions):
            _stats.clear()
            md, figures = render(path)
            rows.append(dict(figures, file=os.path.basename(path), bytes=len(md.encode())))
            for k, v in _stats.items():
                redactions[k] = redactions.get(k, 0) + v
        print(json.dumps({
            "sessions": len(rows),
            "total_bytes": sum(r["bytes"] for r in rows),
            "max_bytes": max((r["bytes"] for r in rows), default=0),
            "figures": {r["file"]: {k: r[k] for k in
                                    ("id", "title", "first", "last", "user", "assistant",
                                     "thinking", "tool_call", "tool_result", "bytes")}
                        for r in rows},
            "redactions": redactions,
        }, indent=1))
        return

    rows = cmd_generate(args.sessions, args.out)
    total = sum(r["bytes"] for r in rows)
    print(f"wrote {len(rows)} sessions to {args.out} · {total / 1e6:.2f} MB")


if __name__ == "__main__":
    main()
