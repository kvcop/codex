# План изменений: безопасные reset credits в `/status`

> **How to read this plan:** review `Decision Brief` and `Execution Contract`.
> `Machine Appendix` is for executor/reviewer context and can be skipped unless
> you want the deep trace.
>
> **Plan freeze:** after approval or implementation start, this file is
> read-only except for `phased-execution` Drift Alert amendments or explicit
> user-requested amendments. Do not edit it retroactively to make the
> implementation fit the plan.

## Decision Brief

**Goal:** добавить в TUI безопасное использование накопленных Desktop-style
rate-limit resets из `/status`, без риска случайно потратить reset во время
разработки, тестов или дребезга клавиш в терминале.

**Non-negotiable constraints:**
- Reset action lives in `/status`, next to ChatGPT usage/rate-limit information.
- Because `/status` is transcript history output rather than a clickable
  surface, `/status` must render reset state and a hint to run explicit
  `/reset-usage`. The consume flow starts from `/reset-usage`, not from
  clickable history text.
- If the eligible limit has more than 35% remaining, reset is disabled entirely.
- If 1%..35% remains, confirmation must include the remaining percentage.
- If 1% or less remains, confirmation may use the shorter "Reset usage now?"
  wording.
- The first `/reset-usage` activation only opens confirmation; it must never
  call the backend directly.
- Confirmation defaults to `Cancel`.
- The confirmation modal ignores confirm/cancel/navigation control keys for
  500 ms after opening, including `Enter`, `Esc`, `Tab`, `Backspace`, mouse
  activation, and similar control keys.
- No real ChatGPT/OpenAI backend consume call is allowed in automated tests.

**User decisions needed:**
- None before implementation. Default decision: `/reset-usage` is the explicit
  action trigger because `/status` is not an interactive screen. The 35%
  hard-disable threshold is intentionally reversible if it feels too strict
  after manual testing.

**Known architectural risks:**
- `/status` currently renders as a transcript history cell, not a naturally
  clickable screen. Implementation must add a deliberate TUI action path around
  the status surface instead of pretending history text is clickable.
- `rateLimitResetCredits` is present in `account/rateLimits/read`, but current
  TUI rate-limit fetch plumbing drops it by returning only snapshots.
- The app-server consume API currently carries only `idempotencyKey`; upstream
  Desktop can send a selected `credit_id`, so the API boundary should accept an
  optional `creditId`.

**Design calls you must notice:**
- Prefer an explicit row/action in `/status`: available reset count plus either
  `/reset-usage`, disabled copy, or an explanatory unavailable state.
- Treat the 35% rule as client-side safety. The backend may still return
  `nothingToReset`; surface that as a normal result, not as a bug.
- After successful or idempotent redemption, refetch `account/rateLimits/read`
  instead of inferring new limits locally.

**Phase outcomes:**
- Phase 1: app-server/backend data plumbing keeps optional selected credit id.
- Phase 2: TUI rate-limit plumbing preserves reset-credit count, `/status`
  renders the safe affordance, and `/reset-usage` owns confirmation with the
  35% lockout and 500 ms guard.
- Phase 3: tests, docs, final review, and debug package build.

**Assumptions:**
- The TUI may consume without `creditId` when there is no selected card id, but
  protocol/backend types must allow `creditId` so card-specific clients stay
  compatible with upstream Desktop semantics.
- The eligible display limit for the safety threshold is the primary Codex
  usage window unless implementation evidence proves a different app-server
  field is the only correct target. If that assumption is wrong, raise a Drift
  Alert before coding around it.

**Non-goals:**
- No release build in this plan. Build debug first, then decide on release.
- No live manual redemption during implementation.
- No `usage daily` Monday-Sunday ordering work in this execution; it remains
  tracked in `FORK_NOTES.md`.

## Execution Contract

### Текущее состояние
- **Git baseline:** `fix/md-tables-rust-v0.141.0-new` is clean at
  `ee7385d985 Document planned rate limit reset work`.
- **Ключевые файлы:**
  - `codex-rs/app-server-protocol/src/protocol/v2/account.rs`
  - `codex-rs/backend-client/src/client/rate_limit_resets.rs`
  - `codex-rs/app-server/src/request_processors/account_processor/rate_limit_resets.rs`
  - `codex-rs/tui/src/app/background_requests.rs`
  - `codex-rs/tui/src/app_event.rs`
  - `codex-rs/tui/src/app/event_dispatch.rs`
  - `codex-rs/tui/src/chatwidget/status_controls.rs`
  - `codex-rs/tui/src/status/card.rs`
  - `codex-rs/tui/src/status/rate_limits.rs`
  - `codex-rs/tui/src/chatwidget/tests/status_command_tests.rs`
  - `codex-rs/tui/src/status/tests.rs`
