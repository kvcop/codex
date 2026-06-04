---
parent_roadmap: ./_roadmap.md
plan_id: 04-letter-interactions
status: future-note
planning_maturity: far-horizon
depends_on: [01-movement-substrate, 03-composer-curiosity-grill]
unblocks: []
---

# Future 04: Letter Interactions

## Scope

- Explore animations where the pet appears to pick up, rotate, inspect, or
  return a rendered glyph.

## Why Deferred

This likely requires a text restoration contract, collision model, and careful
ownership boundaries. It should not be mixed into movement substrate or first
behavior plans.

## Candidate Deliverables

1. Define whether interactions are pure image overlay or actual text mutation.
2. Define how Codex restores original cells exactly.
3. Add failure handling for resize, scroll, submit, and modal transitions.

## Validation Ideas

- Snapshot or buffer tests for restored cells.
- Manual checks with Russian text, emoji, wide glyphs, and hyperlinks.

## Promotion Trigger

Promote through a fresh roadmap or plan only after composer/idle movement has a
stable safety model.

## Close Without Implementation

Close if overlay-only movement provides enough delight and text interactions
feel too risky or distracting.
