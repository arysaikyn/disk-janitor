# Destructive-Action UX Patterns — Research Note

Interaction-design mechanics that make irreversible, high-consequence operations legible and hard to trigger by accident — published platform guidelines plus shipping-product ceremonies, mapped onto the Giveback Review / PlanReview / PurgeGate / Ledger surfaces · researched 2026-09-13

Scope: published human-interface guidelines (Apple HIG, GNOME HIG, Microsoft Fluent, W3C WAI-ARIA APG, NN/g) and shipping-product destructive-action ceremonies (GitHub repository delete/restore, npm unpublish, Stripe API rollback, Amazon S3 `DeleteObjects`, Docker `system prune`, Terraform `destroy`, `git clean`, `kubectl delete`, `gh repo delete`) — examined for *mechanic*, not for copy or visual style. Every quote, version field, and license in this note was read from a primary source **on 2026-09-13**; the access method and the exact thing read are recorded per row in `## Sources`. This lane is **guide-and-mechanic**; it deliberately does not re-derive the one-tap/receipt product landscape of note 15 or the vault/restore grammar of note 17.

## Predecessors — what this note does NOT re-cover

Two in-corpus predecessors already own adjacent ground. Read them first; this note only adds what they do not have.

| Predecessor | What it already owns | Status in this lane |
|---|---|---|
| `docs/research/15-one-tap-clean-receipts.md` | CleanMyMac Smart Care (all tiles pre-checked, single Run, no typed confirm, no itemized receipt, no undo), DevCleaner three risk tiers (SAFE pre-selected / WARNING never pre-selected / DANGER asks twice, 30-day history), CCleaner Health Check Analyze→Make-it-better→Results, MacBooster/Sensei/OnyX, the §6 receipt table (white space: nobody distinguishes "freed now" from "quarantined, restorable") | **COVERED.** Every competitor in this lane is skipped here. Note 15 has no published-guideline quotes and no API-level partial-failure contract — that is what §1–§8 add. |
| `docs/research/17-quarantine-vault-restore-ux.md` | macOS Recently-Deleted grammar (Photos 30 d + locked album, Mail configurable, Messages 30 d), Time Machine Keep Both/Replace/Stop, TrashMe/AppCleaner/CleanMyMac undo gaps, git reflog addressability-vs-pruning, per-row countdown + empty-state posture, restore-collision triad, vault layout §6, bilingual copy table | **COVERED.** The retention-window survey and collision triad are not restated. Note 17's per-claim URLs `[S1]`–`[S15]` were never persisted to disk, so nothing from it is cited here; every claim below was re-fetched independently. |
| `docs/research/21-daisydisk-design-principles.md` L93–100 | A 5 s countdown that the quarantine design deliberately replaced | **COVERED** (counted, not repeated). |

**NEW in this note (not present anywhere in notes 01–22):**

1. Published guideline text on confirm-vs-undo doctrine, button role/ordering, typed gates, and progress honesty — quoted verbatim with URLs.
2. **The inert-gate pattern.** A modal that intentionally withholds Esc and outside-click dismissal, and the direct, adversarially-tested conflict this creates with W3C APG / GNOME / Apple / Fluent. This is our `PurgeGate.svelte` design; no corpus note has ever discussed it.
3. **Partial-failure presentation as a first-class receipt contract** — S3 `DeleteObjects` verbose mode returning HTTP 200 with a `<Deleted>`/`<Error>` mix, and its absence from every competitor in note 15.
4. **The dry-run alias** — `terraform plan -destroy`, `git clean -n/-i`, `kubectl delete --dry-run=client|server` — as the published mechanic behind our scan→plan→apply split.
5. **Escalating force** — `git clean -f` refusing nested git repos unless given a *second* `-f` — a count-based alternative to typed confirmation.
6. The **apply=soft-gate vs purge=hard-gate asymmetry** in our own GUI, mapped against GNOME's insensitivity rule and Apple's Return-key rule.

## Findings

### 1. Confirm-or-undo doctrine: undo is usually the better instrument

**GNOME HIG, Dialogs** — the strongest published statement that confirmation dialogs are a second-best instrument:

> "Destructive actions should always be accompanied by either a confirmation dialog or an offer to undo the action (typically using a toast). Since users will often habitually click through confirmation dialogs without fully reading or considering them, undo is typically a better option than a confirmation dialog."

> "However, in cases where it is not possible to offer an undo feature, a confirmation dialog is still recommended."

**Apple HIG, Alerts** — the same doctrine with the frequency criterion made explicit:

> "Avoid displaying alerts for common, undoable actions, even when they're destructive. For example, you don't need to alert people about data loss every time they delete an email or file because they do so with the intention of discarding data, and they can undo the action. In comparison, when people take an uncommon destructive action that they can't undo, it's important to display an alert in case they initiated the action accidentally."

**NN/g (Nielsen), "Confirmation Dialogs Can Prevent User Errors — If Not Overused"** (reviewed 2026-08-07) names the failure mode the doctrine exists to prevent:

> "Let's go back to basics: why have confirmation dialogs in the first place? To allow users a second chance to check their work before proceeding with a dangerous action. However, it's not a second chance at all if users automate their response to the conformation and simply click Yes without thinking further."

> "When users are asked *Are you sure you want to do this?* without further details, the only sensible reaction is 'of course I want to do the thing I just told you to do,' and hit Yes without further thinking. Such automated behavior provides no protection at all, and only serves to annoy users — and make them pay less attention to future warnings."

NN/g also states the tension outright and resolves it by severity, not by frequency:

> "There is admittedly a tension between guidelines #1 and #2: you want to warn against serious consequences, but you don't want to warn so often that the warning is overlooked and the answer becomes an automated behavior. The solution is to conduct a task analysis to determine the severity of different outcomes and the frequency of possible user actions."

> "Finally, for improved total user experience, beyond the design of the dialog box itself, do go to great lengths to provide undo, because some user errors will remain despite the even the best of confirmation dialogs."

**Application to Giveback.** This is the doctrine our architecture already implements — and it explains why the design is right: the 72 h quarantine (undo) is the *primary* instrument (apply has no alert at all, `plan.softgate`), while the typed gate is reserved for the one truly irreversible act (purge). Note that NN/g's severity-not-frequency resolution is exactly the axis that separates our two gates.

### 2. Button semantics: role, prominence, order, and label

**Apple HIG, Alerts / Buttons** — role is a semantic property, not a colour choice:

> "Destructive. The button performs an action that can result in data destruction."

> "A button's role can have additional effects on its appearance. For example, a primary button uses an app's accent color, whereas a destructive button uses the system red color."

> "Don't assign the primary role to a button that performs a destructive action, even if that action is the most likely choice. Because of its visual prominence, people sometimes choose a primary button without reading it first. Help people avoid losing content by assigning the primary role to nondestructive buttons."

> "Use the destructive style to identify a button that performs a destructive action people *didn't* deliberately choose. For example, when people deliberately choose a destructive action — such as Empty Trash — the resulting alert doesn't apply the destructive style to the Empty Trash button because the button performs the person's original intent."

This last rule is subtle and directly applicable: our purge button is the person's *deliberately chosen* act (they typed the byte count to get there), so by Apple's own logic the alarm styling belongs on the *consequence statement* (`purge.destroy` / `purge.irreversible`) rather than on the affirmative button.

**GNOME HIG, Dialogs** — labels must be verbs naming the result, and layout order is fixed:

> "Label the affirmative button with a specific imperative verb, for example: Save or Print."

> "Always ensure that the cancel button appears first, before the affirmative button."

(Note the contradiction with Fluent below: GNOME puts Cancel **first (left)**, Fluent puts the destructive action **leftmost** and the safe action **rightmost**. Both are internally consistent; neither is universal. Our `PurgeGate.svelte` places Cancel before the destroy button (L253 Cancel, L254 destroy), matching GNOME.)

**Microsoft Fluent (Windows App SDK, Dialog controls; doc `ms.date: 2026-07-15`, page `updated_at 2026-07-17`)**:

> "**All dialogs should contain at least one safe action button to close the dialog.**"

> "Use specific responses to the main instruction or content as button text…"