- **Паттерны проекта:**
  - `/status` creates a `StatusHistoryCell` and refreshes rate limits through
    `AppEvent::RefreshRateLimits`.
  - Background TUI requests return through `AppEvent` so UI mutation stays on
    the main loop.
  - UI-visible TUI changes need focused tests and `insta` snapshots when rendered
    output changes.
  - Run commands from the repository root; the repo `justfile` sets
    `working-directory := "codex-rs"` internally.

### Phase 1: API and backend parity
**Goal:** preserve optional selected credit id through app-server/backend
boundaries.

**Inputs:** existing `account/rateLimits/read`, `account/rateLimitResetCredit/consume`,
upstream Desktop body `{ credit_id, redeem_request_id }`.

**Outputs:**
- `ConsumeAccountRateLimitResetCreditParams` has optional `creditId`:
  `#[ts(optional = nullable)] pub credit_id: Option<String>`.
- Backend consume request serializes `credit_id` only when present.
- App-server forwards `creditId` to the backend client.

**Invariants:**
- `idempotencyKey` still maps to `redeem_request_id`.
- Empty `idempotencyKey` remains invalid.
- Omitted `creditId` remains valid.
- No live backend calls in tests.

**Acceptance criteria:**
- [ ] Backend client tests cover consume payload with and without `credit_id`.
- [ ] App-server tests cover JSON-RPC `creditId` forwarding.
- [ ] Schema fixtures are regenerated if protocol shape changes.
- [ ] Existing idempotency-only callers still compile and behave the same.

**Validation commands:**
- `just write-app-server-schema`
- `just test -p codex-backend-client`
- `just test -p codex-app-server-protocol`
- `just test -p codex-app-server`

**Rollback:** revert Phase 1 commit; app-server returns to idempotency-only
consume and TUI drops reset-credit count.

**Forbidden shortcuts:**
- Do not change backend endpoint paths.
- Do not require `creditId`; omitted `creditId` remains a supported compact path.

**Files likely touched:** app-server protocol/schema, backend-client,
app-server processor/tests.

### Phase 2: TUI `/status` state, `/reset-usage`, and guarded confirmation
**Goal:** expose safe reset usage from `/status` plus explicit `/reset-usage`
with 35% lockout, confirmation, default cancel, and 500 ms input guard.

**Inputs:** Phase 1 reset-credit summary and current primary Codex rate-limit
display snapshot.

**Outputs:**
- TUI rate-limit refresh result carries both snapshots and
  `RateLimitResetCreditsSummary`.
- ChatWidget/status state remembers available reset count.
- `/status` shows available reset count when known.
- `/status` shows `Run /reset-usage` when reset is eligible and explanatory
  disabled/unavailable copy otherwise.
- Add `SlashCommand::ResetUsage` for `/reset-usage`.
- Reset action is available only when `availableCount > 0` and eligible
  remaining percentage is `<= 35` from `StatusRateLimitData::Available`.
- `>35%` remaining renders disabled/explanatory copy and cannot send consume.
- Stale, missing, or unavailable rate-limit data disables reset and does not
  send consume.
- Confirmation modal opens before any consume request.
- Modal ignores control-key/mouse activation for 500 ms after opening.
- Confirmed reset sends `account/rateLimitResetCredit/consume`, then refetches
  `account/rateLimits/read`.
- Add a TUI background consume helper and AppEvent result path.

**Invariants:**
- First activation from `/status` never calls consume.
- Default focused action is `Cancel`.
- Repeated/duplicated `Enter`, `Esc`, `Tab`, or `Backspace` immediately after
  opening cannot confirm or cancel the modal. Guarded keys are dropped/consumed,
  not queued for replay after the guard elapses.
- Consume uses a fresh idempotency key per confirmed logical attempt.
- If a confirmed logical attempt times out and the user retries that same
  attempt from the still-visible error state, reuse the same idempotency key.
- `reset` and `alreadyRedeemed` are success-like outcomes for refetch and user
  feedback.
- Use an injectable/testable clock seam such as `opened_at: Instant` plus
  `handle_key_event_at(key, now)` for guard tests. Do not use real sleeps.

**Outcome text and refresh behavior:**
- `Reset`: show `Usage reset. You have {# left}` and refetch rate limits.
- `AlreadyRedeemed`: show `Usage reset. You have {# left}` and refetch rate
  limits, because the logical attempt already completed.
- `NothingToReset`: show `Your usage does not need a reset right now`; refetch
  rate limits so `/status` is current.
- `NoCredit`: show `No resets are available`; refetch rate limits so the reset
  count is current.
- Transport/internal error: show `Couldn’t reset usage. Please try again`; keep
  retry tied to the same idempotency key while the same confirmation/error
  surface remains open.

