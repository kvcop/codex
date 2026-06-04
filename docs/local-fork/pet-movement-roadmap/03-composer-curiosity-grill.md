---
parent_roadmap: ./_roadmap.md
plan_id: 03-composer-curiosity-grill
status: requires_grill
planning_maturity: future-wave
depends_on: [01-movement-substrate, 02-idle-screensaver-grill]
unblocks: [04-letter-interactions]
---

# Grill 03: Composer Curiosity

## Unresolved Decision

The roadmap needs rules for how close the pet may get to user input while the
prompt is being written.

## Why Code Cannot Answer This

Composer layout can expose wrapped lines and cursor position, but code cannot
decide whether motion near the prompt is delightful, distracting, or too risky.

## Why Prior Context Did Not Settle This

The user idea described the desired vibe, but no accepted plan has decided
whether pets may approach, cover, or distract from active prompt text.

## Why It Matters

Composer curiosity can fight input redraws, multiline growth, submit handling,
and user attention. It needs explicit behavior limits before implementation.

## Recommended First Grill Question

May the pet ever overlap or visually cover composer text, or must it only move
inside blank columns adjacent to already stable wrapped lines?

## Recommended Default

Never overlap composer text. Allow only blank adjacent regions, and force an
immediate return home on submit, modal open, resize, paste burst, or command
execution.

## Promotion Trigger

Promote after idle movement is accepted, or earlier only if Вова explicitly
prioritizes composer curiosity over idle screensaver behavior.
