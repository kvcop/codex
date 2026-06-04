---
parent_roadmap: ./_roadmap.md
plan_id: 01-movement-substrate
status: pending
planning_maturity: current-wave
depends_on: []
unblocks: [02-idle-screensaver-grill, 06-terminal-compat-validation]
---

# Plan 01: Movement Substrate

## Scope

- Add a bounded movement state layer for ambient pets.
- Keep existing default visual behavior unless an explicit local/debug movement
  mode is enabled.
- Support a simple home-to-target-to-home manual/debug path so movement can be
  tested before behavior decisions.
- Limit first-wave debug targets to the already reserved right-side pet lane
  between composer-bottom and screen-bottom anchors, or to cells explicitly
  proven Codex-owned and empty by the implementation plan.
- Preserve current Kitty/tmux flicker mitigations and Sixel behavior.

## Control Surface

- **Outcome:** Codex has safe primitives for pet movement: home anchor, target
  anchor, current rect, tick scheduling, and return-home behavior.
- **Boundaries:** no idle screensaver, no composer curiosity, no letter
  interaction, no stable Kitty placement caching, no arbitrary transcript
  targets, and no Sixel movement in the first wave.
- **Risk triggers:** movement worsens flicker, requires unbounded frame
  scheduling, or needs product decisions about where the pet may go.
- **Handoff decisions:** if movement needs visible behavior choices, stop at
  `propose_grill:02-idle-screensaver-grill`.

## Acceptance

- Movement state is tested independently from terminal image protocol payloads.
- Ambient pet draw can still produce the exact current home placement when
  movement is disabled.
- A debug/manual movement mode can move the pet through a bounded target and
  return it home without changing public CLI/config contracts.
- Debug/manual targets stay inside the right-side pet lane or an explicitly
  verified Codex-owned empty cell region.
- Sixel keeps current home-only behavior when debug movement is enabled.
- Frame scheduling is bounded and respects disabled animations.
- Existing pet image protocol tests continue to pass.
- Debug build succeeds for manual Kitty/tmux validation.

## Notes for `plan-creation`

- Use `docs/local-fork/pet-movement-roadmap/_roadmap.md` as parent roadmap.
- Recommended mode: `phased-execution`.
- Suggested phases:
  1. Movement model and unit tests.
  2. Wire model into ambient pet draw while preserving default behavior.
  3. Add hidden local debug movement trigger and manual validation docs/audit.
- Explicitly handle interaction between movement ticks and existing pet
  animation/notification ticks. Notification state changes reset pet animation
  timing and must not leave movement state inconsistent.
- Verify the right-side pet lane row-by-row, including footer/status/bottom-pane
  rows. If a target row is not clearly Codex-owned and empty, reject that target
  rather than drawing over it.
- Validate the plan with Claude before execution.
