---
status: active
source_context: ./REFERENCE.md
---

# Roadmap: TUI Pet Movement

## Current State / Next Action

**Roadmap status:** active
**Current wave:** Wave 1
**Program max depth:** 2
**Next action:** run_plan_creation:01-movement-substrate
**Why:** movement needs safe technical primitives before product behavior can
be grilled or implemented. The first item is bounded and does not decide idle or
composer personality.
**Do not:** implement `requires_grill`, `future-note`, or `refactor-gate` items
directly. For `validation-note`, run only concrete checks when `Next action` is
`run_validation:<note_id>`; otherwise promote first.

## Target Outcome

TUI pets can move around Codex-owned visible terminal areas without corrupting
text, fighting composer/input redraws, or making Kitty/tmux flicker materially
worse. Movement should feel playful, but the technical contract is conservative:
Codex only moves the pet through owned anchors, bounded animation ticks, and
explicitly classified safe rectangles.

## Why Roadmap

This is broader than one implementation plan:

- movement primitives, idle behavior, composer behavior, and future text
  interactions have different risks;
- tmux/Kitty rendering evidence can change later choices;
- several behavior questions need `grill-me` before they become executable;
- cached Kitty placement may become necessary later, but previous evidence says
  it is unsafe to re-enable blindly.

## Production Cutoff / Scenario Matrix

| Scenario / Requirement | Horizon | Classification | Owned By | Done Means | Notes |
|---|---|---|---|---|---|
| Pet can move between home and a bounded target without touching behavior UX | MVP | must before behavior work | Plan 01 | Movement state, anchors, tick scheduling, and tests exist | Debug targets stay inside the right-side pet lane or explicitly owned empty cells |
| Movement does not worsen known tmux/Kitty flicker beyond accepted residual | MVP | must before manual acceptance | Plan 01 | Debug build accepted in Kitty/tmux | Keep Phase 3 disabled |
| First-wave movement does not regress Sixel | MVP | must before manual acceptance | Plan 01 | Sixel keeps current home-only behavior unless separately validated | Kitty-first debug movement |
| Idle pet can visit blank transcript-side space | production | requires product decision | Grill 02 then Plan 02 | Rules for idle time, safe rectangles, and exit conditions are accepted | Requires `grill-me` |
| Pet can visit composer text while user types | production | requires product decision | Grill 03 then Plan 03 | Rules for multiline growth, submit escape, and typing interference are accepted | Requires `grill-me` |
| Letter pickup/restore animation | hardening/scale | future-note | Future 04 | Separate text restoration contract exists | Not executable now |
| Cached Kitty placement lifecycle | hardening/scale | refactor-gate | Gate 05 | Evidence shows retransmit path is insufficient and tmux lifecycle strategy exists | Prior Phase 3 failed |
| Cross-terminal fallback matrix | hardening | validation-note | Validation 06 | Kitty/tmux, plain Kitty, SSH/mobile fallback, Sixel are checked | Promote if validation finds blocker |

## Planning Maturity

| Item | Status | Horizon | Depends On | Promotion / Close Trigger |
|---|---|---|---|---|
| [01-movement-substrate](01-movement-substrate.md) | pending | current-wave | none | Run `plan-creation` before execution |
| [02-idle-screensaver-grill](02-idle-screensaver-grill.md) | requires_grill | current-wave | 01 | `propose_grill:02-idle-screensaver-grill`; promote after accepted grill output |
| [03-composer-curiosity-grill](03-composer-curiosity-grill.md) | requires_grill | future-wave | 01, 02 evidence | Promote after idle movement evidence or explicit priority override |
| [04-letter-interactions](04-letter-interactions.md) | future-note | far-horizon | 01, 03 | Promote when text restoration can be specified safely |
| [05-kitty-placement-lifecycle](05-kitty-placement-lifecycle.md) | refactor-gate | gated | 01, 06 | Promote only if retransmit movement is visibly insufficient |
| [06-terminal-compat-validation](06-terminal-compat-validation.md) | validation-note | evidence | 01 | Run after first movement debug build, or promote if manual checks expose a blocker |

## Current Wave

Wave 1 intentionally has one executable plan: `01-movement-substrate` gates all
behavior plans and terminal compatibility evidence.

| Plan | Status | Depends On | Commit / Branch | Done Means |
|---|---|---|---|---|
| [01-movement-substrate](01-movement-substrate.md) | pending | none | - | Movement primitives and debug-manual path are accepted |
| [02-idle-screensaver-grill](02-idle-screensaver-grill.md) | requires_grill | 01 | - | Grill decisions exist and Plan 02 can be created |
| [06-terminal-compat-validation](06-terminal-compat-validation.md) | validation-note | 01 | - | Compatibility notes are updated after movement debug evidence |

## Future Horizon

| Note | Status | Depends On | Why Deferred |
|---|---|---|---|
| [03-composer-curiosity-grill](03-composer-curiosity-grill.md) | requires_grill | 01, 02 evidence | Composer behavior has more product and layout edge cases than idle movement |
| [04-letter-interactions](04-letter-interactions.md) | future-note | 01, 03 | Text mutation/restoration is too risky before movement safety is proven |
| [05-kitty-placement-lifecycle](05-kitty-placement-lifecycle.md) | refactor-gate | 01, 06 | Prior cached placement attempt failed in tmux tab switching |

## Dependency Graph

```text
Plan 01: movement substrate
├── Grill 02 -> Plan 02: idle screensaver
│   └── Grill 03 -> Plan 03: composer curiosity
│       └── Future 04: letter interactions
├── Validation 06: terminal compatibility
└── Gate 05: Kitty placement lifecycle, only if evidence requires it
```

## Risk Triggers

- Movement requires per-frame image retransmits so often that flicker regresses
  or CPU/terminal load becomes noticeable.
- A behavior plan wants to draw over non-owned scrollback or unknown terminal
  content.
- First-wave movement attempts to animate Sixel without a separate validation
  gate.
- Composer curiosity needs hidden product decisions about whether pets may
  distract from prompt entry.
- A fix reintroduces stable Kitty placement (`a=p`) without resolving the tmux
  lifecycle problem observed in the flicker plan.
- Sixel behavior regresses while optimizing Kitty movement.

## Decisions Log

- 2026-06-05: Start with roadmap, not one large plan, because the end-state is
  multi-wave and behavior-heavy.
- 2026-06-05: Make `grill-me` mandatory before idle/composer/letter behavior
  plans, but not before the first movement substrate plan.
- 2026-06-05: Keep cached Kitty placement disabled until a refactor gate is
  promoted by evidence.
- 2026-06-05: Plan 01 debug movement is Kitty-first; Sixel remains home-only
  unless Validation 06 later promotes Sixel movement.
- 2026-06-05: After Plan 01 acceptance, run Validation 06 before
  `propose_grill:02-idle-screensaver-grill` so behavior work starts from real
  movement evidence.
- 2026-06-05: The accepted residual flicker note from `FORK_NOTES.md` is
  revisited through Plan 01 manual validation and Validation 06.

## Resume Instructions

1. Read `Current State / Next Action` first and follow its `Next action`.
2. Read `REFERENCE.md` for source facts before creating or executing subplans.
3. Do not implement `requires_grill`, `future-note`, or `refactor-gate` items
   directly. For `validation-note`, run only concrete checks when `Next action`
   is `run_validation:<note_id>`.
4. After any subplan execution, update status, commits, evidence, decisions, and
   the next action.
5. If the current wave completes but behavior work remains, refresh this
   roadmap instead of extending an approved executable plan.
