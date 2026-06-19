---
parent_roadmap: ./_roadmap.md
plan_id: 02-idle-screensaver-grill
status: grilled
planning_maturity: current-wave
depends_on: [01-movement-substrate]
unblocks: [03-composer-curiosity-grill]
decision_log: ./grill-logs/2026-06-18-idle-pet-screensaver.md
---

# Grill 02: Idle Screensaver Behavior

## Resolved Decision

The roadmap needed product rules for when the pet may leave home during idle
time and what counts as acceptable idle behavior.

The accepted Plan 02 scope is narrow sparse lane-patrol only. Advanced
transcript-side screensaver behavior remains deferred to a separate future
grill/plan.

## Why Code Cannot Answer This

Code can report layout facts, but it cannot decide how distracting the pet may
be, whether movement should be opt-in, or how playful the idle behavior should
feel.

## Why Prior Context Did Not Settle This

The flicker plan established render safety workarounds, but it intentionally
left real movement behavior out of scope.

## Why It Matters

Idle behavior defines collision rules, timing, animation intensity, and the
acceptance criteria for "not annoying".

## Recommended First Grill Question

Should idle movement be opt-in debug/local-fork behavior at first, or enabled
automatically whenever pets are enabled?

## Recommended Default

Start opt-in. Enable automatically only after manual evidence shows it is calm
and does not distract from reading.

## Promotion Trigger

Promote through `plan-creation` when Vladimir resumes implementation. Do not
start implementation directly from this grill note.
