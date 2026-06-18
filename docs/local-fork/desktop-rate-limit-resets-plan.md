# Planned Feature: Desktop Rate Limit Resets

Status: parked until the `rust-v0.141.0` branch is stable.

This file tracks a future local-fork feature near the other local planning
artifacts. It is not part of the TUI pet movement roadmap. Before implementation,
run `grill-me` and/or a focused plan if the product/API surface is still unclear.

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
- The first `Reset usage` action must only open a confirmation modal. It must
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
  activation, and similar control keys for the modal. This protects terminals
  that occasionally duplicate control-key events.
- No `account/rateLimitResetCredit/consume` request may be sent until after the
  500 ms guard has elapsed and the user explicitly confirms.

## Planned Local Work

- Keep `idempotencyKey` mapped to upstream `redeem_request_id`.
- Add optional `creditId` at the app-server boundary if upstream still expects
  selected reset-card ids.
- Forward `credit_id` only when a caller provides one.
- Render the reset affordance in `/status`, not in the composer.
- Preserve the official Desktop wording above in any local UI or docs.
- Refetch/read account rate limits after a successful or idempotent redemption.

## Safety Rules

- Automated tests must use mock app-server/backend fixtures only.
- Do not run development or smoke tests that post to a real ChatGPT/OpenAI
  backend with live auth.
- Do not add a manual verification step that consumes a real reset by default.
- Any live redemption must be a separate, explicit manual action from Vladimir.

## Promotion Trigger

After the `rust-v0.141.0` branch is merged and clean, promote this note through
`grill-me` if the UX or safety contract needs product decisions; otherwise create
a focused implementation plan and develop against mocks first.
