# Grill Log: Idle Pet Screensaver

**Created:** 2026-06-18 12:31
**Mode:** tracked-durable
**Repo/workdir:** /home/user/code/mine/rust/codex
**Status:** active
**Promotion target:** docs/local-fork/pet-movement-roadmap/REFERENCE.md
**Collision suffix:** none

## Objective
- Decide the product and safety rules for idle pet screensaver movement before
  creating an executable Plan 02.

## Context Checked
| Source | Finding |
|---|---|
| `docs/local-fork/pet-movement-roadmap/_roadmap.md` | Next action is `propose_grill:02-idle-screensaver-grill`; behavior-heavy idle/composer movement needs `grill-me` before implementation. |
| `docs/local-fork/pet-movement-roadmap/02-idle-screensaver-grill.md` | First unresolved decision is whether idle movement starts opt-in or automatically whenever pets are enabled. |
| `FORK_NOTES.md` | Current `lane-patrol` is accepted only as a test baseline; returning home when text enters the lane can still teleport and is debug-only. |
| User answer, 2026-06-18 | This is for a personal fork, so playful idle movement can default on as long as there is a flag/settings switch to disable it. |
| User answer, 2026-06-18 | The disable control should exist in both config and `/pets` UI. |
| User answer, 2026-06-18 | Accepted the recommended idle timing model: sparse normal cadence, accelerated debug cadence, and reset-on-user-activity or lane collision. |

## Question Log
### Q1 - Idle Movement Rollout Mode
**Why this matters:** This decides default user surprise level, acceptance
criteria, and whether Plan 02 needs config/UI controls before behavior polish.
**Recommended answer:** Start opt-in behind a local config/env flag, then enable
automatically only after manual evidence shows the behavior is calm and does not
distract from reading.
**User answer:** Since this is a personal fork, default-on is acceptable. Add a
disable flag, preferably surfaced in pet settings. If the feature is only lane
patrol, it must not constantly wander: the pet should approach, stop, wait, go
back, and then stay idle again. A rough cadence is one walk about every five
minutes, then a one-to-two-minute pause before the return. For debugging, allow
removing the delay so movement correctness is visible quickly. If the feature
becomes a more advanced screensaver, that should also default on.
**Decision:** accepted, revised from the recommended answer
**Class if unresolved:** none

### Q2 - Idle Movement Control Surface
**Why this matters:** The disable switch can be config-only, `/pets` UI-only, or
both. This affects config schema, settings persistence, `/pets` picker scope,
and tests.
**Recommended answer:** Add both layers: a stable config key such as
`tui.pet_idle_movement = false` and a `/pets` UI toggle. Default the setting to
`true` in this personal fork.
**User answer:** Yes.
**Decision:** accepted
**Class if unresolved:** none

### Q3 - Idle Timing Model
**Why this matters:** Plan 02 needs concrete timing defaults so movement does
not become constant visual noise, and debug builds still need fast manual
feedback.
**Recommended answer:** Normal mode starts a walk after about five minutes of
idle, walks to the target, waits 60-120 seconds, returns home, then waits
another five minutes before trying again. Debug mode keeps the same phase model
but accelerates it, for example with a `fast` mode or about `0.02x` timing so
five minutes becomes roughly six seconds and a 60-120 second pause becomes
roughly 1-2 seconds. Any user action or rendered text entering the movement
lane sends the pet home and resets the idle timer.
**User answer:** Try that.
**Decision:** accepted
**Class if unresolved:** none

## Accepted Decisions
1. **Decision:** Idle screensaver movement defaults on in this personal fork,
   but must have an explicit disable switch.
   **Rationale:** The fork is tuned for Vladimir's own Codex experience, so
   default-on delight is acceptable, but it needs a quick escape hatch if it
   becomes distracting.
   **Evidence:** User answer on 2026-06-18.
2. **Decision:** First lane-patrol behavior must be sparse, not continuous.
   **Rationale:** Constant movement would be annoying. The pet should travel,
   pause at the visited location, return, then stay idle for a long interval.
   **Evidence:** User answer on 2026-06-18.
3. **Decision:** Debug mode may shorten or remove idle delays.
   **Rationale:** Manual validation needs fast feedback on route correctness
   without waiting five minutes per cycle.
   **Evidence:** User answer on 2026-06-18.
4. **Decision:** The idle movement disable control should exist both in config
   and in the `/pets` UI.
   **Rationale:** Config gives a stable durable switch, while `/pets` makes the
   feature easy to turn off if the default-on behavior gets distracting.
   **Evidence:** User answer on 2026-06-18.
5. **Decision:** Idle movement uses a sparse phase cadence in normal mode and a
   fast equivalent for debug/manual validation.
   **Rationale:** The normal behavior should feel like occasional life, not a
   looping distraction. The debug behavior should preserve the same logic while
   making route/timing bugs visible quickly.
   **Evidence:** User answer on 2026-06-18.
6. **Decision:** User activity or lane collision sends the pet home and resets
   the idle timer.
   **Rationale:** Movement must yield immediately to reading and interaction,
   and should not keep trying to occupy a lane once text enters it.
   **Evidence:** User answer on 2026-06-18.

## Open Items
| Item | Class | Why It Matters | Proposed Next Step |
|---|---|---|---|
| Advanced screensaver scope | future-note | Default-on applies if promoted, but advanced behavior may need more safety rules than lane patrol. | Keep out of Plan 02 unless explicitly promoted. |

## Promotion Signals
- Existing roadmap already classifies idle screensaver behavior as current-wave
  `requires_grill`.
- After accepted grill output, promote to Plan 02 through `plan-creation`.
- Do not start implementation immediately after this grill; another agent is
  working on reset-limit docs/features, and this session should only discuss
  and record decisions.

## Closing Summary
- pending