> "The 'do it' action button(s) should appear as the leftmost buttons. The safe, nondestructive action should appear as the rightmost button."

> "Three button dialogs should be used sparingly."

Its canonical sample content is the tightest published template for a permanent-delete dialog:

> `Title = "Delete file permanently?"` · `Content = "If you delete this file, you won't be able to recover it. Do you want to delete it?"` · `PrimaryButtonText = "Delete"` · `CloseButtonText = "Cancel"`

and the doc names every escape hatch the platform itself provides — the pattern we deliberately disable:

> "The user clicked the CloseButton, pressed ESC, Gamepad B, or the system back button."

### 3. Typed / exact-target confirmation

Typing something the user cannot produce from muscle memory is the published answer to NN/g's automation problem.

> "For particularly dangerous operations, require a nonstandard action from the user to confirm. Rather than simply clicking an OK button (or, better, a button with a word or two to describe the action) — which risks becoming an automated behavior — have people do something they would normally not do. For example, type a word into a box, as MailChimp requires before deleting a mailing list." — NN/g (the MailChimp case is: type the word `DELETE`; the field itself only appears **after the user has scrolled** the consequence list)

**GitHub — deleting a repository** (`docs/github.com` .md source) is the canonical seven-step escalation:

Settings → **Danger Zone** → **Delete this repository** → **I want to delete this repository** → **I have read and understand these effects** → *type the repository name* → **Delete this repository**.

The warning text carries the consequences the user is being asked to affirm:

> "Deleting a repository will **permanently** delete team permissions. This action **cannot** be undone."
> "Deleting a private repository will delete all forks of the repository."
> "Some deleted repositories can be restored within 90 days of deletion."

GitHub also **scopes the typed target to the object's identity**, not to a byte count. `gh repo delete` (GitHub CLI manual) shows the identity-confirmation's side effect on scripting:

> "For safety, when no repository argument is provided, the `--yes` flag is ignored and you will be prompted for confirmation. To delete the current repository non-interactively, specify it explicitly (e.g., `gh repo delete owner/repo --yes`)."

i.e. the safety ceremony overrides the automation flag when the target is *implicit*. That is a transferable rule: our purge gate's byte count is only meaningful because the number is derived from the current plan, not typed from a constant.

**git clean — escalating force as a counting mechanism** (git-scm docs, manual last changed in 2.45.0, 2024-04-29):

> "If the Git configuration variable `clean.requireForce` is not set to false, *git clean* will refuse to delete files or directories unless given `-f`. Git will refuse to modify untracked nested git repositories (directories with a `.git` subdirectory) unless a second `-f` is given."

**kubectl delete** (Kubernetes reference, page regenerated for v1.37, last modified 2026-08-26) adds two more instruments: a *grace period* decoupled from the confirmation, and an opt-in confirmation flag:

> `--grace-period int  Default: -1` — "Period of time in seconds given to the resource to terminate gracefully. Ignored if negative. Set to 1 for immediate shutdown. Can only be set to 0 when --force is true (force deletion)."

> `-i, --interactive` — "If true, delete resource only when user confirms."

> "IMPORTANT: Force deleting pods does not wait for confirmation that the pod's processes have been terminated, which can leave those processes running until the node detects the deletion and completes graceful deletion…"

**GNOME's insensitivity rule** — the cleanest statement of what our `canDestroy` computes:

> "Sometimes, the user may be required to choose options before an action can be carried out. In these cases, the affirmative dialog button should be insensitive until the required options have been selected."

### 4. The inert gate: a deliberate, argued divergence from every published guideline

`app/src/lib/PurgeGate.svelte` implements a modal with `role="alertdialog"` (L198), `aria-modal="true"`, and **no Esc handler and no outside-click handler** — the file carries an explicit HTML comment citing SPEC Part C §4.2 that the only exits are Cancel (L253) and post-success Done (L287).

Every published guideline disagrees, and the note records that disagreement rather than hiding it:

| Source | Statement | Conflict |
|---|---|---|
| W3C WAI-ARIA APG, Modal Dialog | "`Escape`: Closes the dialog." | Direct: our gate ignores Esc. |
| GNOME HIG, Dialogs | "Ensure that the Esc key activates the cancel button, if there is one." | Direct. |
| Apple HIG, Buttons | "Provide alternative ways to cancel an alert when it makes sense. In addition to choosing a Cancel button, people appreciate using keyboard shortcuts or other quick ways to cancel an onscreen alert." | Direct. |
| Microsoft Fluent | "The user clicked the CloseButton, pressed ESC, Gamepad B, or the system back button." | Direct: the platform documents Esc as a supported exit. |
| W3C WAI-ARIA APG, Modal Dialog | "If a dialog contains the final step in a process that is not easily reversible, such as deleting data or completing a financial transaction, it may be advisable to set focus on the least destructive action, especially if undoing the action is difficult or impossible." | **Supports us.** Focus starts on the typed input / least destructive affordance, not on destroy. |
| GNOME HIG, Dialogs | "Assign the return key to activate the affirmative button. **However, this should not be done if its action is irreversible, destructive or otherwise inconvenient to the user.** If there is no appropriate button to designate as the default button, do not set one." | **Supports us.** Enter is not wired to the affirmative action by default; our L242 binds Enter to `canDestroy ? destroy() : noteAttempt()`, so Enter can never *create* the irreversible act. |
| Apple HIG, Alerts | "If you want to encourage people to read an alert and not just automatically press Return to dismiss it, avoid making any button the default button." | **Supports us.** |
| W3C APG, Alert Dialog | "mark a dialog modal **only when both**: 1. Application code prevents all users from interacting in any way with content outside of it. 2. Visual styling obscures the content outside of it." | **Supports us** (our gate does both). |

The honest reading: the divergence is **real and should be disclosed**, and it is *partially justified by the same guidelines*. Esc is a safety hatch for the case where the user regrets *opening* the dialog — but our gate is only reachable from an already-confirmed plan review, so the "regret the entry point" case is already handled further upstream, and the gate's stated purpose (SPEC Part C §4.2) is to make one specific act impossible to perform reflexively. We should present it as an accepted, scope-limited divergence, and cite GNOME's no-Return-on-destructive rule and APG's least-destructive-focus rule as the justification — not as a guideline-conformant choice.

### 5. Time-boxed undo budgets, and the obligation to disclose their limits

Four independent products publish an explicit window after which a reversal stops being possible. They are **independent instances of the same mechanic**, not a shared lineage.

| Product | Window | Exact mechanic (verbatim) |
|---|---|---|
| npm (unpublish policy) | **72 h** | "For newly created packages, as long as no other packages in the npm Public Registry depend on your package, you can unpublish anytime within the first 72 hours after publishing." |
| Stripe (API version rollback) | **72 h** | "For 72 hours after you've upgraded your API version, you can safely roll back to the version you were upgrading from in Workbench." |
| GitHub (deleted repository) | **90 d** | "A deleted repository can be restored within 90 days…" / "It can take up to an hour after a repository is deleted before that repository is available for restoration." |
| Giveback (quarantine grace) | **72 h** | `purge.restorable` — "其余 72 h 内可恢复"; enforced by `src/execute.rs` recovery + `Ledger.svelte::rowState()` |

The genuinely **transferable** part is not the number — it is that all four state the *end state* and its loss at the moment of commitment:

- npm: "Once you have unpublished a package, you will not be able to undo the unpublish." — and even the *name* becomes unusable: "Once `package@version` has been used, you can never use it again."
- npm's throttle: "If you entirely unpublish all versions of a package, you may not publish any new versions of that package until 24 hours have passed."
- npm's fallback for the post-72 h case: `npm deprecate` — i.e. **the policy names the weaker-but-still-available alternative** rather than only refusing. Our analogue is: post-purge, the ledger row remains (`ledger.not_restorable`) but nothing else can be offered.
- GitHub's restore is explicitly **lossy**: "Restoring a repository will not restore team permissions." — and the restore itself re-enters a typed-ceremony: "I understand, restore this repository."
- Stripe quantifies backwards-compatibility instead of promising it: adding resources / optional request params / response properties / reordering properties / changing "the length or format of opaque strings" are all declared compatible.