**Acceptance criteria:**
- [ ] Snapshot coverage shows reset-count row and disabled `>35%` state.
- [ ] Tests prove `/status` never emits consume and only renders reset state plus
  `/reset-usage` guidance.
- [ ] Tests prove `/reset-usage` refuses stale/missing/unavailable data and
  `>35%` remaining without opening a consume-capable path.
- [ ] Tests prove guard blocks control keys before 500 ms and allows intended
  confirmation after 500 ms.
- [ ] Tests cover confirmation copy for `>1%` and `<=1%`.
- [ ] Tests cover no reset action when no credits are available or summary is
  unknown.
- [ ] Consume completion refetches rate limits and surfaces every backend
  outcome above.

**Validation commands:**
- `just test -p codex-tui`
- `cargo insta pending-snapshots -p codex-tui`
- `cargo insta accept -p codex-tui` only after reviewing intended snapshots.

**Rollback:** revert Phase 2 commit; Phase 1 API support can remain harmless if
needed, or be reverted separately.

**Forbidden shortcuts:**
- Do not make transcript history text itself clickable.
- Do not put reset in the composer as the primary surface.
- Do not allow keyboard shortcuts to bypass the modal.
- Do not consume a reset while rate-limit data is stale/unknown unless a future
  explicit product decision says otherwise.
- Do not route `/reset-usage` directly to consume; it must go through the
  guarded confirmation.

**Files likely touched:** TUI status/rate-limit modules, app event dispatch,
bottom pane selection/confirmation view or a small dedicated reset confirmation
view, TUI tests/snapshots.

### Phase 3: docs, final checks, debug package
**Goal:** finish verification, record behavior, run external/final review, and
produce a debug build for manual testing.

**Inputs:** Phase 1-2 commits.

**Outputs:**
- `FORK_NOTES.md` and this plan remain accurate.
- Phased execution audit trail records checks, review artifacts, commits, and
  final plan checksum.
- Debug binary is built for manual testing.

**Invariants:**
- No release build unless Vladimir asks after debug acceptance.
- No live reset redemption during build/test.

**Acceptance criteria:**
- [ ] `just fmt` passes after code changes.
- [ ] `just fix -p codex-tui` is run before finalizing TUI changes.
- [ ] `cargo build -p codex-cli` produces `codex-rs/target/debug/codex`.
- [ ] Final Claude review or local reviewer-shaped fallback approves the full
  diff.
- [ ] `git status --short` is clean after phase commits.

**Validation commands:**
- `just fmt`
- `just fix -p codex-tui`
- `just test -p codex-tui`
- `cargo build -p codex-cli`

**Rollback:** revert Phase 3 docs/check commits if any; debug build artifacts
under `target/` are untracked.

**Forbidden shortcuts:**
- Do not skip snapshot review when `/status` output changes.
- Do not claim manual reset verification unless a real live redemption was
  explicitly requested and performed by Vladimir.

**Files likely touched:** `FORK_NOTES.md`, this plan, `.tmp/current/phased-execution-log.md`,
possibly no source files beyond prior phases.

### Миграции и совместимость
- **State-файлы:** none.
- **БД/Alembic:** none.
- **API/CLI/UI compatibility:** app-server v2 gains an optional request field;
  existing clients remain compatible. Generated schema fixtures must match.

### Логи и наблюдаемость
- Add tracing only if consume/refetch error handling needs diagnostics; avoid
  logging account secrets or reset credit ids beyond ordinary request context.

### Тестирование
- Unit: backend payload serialization, rate-limit/reset display helpers, modal
  guard timing.
- Integration/e2e: app-server consume request mapping, TUI status command/event
  tests.
- Snapshot: `/status` rows and confirmation/disabled states where rendered.
- Manual/smoke: run debug binary, open `/status`, verify disabled/action states
  without confirming a live reset.

### Out of scope but tracked
- `usage daily` should render Monday as first row and Sunday as last row; tracked
  in `FORK_NOTES.md`.
- Pet animation/roadmap work remains separate.
- Release build after debug acceptance.

### Риски и откат
- Main risk: accidental real reset consumption. Mitigations are mock-only tests,
  disabled state above 35%, explicit `/reset-usage` trigger, confirmation only,
  default cancel, 500 ms guard, and no live consume smoke.
- If eligible limit selection is ambiguous, raise Drift Alert before shipping.
- If TUI action plumbing becomes too invasive, prefer a bounded dedicated
  `/status` reset confirmation surface over broad bottom-pane refactors.

### Execution
- **Recommended mode:** `phased-execution`.
- **Why:** touches app-server protocol/backend, TUI event plumbing, rendered UI,
  tests, docs, and debug build.
- **Commit policy:** commit the plan after Claude validation; then commit each
  approved implementation phase separately.
