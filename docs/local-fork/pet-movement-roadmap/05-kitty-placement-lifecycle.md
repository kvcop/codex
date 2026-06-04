---
parent_roadmap: ./_roadmap.md
plan_id: 05-kitty-placement-lifecycle
status: refactor-gate
planning_maturity: gated
depends_on: [01-movement-substrate, 06-terminal-compat-validation]
unblocks: []
---

# Refactor Gate 05: Kitty Placement Lifecycle

## Problem Signal

Movement may make repeated PNG retransmits too expensive or visibly flickery,
but the previous cached-placement attempt was unreliable in tmux tab switching.

## Why Not Planned Yet

The fork already has concrete negative evidence for stable `a=p` placement in
tmux: stale/foreign placements could remain visible and current pets sometimes
failed to repaint.

## Next Safe Step

Keep the current retransmit/dedupe path for Plan 01. Collect manual evidence
from movement debug builds before planning placement caching again.

## Evidence Needed

- Movement debug build shows unacceptable flicker or load with retransmit path.
- A tmux-safe placement lifecycle strategy is identified.
- Clear/repaint behavior can be tested across tab switch, pane switch, resize,
  and SSH/mobile fallback.

## Promotion Trigger

Promote to fresh `roadmap-planning` or `plan-creation` only when movement
evidence proves the current path is not good enough.