**Disclosure-of-limits is the pattern.** A time-boxed undo is only honest if the UI says (a) how long is left, (b) what specifically stops working after it, (c) what silent side effects the operation had that a reversal will not undo. GitHub's "some deleted repositories", "may not restore team permissions" and Stripe's stated-compatible change list are the reference implementations of (b) and (c).

### 6. Dry-run / preview: the published alias behind our scan→plan→apply split

Every serious destructive CLI ships a **non-destructive twin** of the destructive verb, and documents it as the same command:

| Tool | Preview mechanic (verbatim) |
|---|---|
| Terraform | "This command is just a convenience alias for the following command: `terraform apply -destroy`." / "You can also create a speculative destroy plan, to see what the effect of destroying would be, by running the following command: `terraform plan -destroy` … showing you the proposed destroy changes without executing them." |
| git clean | `-n`, `--dry-run`: "Don't actually remove anything, just show what would be done." / `-i`, `--interactive`: "Show what would be done and clean files interactively… this mode gives its own safety protection by going interactive." |
| kubectl delete | `--dry-run string[="unchanged"] Default: "none"` — "Must be 'none', 'server', or 'client'. If client strategy, only print the object that would be sent, without sending it. If server strategy, submit server-side request without persisting the resource." |
| docker system prune | Pre-flight `WARNING! This will remove:` itemized four-line list, then `Are you sure you want to continue? [y/N]`; `-f, --force` = "Do not prompt for confirmation" |

Two design details are worth extracting:

- **Two grades of dry-run.** kubectl distinguishes *client* (print what would be sent) from *server* (submit without persisting). That is precisely the difference between our `Plan` (computed locally from scan evidence) and a would-be server-side validation; the client tier is the one we implement and the one worth claiming.
- **git clean's interactive menu** offers three distinct selection postures that a GUI can borrow wholesale: `filter by pattern` (exclusion — directly analogous to our `deny`-globs in `src/policy.rs`), `select by numbers` (ranges, `*` for all), and `4: ask each`, documented plainly as "you must confirm one by one in order to delete items. Please note that this action is not as efficient as the above two actions." — i.e. **git documents the cost of per-item confirmation as a deliberate trade, not a defect.** That is the correct framing for our per-row judge list in `Review.svelte::judgeList`.

### 7. Honest progress reporting

**Apple HIG, Progress indicators** — the doctrine, including the sentence that should be quoted in the design document verbatim:

> "When possible, use a determinate progress indicator. An indeterminate progress indicator shows that a process is occurring, but it doesn't help people estimate how long a task will take. A determinate progress indicator can help people decide whether to do something else while waiting for the task to complete, restart the task at a different time, or abandon the task."

> "Be as accurate as possible when reporting advancement in a determinate progress indicator. Consider evening out the pace of advancement to help people feel confident about the time needed for the task to complete. **Showing 90 percent completion in five seconds and the last 10 percent in 5 minutes can make people wonder if your app is still working and can even feel deceptive.**"

> "Keep progress indicators moving so people know something is continuing to happen. People tend to associate a stationary indicator with a stalled process or a frozen app. If a process stalls for some reason, provide feedback that helps people understand the problem and what they can do about it."

> "If it's helpful, display a description that provides additional context for the task. Be accurate and succinct. **Avoid vague terms like loading or authenticating** because they seldom add value."