- **Plan freeze starts:** after Claude validation is integrated and phased
  execution starts.

### Открытые вопросы
- None blocking. Default: use primary Codex usage window as the eligible limit
  for the 35% threshold; Drift Alert if source evidence contradicts it.

## Machine Appendix

- Prior source inspection found the official Desktop webview uses:
  `GET /wham/rate-limit-reset-credits` and
  `POST /wham/rate-limit-reset-credits/consume`.
- Official consume body observed:
  `{ "credit_id": <selected credit id>, "redeem_request_id": <uuid> }`.
- Official outcome codes observed:
  `reset`, `already_redeemed`, `no_credit`, `nothing_to_reset`.
- Current TUI `fetch_account_rate_limits` returns only
  `Vec<RateLimitSnapshot>`, dropping `rate_limit_reset_credits`.
- Current `/status` card is a `HistoryCell`; interactive reset must be routed
  through ChatWidget/AppEvent/bottom-pane action plumbing, not through clickable
  transcript text.
- Claude plan validation (`.tmp/current/claude-rate-limit-reset-plan-review.md`)
  required a concrete trigger. Default accepted here: `/status` displays state;
  `/reset-usage` opens the guarded confirmation surface.
- Relevant committed setup note:
  `ee7385d985 Document planned rate limit reset work`.

## Source Snapshot

- Official Desktop DMG: `Codex-26.611.62324-arm64`.
- `app.asar` build date: 2026-06-17.
- DMG SHA-256:
  `31d8e2666a0895a830df0832dc4083ae82a6e9bd26603141c0293acea6618211`.
- Inspection mode: static extraction/grep only. No account API calls were made.

## Upstream Desktop Shape

Observed webview endpoints:

- `GET /wham/rate-limit-reset-credits`
- `POST /wham/rate-limit-reset-credits/consume`

Observed consume body:

```json
{
  "credit_id": "<selected credit id>",
  "redeem_request_id": "<uuid>"
}
```

The compact prompt path may omit `credit_id`, but the card-stack modal passes
the selected reset card id when the user chooses a concrete card.

Observed backend response codes:

- `reset`
- `already_redeemed`
- `no_credit`
- `nothing_to_reset`

## Wording To Match

- Banner CTA: `Reset usage`
- Modal CTA: `Reset rate limit`
- Summary/menu row: `{# reset available}` / `{# resets available}`
- Success toast: `Usage reset. You have {# left}`
- Error text:
  - `This reset was already used`
  - `No resets are available`
  - `Your usage does not need a reset right now`
  - `Couldn’t reset usage. Please try again`
  - `Couldn’t load rate limit resets. Please try again`

## TUI Placement And Confirmation

- Primary placement: `/status`, next to the existing ChatGPT usage/rate-limit
  information.
- Show the available earned-reset count when `account/rateLimits/read` includes
  `rateLimitResetCredits`.
- `/status` should show `Run /reset-usage` when reset is eligible. `/status`
  history text is not clickable and must never call the backend directly.
- The first `/reset-usage` action must only open a confirmation modal. It must
  never call the backend directly.
- Confirmation copy:
  - if more than 1% of the eligible limit remains:
    `You still have {#}% of your limit left. Reset usage anyway?`
  - if 1% or less remains: `Reset usage now?`
  - always include: `This will consume 1 rate limit reset.`
- Default confirmation focus: `Cancel`, not `Reset usage`.
- After the confirmation modal opens, arm its controls for 500 ms before any
  confirm/cancel/navigation control key can take effect. During that guard
  window, ignore activation from `Enter`, `Esc`, `Tab`, `Backspace`, mouse
  activation, and similar control keys for the modal. Drop guarded input instead
  of queueing it. This protects terminals that occasionally duplicate
  control-key events.
- No `account/rateLimitResetCredit/consume` request may be sent until after the
  500 ms guard has elapsed and the user explicitly confirms.

## Planned Local Work

- Keep `idempotencyKey` mapped to upstream `redeem_request_id`.
- Add optional `creditId` at the app-server boundary if upstream still expects
  selected reset-card ids.
- Forward `credit_id` only when a caller provides one.
- Render reset state and `/reset-usage` guidance in `/status`, not in the
  composer.
- Preserve the official Desktop wording above in any local UI or docs.
- Refetch/read account rate limits after a successful or idempotent redemption.

## Safety Rules

- Automated tests must use mock app-server/backend fixtures only.
- Do not run development or smoke tests that post to a real ChatGPT/OpenAI
  backend with live auth.
- Do not add a manual verification step that consumes a real reset by default.
- Any live redemption must be a separate, explicit manual action from Vladimir.

## Promotion Trigger

Promoted into the executable plan above after the `rust-v0.141.0` branch became
clean. Develop through `phased-execution` against mocks first.