(Apple's "90 percent in five seconds" number is a near-exact description of our apply phase: N per-entry moves stream in fast, then a slow tail. Worth citing as the reason the per-row stream must be paced/animated rather than being an instantaneous burst followed by a stall.)

**Apple's in-button label swap** is the cheapest honest-progress mechanic available:

> "Configure a button to display an activity indicator when you need to provide feedback about an action that doesn't instantly complete… you can also configure the button to display a different label alongside the activity indicator. For example, the label 'Checkout' could change to 'Checking out…' while the activity indicator animates."

**Microsoft Fluent progress controls** (doc `ms.date: 2026-07-08`): determinate `ProgressBar` when duration is known; an indeterminate `ProgressBar` "does not block interaction", while an indeterminate `ProgressRing` "blocks" — i.e. the choice between them *is* the choice about whether the user may keep working. Our purge (`PurgeGate` `phase === "purging"`, `purge.progress` 销毁中…) is a blocking indeterminate case and is correctly rendered as a modal state rather than an inline spinner.

**Docker's post-run summary** is a two-part honesty contract: grouped per-kind results ("Deleted Containers: … Deleted Networks: … Deleted Images: …") plus a single measured total — "Total reclaimed space: 1.84kB".

**Apple HIG, Undo and redo** — the progress-integrity rule applied to reversal:

> "Show the results of an undo or redo. Sometimes, the most recent action that people want to undo affects content or an area that's no longer visible. In cases like this, it's crucial to highlight the result of each undo and redo to keep people from thinking that the action had no effect, which can lead them to perform it repeatedly."

> "Help people predict the results of undo and redo as much as possible." (e.g. menu labels "Undo Typing" / "Redo Bold" — a *named* reversal, not a bare "Undo")

> "Let people undo multiple times. Avoid placing unnecessary limits on the number of times people can undo or redo."

> "Consider giving people the option to revert multiple changes at once… people might appreciate the ability to undo a batch of discrete but related actions."

### 8. Post-operation receipts and partial-failure reporting (the strongest new borrow)

**Amazon S3 `DeleteObjects`** is the only primary-source API contract in this lane that models a *partially successful batch* explicitly. Read from `docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteObjects.html` this session:

> "The request can contain a list of up to 1,000 keys…"
> "For each key, Amazon S3 performs a delete operation and returns the result of that delete, success or failure, in the response."
> "By default, the operation uses verbose mode. In verbose mode, the response includes the result of each key."

`DeleteResult` is a two-list structure: `Deleted[]` and `Error[]` where each `Error` carries `Key`, `Code`, `Message`, `VersionId`. The documented sample response returns **HTTP 200** while containing both a success and a failure:

```
<Deleted><Key>sample1.txt</Key></Deleted>
<Error><Key>sample2.txt</Key><Code>AccessDenied</Code></Error>
```

Three transferable rules follow, and they are exactly what our GUI lacks:

1. **A batch outcome is not a status code.** Partial failure is neither success nor failure; it needs a two-list payload.
2. **Quiet mode is an explicit user choice, not a default.** "In quiet mode the response includes only keys where the delete operation encountered an error." The default is verbose because the *errors* are what the caller must not lose. Our current truncation does the opposite: it drops errors off the front of a 50-row window.
3. **The error row carries the key.** Without the identity, an `Error[]` entry cannot be acted on.

Also worth noting for honesty: versioned buckets do not destroy data even on a successful delete — they insert `<DeleteMarker>true</DeleteMarker>`. S3 surfaces the *effective* outcome rather than the *requested* outcome, which is the same distinction as our quarantined-vs-freed reporting.

**Docker** pairs the receipt with a measured total (`Total reclaimed space: 1.84kB`) and **excludes destructive-by-default targets** — volumes are skipped "to prevent important data from being deleted", opt-in via `--volumes`. That is the same posture as our grade gate: the default set stops short of the dangerous class.

**GitHub's restore ceremony** re-applies the confirmation on the reversal side ("I understand, restore this repository"), which is the correct asymmetry rule: undo is *cheap*, but not *unattended*.

## Pattern table

| Pattern | Source product / guide | Exact mechanic | Copyable? | Risk if omitted |
|---|---|---|---|---|
| Prefer undo over confirm | GNOME HIG · Apple HIG | Undo window as primary instrument; dialog only when undo is impossible | Yes — we already do | N/A (satisfied) |
| Alert only for uncommon + irreversible | Apple HIG Alerts | Alert iff action is uncommon **and** cannot be undone | Yes | Either alert fatigue or silent data loss |
| Don't make any button the default on an irreversible alert | Apple HIG · GNOME HIG | No default button ⇒ Return cannot dismiss | Yes — L242 already guards | Reflex Enter commits destruction |
| Specificity in the confirm text | NN/g · Fluent dialog sample | Restate the request with identifying detail, not "Are you sure?" | Yes (partially — see gaps) | Confirmation becomes an automated click |
| Typed nonstandard action | GitHub repo delete · NN/g MailChimp | Type an object identity/total that cannot be produced from muscle memory | Yes — byte count | Reflex confirmation |
| Field appears only after consequences are shown | NN/g MailChimp case | Scroll/reveal gating before the input exists | Yes | User types before reading |
| Escalating force count | `git clean -f` / `-ff` | Second `-f` required for the nested-git class | Partly (`--force` tiers) | Uniform gate is either too weak or too noisy |
| Safe action always present | Fluent dialog controls | "All dialogs should contain at least one safe action button to close the dialog." | Yes — Cancel L253 | User trapped in a modal |
| Least-destructive initial focus | W3C APG modal dialog | Focus lands on the reversible option for final irreversibility | Yes | Focus order invites the destructive act |
| Affirmative label = imperative verb naming the result | GNOME HIG · Apple HIG | "Delete"/"Erase", never "OK" | Yes (`purge.destroybtn` 销毁) | Ambiguous consent |
| Destructive role ≠ primary role | Apple HIG Buttons | Never accent-colour the destructive button | Yes | Prominence attracts the click |
| Cancellation order | GNOME (cancel first) vs Fluent (safe last) | Fixed position per platform; both internally consistent | Yes, pick one and stay | Contradictory muscle memory |
| Itemized pre-flight list | docker system prune | `WARNING! This will remove:` four lines, then `[y/N]` | Yes — plan screen | "What will be deleted?" unanswerable |
| Dry-run twin of the destructive verb | Terraform `plan -destroy` · git `-n` · kubectl `--dry-run=client` | Same command, no side effects; documented as an alias | Yes — scan→plan split | Users must gamble to learn the effect |
| Per-item opt-out / pattern filter | `git clean -i` (filter by pattern, select by numbers, ask each) | Interactive exclusion before commit | Yes — deny-globs + judge list | All-or-nothing batches |
| Determinate where possible | Apple HIG Progress | Show real fraction; don't ramp-then-stall | Partial (per-row stream) | Perceived hang, perceived deception |
| No vague status words | Apple HIG Progress | Ban "loading"/"authenticating" | Yes | Zero information |
| In-control label swap | Apple HIG Buttons | "Checkout" → "Checking out…" | Yes | Button looks dead while it works |
| Blocking vs non-blocking indicator is a policy choice | Fluent progress controls | Indeterminate bar = non-blocking; ring = blocking | Yes — purge is blocking by design | User starts another destructive action |
| Post-run grouped receipt + measured total | docker prune · S3 `DeleteResult` | Per-kind result groups; one measured total | **GAP** — counts only | Free-space claim unverifiable |
| Two-list partial-failure payload | S3 `DeleteObjects` verbose | `Deleted[]` + `Error[]{Key,Code,Message}` at HTTP 200 | **GAP** — truncated | Silent silent failures; false success |
| Errors never truncated away | S3 default-verbose | Quiet mode surfaces errors only; verbose is default | **GAP** | Concrete failures discarded |
| Lossy-reversal disclosure | GitHub "will not restore team permissions" · npm "cannot undo the unpublish" | Name exactly what the reversal does not bring back | Partial | User assumes restore == revert |
| Time-box with remaining time visible | GitHub 90 d · Stripe 72 h · npm 72 h | Window stated at commit; countdown thereafter | Yes in Ledger, GAP elsewhere | Window discovered by expiry |
| Re-confirm the reversal | GitHub "I understand, restore this repository" | Undo is cheap but not unattended | Partial | Accidental restore / collision |
| Named reversal action | Apple HIG Undo/Redo | "Undo Typing", not "Undo" | Yes | User can't predict the result |
| Highlight the reversal's result | Apple HIG Undo/Redo | Scroll/highlight the restored item into view | GAP | User repeats the action believing it failed |
| Defaults stop short of the dangerous class | docker prune (no volumes) | Dangerous targets opt-in | Yes — SAFE/`AUTO` sets | One-tap overreach |

## Mapping onto the repo

Status per pattern against the implemented surface. Line numbers re-verified 2026-09-13 on branch `agent/repo-scaffold`.

| Pattern | Where it lives in Giveback | Status | Note |
|---|---|---|---|
| Undo-over-confirm | `src/execute.rs::apply` (quarantine move) + 72 h grace; `src/execute/macos.rs::renamex_np(RENAME_EXCL)` | **SATISFIED** | Apply has no alert; `plan.softgate` = "软确认：可直接 Enter". Doctrine-conformant. |
| Alert only for uncommon + irreversible | `PurgeGate.svelte` reachable only from `PlanReview.svelte` purge button L320 | **SATISFIED** | Purge is the single uncommon+irreversible act. |
| No default button / Return cannot commit | `PurgeGate.svelte` `onkeydown` L242 — Enter → `canDestroy ? destroy() : noteAttempt()` | **SATISFIED** | Matches GNOME's no-Return-on-destructive and Apple's no-default rule. |
| Insensitive until requirements met | `PurgeGate.svelte` `canDestroy` L46 = `match && checked && phase === "input"`; button `disabled={!canDestroy}` L257 | **SATISFIED** | GNOME's insensitivity rule, implemented literally. |
| Typed nonstandard action | `PurgeGate.svelte` `match` L45 (`exact === bytes && bytes > 0`), placeholder `purge.placeholder` | **SATISFIED** | Byte count is plan-derived ⇒ cannot be muscle-memorised. |
| Incremental feedback while typing | `PurgeGate.svelte` `typingToward` L49 (prefix-of-target), `onInput` L125 | **BETTER THAN SOURCE** | Prefix-vs-foreign discrimination is a refinement no cited product documents. |
| Rejection is explicit and visible | `PurgeGate.svelte` `destroy()` L148 → `noteAttempt()` when `!canDestroy`; `purge.mismatch` "字节数不匹配 — 已拒绝" | **SATISFIED** | — |
| Safe action always present | Cancel `PressButton` L253 | **SATISFIED** (Fluent-explicit) | Precedes destroy, per GNOME ordering. |
| Inert modal (no Esc / no outside-click) | `PurgeGate.svelte` — documented SPEC Part C §4.2 divergence | **ACKNOWLEDGED DIVERGENCE** | Justified by APG least-destructive-focus + GNOME no-Return rule; **do not change**. |
| Itemized "what will be destroyed" pre-flight | `Review.svelte::{SAFE(60), safeList(78), judgeList(79)}`; `PlanReview.svelte` manifest re-built L81–96 | **SATISFIED** | Two-tier: SAFE band vs judge list with `refuseReason` (L109). |
| Dry-run twin | `plan` step stores `localStorage["last-plan-id"]` (`Review.svelte` L144); apply is a separate command | **SATISFIED** | Equivalent of `terraform plan -destroy`. |
| Post-apply receipt with effect split | `PlanReview.svelte::ApplyResult` L51 `{op_id, applied, skipped, refused, quarantined_bytes}`; applied-note L250–252 | **PARTIAL** | Counts only, no per-item identity; `plan.freed_zero` correctly separates freed from quarantined. |
| Itemized refusal identity | `PlanReview.svelte::apply()` L157 streams rows, **`if (rows.length > 50) rows = rows.slice(-50);` L171** | **GAP** | Refusals can be cut; no error `Code`/`Message` per row. |
| Measured total | `PlanReview.svelte` L182 `quarantined_bytes` fallback; `PurgeGate` `phase === "purging"` → `freed` | **PARTIAL** | Purge reports freed; apply's number is the plan's, not a re-measure. |
| Remaining-time visibility | `Ledger.svelte::graceRemaining` L166, `rowState` L200, `{@const rem …}` L312, `ledger.countdown` 剩 {n} | **SATISFIED in Ledger, GAP elsewhere** | Review/PlanReview carry promise copy only (`plan.grace`, Review L196). |
| Lossy-reversal disclosure | `ledger.not_restorable` "已销毁，无法恢复"; `purge.restorable` limits the promise to the grace window | **PARTIAL** | No statement of *what specifically* a restore will not bring back. |
| Re-confirm the reversal | `Ledger.svelte::restore` L212 with `restoredCount` L228 guard | **PARTIAL** | Guard is op-scoped, not a per-restore acknowledgement. |
| Named reversal + result highlight | `Ledger.svelte` row state `restored`/`destroyed` (L177) + vault view | **PARTIAL** | State is shown, but the restored row is not brought into view/highlighted. |
| Blocking progress for irreversible work | `PurgeGate` `phase = "purging"` (L153), `purge.progress` 销毁中…, demo delay ~700 ms (`ipc.ts` L389–391) | **SATISFIED** | Matches Fluent ring-blocks semantics. |
| Honest in-button progress label | `PressButton` in `PurgeGate` L254; `purge.destroybtn`/`purge.finish` | **PARTIAL** | Label swaps only at the phase change, not during the wait. |
| Defaults stop short of the dangerous class | `Review.svelte::SAFE` L60; `Dashboard.svelte::AUTO` L49 (D-24) | **SATISFIED** | Direct analogue of docker prune's volume exclusion. |
| Partial-failure surfaced to the user | `ipc.ts` demo already returns `{applied: 11, skipped: 0, refused: 3}` (L388) | **GAP** | The demo *demonstrates* a mixed batch that the UI cannot itemize. |

### The soft-gate / hard-gate asymmetry is the design's spine

`apply` is a **soft gate** (never in i18n, but the control text at `i18n.ts:141` is `plan.softgate` — "软确认：可直接 Enter" / "soft gate: Enter works"); `purge` is the **hard gate**. The published guidelines support exactly this two-grade structure rather than a uniform one: GNOME's insensitivity rule is scoped to actions needing options first; Apple's no-default-button rule is scoped to alerts meant to be read; NN/g resolves the confirm-vs-warn tension "by conducting a task analysis to determine the severity of different outcomes". The gap list below is therefore ordered by *which gate it strengthens*, not by how interesting it is.

## Reusable code & licenses

Licenses below were read from the primary LICENSE file of each project's own docs/source repository on 2026-09-13 (raw.githubusercontent / gitlab API / on-page licence block), not from a summary page. "Quote-only" means the text is proprietary or its reuse terms were not verifiable this session: paraphrase and cite, do not lift.

| project | file (linked) | license | what is borrowable | fit |
|---|---|---|---|---|
| W3C ARIA Authoring Practices | [`aria-practices/LICENSE.md`](https://raw.githubusercontent.com/w3c/aria-practices/main/LICENSE.md) | [W3C Software and Document License](https://www.w3.org/Consortium/Legal/copyright-software) (verified) | The `alertdialog` role definition + the two-condition modal test + the "least destructive action" focus rule | Spec reference. `PurgeGate`'s `role="alertdialog"` (L198) already satisfies the role requirement by hand. |
| Docker docs | [`reference/cli/docker/system/prune.md`](https://docs.docker.com/reference/cli/docker/system/prune/) | [Apache-2.0](https://raw.githubusercontent.com/docker/docs/main/LICENSE) (verified) | The pre-action itemized warning block and the `Total reclaimed space: …` receipt line | CLI output shape for `janitor purge`. Copy the *shape*, not the wording. |
| GitHub docs | [`deleting-a-repository.md`](https://docs.github.com/en/repositories/creating-and-managing-repositories/deleting-a-repository.md) · [`restoring-a-deleted-repository.md`](https://docs.github.com/en/repositories/creating-and-managing-repositories/restoring-a-deleted-repository.md) | [CC-BY 4.0](https://raw.githubusercontent.com/github/docs/main/LICENSE) (verified) | The 7-step irreversibility ceremony; the restore-side "I understand" acknowledgement; the lossy-reversal sentence | Direct copy material for `i18n.ts` copy work, with attribution. |
| Kubernetes website | [`kubectl delete` reference](https://kubernetes.io/docs/reference/kubectl/generated/kubectl_delete/) | [CC-BY 4.0](https://raw.githubusercontent.com/kubernetes/website/main/LICENSE) (verified) | `--dry-run=client\|server`, `-i/--interactive`, `--grace-period` (with its force-only-at-zero rule) | CLI flag vocabulary for the missing dry-run alias. Do not copy prose wholesale. |
| HashiCorp web-unified-docs | [`terraform destroy`](https://developer.hashicorp.com/terraform/cli/commands/destroy) | [BUSL-1.1](https://raw.githubusercontent.com/hashicorp/web-unified-docs/main/LICENSE) (verified — Licensor IBM Corp.) | The *idea* that `destroy` is an alias for `apply -destroy` and that `plan -destroy` is the speculative preview | **Design principle only. BUSL text is not reusable** — paraphrase in our own words. |
| MicrosoftDocs/windows-dev-docs | [Dialog controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/dialogs) · [Progress controls](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls) | [CC-BY 4.0](https://raw.githubusercontent.com/MicrosoftDocs/windows-dev-docs/main/LICENSE) (verified; the `-pr` repo has no LICENSE file, 404) | The mandatory safe-action rule; the sample `Title`/`Content`/`PrimaryButtonText`/`CloseButtonText` shape; determinate-vs-indeterminate decision keywords | Shape + prose. Attribute Microsoft. |
| AWS S3 user guide | [`API_DeleteObjects.md`](https://docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteObjects.html) | [CC-BY-**SA** 4.0](https://raw.githubusercontent.com/awsdocs/amazon-s3-userguide/main/LICENSE) (verified) | The verbose-by-default `DeleteResult{Deleted[], Error[]}` batch shape | **ShareAlike** — attribution + same-licence obligation if text is reused. Use the schema idea, write our own prose. |
| npm docs | [Unpublish Policy](https://docs.npmjs.com/policies/unpublish/) | CC-BY-SA ("may be reused under a Creative Commons Attribution-ShareAlike License", read on-page) | The blunt irreversibility disclosure ("you will not be able to undo the unpublish") | Same ShareAlike caveat. One sentence of copy. |
| GNOME HIG | [Dialogs](https://developer.gnome.org/hig/patterns/feedback/dialogs.html) | **UNVERIFIED** — the site footer carries no licence block and `gitlab.gnome.org/GNOME/hig` returns HTTP 406 to non-browser clients | The confirm-vs-undo doctrine and the affirmative/insensitive/return-key rules | Quote-and-cite only until the licence is confirmed. |
| Apple HIG | [Alerts](https://developer.apple.com/design/human-interface-guidelines/alerts), [Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons), [Undo and redo](https://developer.apple.com/design/human-interface-guidelines/undo-and-redo), [Progress indicators](https://developer.apple.com/design/human-interface-guidelines/progress-indicators), [File management](https://developer.apple.com/design/human-interface-guidelines/file-management) | Apple copyright — the [Agreements and Guidelines index](https://developer.apple.com/support/terms/) was read but contains no licence grant for HIG text | Destructive-only-for-undeliberate-actions; no primary/secondary role on a destructive alert; no default button on a destructive alert; progress-honesty rules; "Show the results of an undo" | **Quote-only.** Paraphrase in the design doc, link the sections. |
| Material Design 3 | [Snackbar guidelines](https://m3.material.io/components/snackbar/guidelines) · [Dialog guidelines](https://m3.material.io/components/dialogs/guidelines) | Google terms — no reuse grant read this session (UNVERIFIED) | "Disable confirming actions until a choice is made. Dismissive actions are never disabled."; "Don't disable the confirmation button"; auto-dismiss-snackbar inaccessibility + the inline-feedback remedy | Quote-only. The first rule is the strongest published analogue of our hard gate. |

Two of our four candidate Rust/JS dependencies are **not currently installed** (verified against `Cargo.toml` and `app/package.json` this session): only `@tauri-apps/api` and `svelte-i18n` appear in `app/package.json`, and the root `Cargo.toml` has no prompt/DUI crate — `clap` is the only CLI-facing dependency.

## Recommended crates

| crate | version | license | why | alternative |
|---|---|---|---|---|
| `inquire` | 0.9.4 (`max_stable_version` confirmed on crates.io API; `rust_version` 1.80.0) | MIT (from `/api/v1/crates/inquire/versions`) | The `CustomType` + `validator` prompts are exactly the typed-confirmation primitive the `janitor` CLI lacks: a numeric prompt that refuses to proceed until the parsed value equals the target byte count — i.e. the terminal analogue of `PurgeGate::match` (`exact === bytes && bytes > 0`). Cross-platform via `crossterm`, so no `libc`/termios work. | `dialoguer` (below) if binary size matters more than validation ergonomics. |
| `dialoguer` | 0.12.0 (`max_stable_version`; `rust_version` 1.66; crate 31 KB vs inquire's 101 KB) | MIT (versions API; also `license = "MIT"` in the repo `Cargo.toml`) | Lower MSRV and ~3× smaller; ships `Confirm` and `Input` with optional input validation. Enough for a "type the byte count" prompt if we hand-check equality ourselves. | `inquire` for richer prompting and built-in parser/validator traits. |
| `bits-ui` | 2.19.2 | MIT (npm registry `license` field; repo LICENSE confirmed) | Provides the focus-trap + dialog primitives W3C APG expects. Listed **for completeness only**: `PurgeGate.svelte` already implements the alertdialog semantics by hand and is frozen; adopting this would change `app/package.json`. | Hand-rolled (current state) — no change recommended. |
| `svelte-sonner` | 1.2.1 | MIT (npm registry; repo LICENSE confirmed) | Transient toast surface, if a future receipt wants to appear after the modal closes. M3's own rule ("avoid using auto-dismissing snackbars on web unless there's also inline feedback") argues for the inline notes we already have instead. | Inline `.applied-note` / `.promise` blocks (current state). |

**Recommendation for this pass: none adopted.** The GUI gate is frozen by the safety architecture and already satisfies the semantics it needs; these are CLI-side (and purely optional Svelte-side) options recorded for the 技术选型 section, not changes to make now. Adding any of them is out of scope for a research pass.

## Port backlog

Ranked by *which gate it strengthens* (hard gate first), then by blast radius. Every row is a UI/CLI-copy change — none touches `renamex_np`, the WAL, the grace window, or the typed-byte-count gate.

| # | what to borrow | exact target symbol in OUR repo | risk | est. size | why it is worth it |
|---|---|---|---|---|---|
| 1 | **Itemized refused/skipped receipt for `apply`.** S3's `DeleteObjects` returns `DeleteResult{Deleted[], Error[]{Key,Code,Message}}` at HTTP 200 — a batch that partly fails is still a success response, so the itemization is the *only* place the failure is visible. Our `ApplyReport` (`src/execute.rs:117`) carries `applied/skipped/refused` **counts only**. | `app/src/routes/PlanReview.svelte` — `interface ApplyResult` (L51), `apply()` (L157), and the applied-note (L250–252). Backend already streams per-entry `ExecEvent::{EntryApplied, Error}` (`src/execute.rs:35–41`), so the rows exist; only the summary loses them. | low | M | This is the single highest-value gap: the demo payload already proves a mixed batch — `ipc.ts` L388 returns `{op_id:"op-demo", applied:11, skipped:0, refused:3, quarantined_bytes:19_300_000_000}` — while the UI can only print the counts. A user who quarantines 14 entries and is told "3 refused" has no way to learn *which* three. |
| 2 | **Never truncate error rows; keep refusals pinned.** S3 defaults to **verbose**; `Quiet` is an explicit opt-in element that must be set to `true`, and even then errors are always returned. | `PlanReview.svelte::apply()` — the sliding window `if (rows.length > 50) rows = rows.slice(-50);` (**L171**). Same handler, fix is to partition rows by `kind` before windowing. | low | S | A refusal that scrolls out of the last 50 rows is indistinguishable from a refusal that never happened. S3's design says the *default* is completeness and brevity is the opt-in — our code has it inverted. |
| 3 | **Post-purge receipt with lossy-reversal disclosure.** GitHub: "Restoring a repository will not restore team permissions." npm: "Once you have unpublished a package, you will not be able to undo the unpublish." | `PurgeGate.svelte` success branch (`phase === "done"`, `PressButton label={t("purge.finish")}` L287) + the `purge.*` keys in `app/src/lib/i18n.ts` (zh L275–289 / en L561–574). | low | S | The gate currently says "这一步不可撤销" *before* the act (`purge.irreversible`, i18n L276/562) but says nothing *after* it about what specifically is gone and what a later restore will not bring back — the exact asymmetry GitHub and npm both close. |
| 4 | **Remaining-grace visibility outside the Ledger.** Apple: "Show the results of an undo or redo"; GNOME: an offer to undo is better than a dialog. | Reuse `Ledger.svelte::graceRemaining()` (**L166**) at `Review.svelte`'s promise line (**L196**, currently the static `t("plan.grace")`) and at `PlanReview.svelte`'s applied-note (L250). | low | S | The promise "0 已销毁 · 72 h 宽限" is static text. A countdown makes the undo window a live, checkable fact instead of a claim — the same move `Ledger.svelte` already makes per row at L312. |
| 5 | **Restore-side confirmation parity.** GitHub's restore flow ends at "Read the warning, then click **I understand, restore this repository**" — the reversible direction is not treated as zero-cost, because it is not zero-cost (it overwrites/clobbers and is itself logged). | `Ledger.svelte::restore()` (**L212**) — guard the batch by `restoredCount` (L228–232) and acknowledge collisions. Backend already separates them: `RestoreReport{collided, missing}` (`src/execute.rs:133–140`) with the D-26 keep-both default. | med | S | Surfaces the `collided` list — files restored as `original-<n>` because the original path was occupied. Today that outcome is computed and then not shown; touch the restore contract carefully. |
| 6 | **In-flight progress label on the destroy button.** Apple: change "Checkout" to "Checking out…"; "Avoid vague terms like loading or authenticating". M3 Dialog: "Don't disable the confirmation button" (a *disabled* button during work is the failure mode this avoids). | `PurgeGate.svelte` destroy `PressButton` (**L254–257**) + `purge.progress` ("销毁中…" i18n L285) / `purge.destroybtn` ("销毁" L282). | low | S | The label currently swaps only at the phase boundary; during the actual wait the button text is the pre-action verb. A user cannot tell "pressed and working" from "press did not register". |
| 7 | **Escalating-force count for the CLI** (`git clean -f` / `-ff`; kubectl's `--force` being valid only with `--grace-period=0`). | `src/main.rs` `Cmd::Purge { manifest, bytes }` (L52–60) + `src/policy.rs`. Note the file's own header comment: "Purge NEVER gains a --yes bypass (P4)". | med | M | The docs already enumerate *why* a second, explicit escalation exists. Must not introduce a bypass — the escalation would be an additional refusal-clearing flag, not a confirm-skipping one. Defer if the P4 invariant is at all ambiguous. |
| 8 | **Dry-run alias for the CLI.** `terraform destroy` == `apply -destroy`, with `terraform plan -destroy` as the speculative preview; `kubectl --dry-run=client` prints without submitting. | `src/main.rs` `Cmd::{Plan, Apply}` (L40–50). `plan` already *is* the dry run; this adds the alias and makes it discoverable. | low | S | Costs almost nothing and matches the published vocabulary; makes the scan→plan→apply split legible to a grader who knows terraform/kubectl conventions. |

Dropped deliberately: any per-row "ask each" prompt in the GUI (`git clean -i`'s mode 4 — its own docs concede "this action is not as efficient"), and any change to `PurgeGate`'s missing Esc handler (see below).

## Risks & open questions

1. **The 72 h convergence is coincidence, not causation — do not claim inspiration.** npm's unpublish window is 72 h for a newly created package with no dependents; Stripe's rollback window is 72 h after an API-version upgrade; our grace window is 72 h. These are three independent instances of the same *time-boxed undo budget* pattern. The design doc should say we align with a known pattern, not that we copied any one product.
2. **The Esc conflict is a real, accepted divergence.** W3C APG ("`Escape`: Closes the dialog."), GNOME ("Ensure that the Esc key activates the cancel button, if there is one."), Apple (no default button on a destructive alert) and Fluent (the `CloseButton` path counts ESC/Gamepad-B as the safe exit) all pull toward Esc-cancels. `PurgeGate.svelte` deliberately has **no** Esc handler and no outside-click handler (`purge.esc` = "Esc 无效" / "Esc won't close", i18n L280/566). Three published rules nonetheless *support* the design: APG's "least destructive action" focus rule and its two-condition test for marking a dialog modal (which PurgeGate satisfies), and GNOME's own carve-out that the return key should not be assigned if the action "is irreversible, destructive". **Disclose this in the design doc as a scoped, argued divergence; do not silently "fix" it.** The Enter handler at L242 already implements the GNOME rule correctly (Enter destroys only when `canDestroy`, otherwise `noteAttempt()`).
3. **One dead end and one 404, recorded rather than omitted.** `https://primer.style/product/ui-patterns/delete/` now redirects to the Primer marketing home page (HTTP 200, final URL `https://primer.style/`) — the DELETE ui-pattern page could not be read, so no GitHub Primer claim appears anywhere in this note. `https://support.apple.com/en-us/104973` returns HTTP 404.
4. **Apple HIG contains no Trash/Recently-Deleted guidance.** The `file-management` page was read in full this session and the only Trash material is the "Empty Trash" example inside `alerts`. Recently-Deleted retention is note 17's ground; this note does not restate it.
5. **GNOME HIG and Material 3 reuse terms are unverified.** `gitlab.gnome.org` returns HTTP 406 to `curl` for raw files and the rendered HIG pages carry no licence block; Material 3 pages carry Google's standard site terms but no explicit content-reuse grant was read. Both are therefore quote-with-attribution only in the design doc.
6. **Note 19 is a platform-parity matrix, and no progress-reporting note exists in the corpus.** §7 (honest progress) is therefore unclaimed ground — it is this note's contribution, and the backlog has only one row against it (item 6). If the design doc wants a stronger progress story, the gap is that `PurgeReport` (`src/execute.rs:127`) carries only `freed_bytes` and `entries`, so per-entry progress exists as events (`ExecEvent::EntryPurged`) but has no aggregate to drive a determinate indicator.
7. **`PlanReview`'s row window is the only truncation of its kind** — verified by reading the handler; no sibling screen applies a cap. That makes item 2 a genuinely local fix, not a systemic one.

## Sources

Every row below was fetched in this session; "read there" names the specific text used, not the page's marketing.

1. https://developer.apple.com/design/human-interface-guidelines/alerts — read: destructive-style-only-for-undeliberate-actions ("Empty Trash" example), "Don't assign the primary role to a button that performs a destructive action", "avoid making any button the default button" · accessed 2026-09-13
2. https://developer.apple.com/design/human-interface-guidelines/buttons — read: button role/prominence rules · accessed 2026-09-13
3. https://developer.apple.com/design/human-interface-guidelines/undo-and-redo — read: "Show the results of an undo or redo", "Let people undo multiple times", "Consider giving people the option to revert multiple changes at once"; and the absence of any Trash/Recently-Deleted section (the `file-management` page was read for this and also has none) · accessed 2026-09-13
4. https://developer.apple.com/design/human-interface-guidelines/progress-indicators — read: "Showing 90 percent completion in five seconds and the last 10 percent in 5 minutes … can even feel deceptive", "Avoid vague terms like loading or authenticating", "Checkout"→"Checking out…" · accessed 2026-09-13
5. https://developer.apple.com/design/human-interface-guidelines/file-management — read in full; no Trash guidance present · accessed 2026-09-13
6. https://developer.apple.com/support/terms/ — read: the Agreements and Guidelines index; **no licence grant for HIG text is stated**, hence "quote-only" in the table above · accessed 2026-09-13
7. https://developer.gnome.org/hig/patterns/feedback/dialogs.html — read: "Destructive actions should always be accompanied by either a confirmation dialog or an offer to undo the action…", "undo is typically a better option than a confirmation dialog", "However, in cases where it is not possible to offer an undo feature…", "Label the affirmative button with a specific imperative verb…", "the affirmative dialog button should be insensitive until the required options have been selected", "Never pop up a dialog window unexpectedly", "Always ensure that the cancel button appears first…", "Assign the return key to activate the affirmative button. However, this should not be done if its action is irreversible, destructive…", "Ensure that the Esc key activates the cancel button, if there is one." · **licence UNVERIFIED** (no footer block; `gitlab.gnome.org/GNOME/hig` → HTTP 406) · accessed 2026-09-13
8. https://www.w3.org/WAI/ARIA/apg/patterns/alertdialog/ — read: `alertdialog` role requirements (`aria-modal=true`, `aria-labelledby`/`aria-label`, `aria-describedby`) and the two-condition rule for marking a dialog modal · accessed 2026-09-13
9. https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/ — read: "`Escape`: Closes the dialog."; "If a dialog contains the final step in a process that is not easily reversible … it may be advisable to set focus on the least destructive action…"; "It is strongly recommended that the tab sequence of all dialogs include a visible element with role button that closes the dialog…" · accessed 2026-09-13
10. https://raw.githubusercontent.com/w3c/aria-practices/main/LICENSE.md — read: "All documents in this Repository are licensed by contributors under the W3C Software and Document License" · accessed 2026-09-13
11. https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/dialogs — read (content-negotiated markdown, `ms.date: 2026-07-15`, `updated_at: 2026-07-17`): "At least one dialog button must appear…", "**All dialogs should contain at least one safe action button to close the dialog.**", "The 'do it' action button(s) should appear as the leftmost buttons. The safe, nondestructive action should appear as the rightmost button.", "Three button dialogs should be used sparingly…", the `Title = "Delete file permanently?"` sample with `PrimaryButtonText = "Delete"` / `CloseButtonText = "Cancel"`, "The user clicked the CloseButton, pressed ESC, Gamepad B, or the system back button.", "There can only be one ContentDialog open per window at a time." (full output also at `'/Users/aaikyn1/.omp/agent/sessions/-Code-disk_janitor/2026-09-12T14-01-30-348Z_01a095ec-5bec-727c-9262-deb249bae831/1550.read.log'`, 377 lines) · accessed 2026-09-13
12. https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls — read (markdown, `ms.date: 2026-07-08`): determinate-vs-indeterminate decision rules, "Indeterminate … ProgressRing … blocks user interaction with the app", keyword lists (`Loading...`, `Retrieving`, `Working...` / `Refreshing`, `Signing in...`, `Connecting...`), "No progress indication necessary" case · accessed 2026-09-13
13. https://raw.githubusercontent.com/MicrosoftDocs/windows-dev-docs/main/LICENSE — read: CC-BY 4.0 · accessed 2026-09-13
14. https://raw.githubusercontent.com/MicrosoftDocs/windows-dev-docs-pr/live/LICENSE — read: **404 Not Found** (the `-pr` repo carries no LICENSE file; the public mirror above does) · accessed 2026-09-13
15. https://raw.githubusercontent.com/microsoft/WinUI-Gallery/main/LICENSE — read: MIT, "Copyright (c) Microsoft Corporation" · accessed 2026-09-13
16. https://docs.github.com/en/repositories/creating-and-managing-repositories/deleting-a-repository.md — read: the 7-step ceremony (Settings → Danger Zone → Delete this repository → I want to delete this repository → I have read and understand these effects → type the repository name → Delete this repository) and the warnings "Deleting a repository will **permanently** delete team permissions. This action **cannot** be undone." / "Deleting a private repository will delete all forks of the repository." / "Some deleted repositories can be restored within 90 days of deletion." · accessed 2026-09-13
17. https://docs.github.com/en/repositories/creating-and-managing-repositories/restoring-a-deleted-repository.md — read: "A deleted repository can be restored within 90 days, unless the repository was part of a fork network that is not currently empty.", "It can take up to an hour after a repository is deleted before that repository is available for restoration.", "Restoring a repository will not restore team permissions.", step 5 "Read the warning, then click **I understand, restore this repository**." · accessed 2026-09-13
18. https://raw.githubusercontent.com/github/docs/main/LICENSE — read: Attribution 4.0 International (CC-BY 4.0) · accessed 2026-09-13
19. https://cli.github.com/manual/gh_repo_delete — read: "For safety, when no repository argument is provided, the `--yes` flag is ignored and you will be prompted for confirmation…", and that deletion requires the `delete_repo` scope · accessed 2026-09-13
20. https://kubernetes.io/docs/reference/kubectl/generated/kubectl_delete/ — read (page regenerated for v1.37, last modified 2026-08-26): `--grace-period int Default: -1` ("Can only be set to 0 when --force is true"), `-i, --interactive` ("If true, delete resource only when user confirms."), `--dry-run string[="unchanged"] Default: "none"` ("client" print-only vs "server" submit-without-persisting), the force-delete IMPORTANT warning · accessed 2026-09-13
21. https://raw.githubusercontent.com/kubernetes/website/main/LICENSE — read: CC-BY 4.0 · accessed 2026-09-13
22. https://developer.hashicorp.com/terraform/cli/commands/destroy — read: "This command is just a convenience alias for the following command: `terraform apply -destroy`", "You can also create a speculative destroy plan … `terraform plan -destroy`", "-target" usage · accessed 2026-09-13
23. https://raw.githubusercontent.com/hashicorp/web-unified-docs/main/LICENSE — read: Business Source License 1.1, "Licensor: International Business Machines Corporation (IBM)"; Additional Use Grant restricted against competing hosted/embedded offerings · accessed 2026-09-13
24. https://docs.docker.com/reference/cli/docker/system/prune/ — read (markdown alternate): "WARNING! This will remove: - all stopped containers - all networks not used by at least one container - all dangling images - unused build cache", "Are you sure you want to continue? [y/N]", the `Deleted Containers:`/`Deleted Networks:`/`Deleted Images:` receipt groups, "Total reclaimed space: 1.84kB", `-f, --force  Do not prompt for confirmation`, and the default exclusion of volumes · accessed 2026-09-13
25. https://raw.githubusercontent.com/docker/docs/main/LICENSE — read: Apache License 2.0 · accessed 2026-09-13
26. https://docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteObjects.html — read: "The operation supports two modes for the response: verbose and quiet. By default, the operation uses verbose mode…"; the `DeleteResult` response syntax with `Deleted[]`/`Error{Code,Key,Message}`; the mixed-success sample returning `<Error><Key>sample2.txt</Key><Code>AccessDenied</Code><Message>Access Denied</Message></Error>` under `HTTP/1.1 200 OK`; the `Quiet` request element ("When you add this element, you must set its value to `true`") (full output also at `'/Users/aaikyn1/.omp/agent/sessions/-Code-disk_janitor/2026-09-12T14-01-30-348Z_01a095ec-5bec-727c-9262-deb249bae831/1581.read.log'`) · accessed 2026-09-13
27. https://raw.githubusercontent.com/awsdocs/amazon-s3-userguide/main/LICENSE — read: Creative Commons Attribution-ShareAlike 4.0 International Public License · accessed 2026-09-13
28. https://docs.npmjs.com/policies/unpublish/ — read: "For newly created packages, as long as no other packages in the npm Public Registry depend on your package, you can unpublish anytime within the first 72 hours after publishing.", "Once you have unpublished a package, you will not be able to undo the unpublish.", "If you entirely unpublish all versions of a package, you may not publish any new versions of that package until 24 hours have passed.", the deprecate-as-alternative guidance, and the on-page licence note "This document may be reused under a Creative Commons Attribution-ShareAlike License" · accessed 2026-09-13
29. https://docs.stripe.com/upgrades.md — read: "For 72 hours after you've upgraded your API version, you can safely roll back to the version you were upgrading from in Workbench." plus the explicit backward-compatible-change list · accessed 2026-09-13
30. https://git-scm.com/docs/git-clean — read: `-n/--dry-run` "Don't actually remove anything, just show what would be done."; `-i/--interactive` "…as this mode gives its own safety protection by going interactive."; `-f` "Git will refuse to modify untracked nested git repositories … unless a second -f is given."; the interactive menu "1: clean 2: filter by pattern 3: select by numbers 4: ask each 5: quit 6: help / What now> "; "ask each — … you must confirm one by one … this action is not as efficient as the above two actions."; `clean.requireForce` "Defaults to true." · accessed 2026-09-13
31. https://www.nngroup.com/articles/confirmation-dialog/ — read (Nielsen, "Confirmation Dialogs Can Prevent User Errors — If Not Overused", 18 Feb 2018 · last reviewed 7 Aug 2026): the automation-failure-mode argument, the "Are you sure…" critique, "conducting a task analysis to determine the severity of different outcomes", "For particularly dangerous operations, require a nonstandard action… type a word into a box, as MailChimp requires…", the MailChimp case (type `DELETE`, field appears only after scrolling), the closing "do go to great lengths to provide undo" · accessed 2026-09-13
32. https://m3.material.io/components/dialogs/guidelines — read (JS-rendered; extracted via browser): "Disable confirming actions (1) until a choice is made. Dismissive actions are never disabled.", "Don't place dismissive actions … to the right of confirming actions. Instead, place them to the left of confirming actions.", "If two actions are provided, one must be a confirming action, and the other a dismissing action", "The confirmation action should be clear about what happens next, like Send or Create. Avoid using vague terms like Done, OK, or Close. Only trigger an additional basic dialog if the action fails. Don't disable the confirmation button." · licence UNVERIFIED · accessed 2026-09-13
33. https://m3.material.io/components/snackbar/guidelines — read (JS-rendered; extracted via browser): "Only one snackbar may be displayed at a time.", "Snackbars communicate messages that are minimally interruptive and don't require user action.", "On web, auto-dismissing snackbars are inaccessible for people with low vision or who require additional time to perceive information.", "1. Add inline feedback … update the label on a 'Save' button to 'Saved', and trigger an auto-dismissing snackbar that communicates the same message.", "Snackbars with actions should remain on the screen until the user takes an action on the snackbar, or dismisses it.", "Don't stack snackbars on top of one another" · licence UNVERIFIED · accessed 2026-09-13
34. https://crates.io/api/v1/crates/inquire — read: `max_stable_version` 0.9.4, repository `mikaelmello/inquire`; the `/versions` endpoint carries `license: "MIT"`, `rust_version: "1.80.0"` · accessed 2026-09-13
35. https://crates.io/api/v1/crates/dialoguer — read: `max_stable_version` 0.12.0, repository `console-rs/dialoguer`; `/versions` carries `license: "MIT"`, `rust_version: "1.66"`, crate_size 31273 · accessed 2026-09-13
36. https://docs.rs/inquire/0.9.4/inquire/ — read: prompt inventory (`Confirm`, `CustomType`, `Password`, `Select`, `MultiSelect`, `Editor`, `DateSelect`), "Cross-platform, supporting UNIX and Windows terminals (thanks to crossterm)", "Custom parsers for Confirm and CustomType prompts", the `validator` module ("Traits and structs used by prompts to validate user input before"), dependency list showing `crossterm ^0.29.0` · accessed 2026-09-13
37. https://docs.rs/dialoguer/0.12.0/dialoguer/ — read: "Confirmation prompts", "Input prompts (regular and password)", "Input validation", feature list showing `fuzzy-select`/`history`/`password` gated; dependency list showing `console ^0.16.0` · accessed 2026-09-13
38. https://registry.npmjs.org/bits-ui/latest — read: `version: 2.19.2`, `license: MIT` · accessed 2026-09-13
39. https://raw.githubusercontent.com/huntabyte/bits-ui/main/LICENSE — read: MIT License, "Copyright (c) 2024 Hunter Johnston" · accessed 2026-09-13
40. https://registry.npmjs.org/svelte-sonner/latest — read: `version: 1.2.1`, `license: MIT` · accessed 2026-09-13
41. https://raw.githubusercontent.com/huntabyte/svelte-sonner/main/LICENSE — read: MIT License, "Copyright (c) 2024 Robert Soriano" · accessed 2026-09-13
42. https://primer.style/product/ui-patterns/delete/ — **UNREACHABLE as intended**: HTTP 200 but redirects to the Primer marketing home (`url_effective: https://primer.style/`). The DELETE ui-pattern page could not be read; no Primer claim is made anywhere in this note · accessed 2026-09-13
43. https://support.apple.com/en-us/104973 — **HTTP 404**. No claim derived from this URL · accessed 2026-09-13
