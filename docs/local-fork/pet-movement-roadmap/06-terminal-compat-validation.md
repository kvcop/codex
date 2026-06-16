---
parent_roadmap: ./_roadmap.md
plan_id: 06-terminal-compat-validation
status: validation-note
planning_maturity: evidence-needed
depends_on: [01-movement-substrate]
unblocks: [05-kitty-placement-lifecycle]
---

# Validation 06: Terminal Compatibility

## Question To Prove

Does movement remain acceptable across the terminal setups Вова actually uses?

## Scenarios / Evidence

- Kitty inside tmux with passthrough enabled.
- Plain Kitty without tmux.
- SSH/mobile client fallback where images may not render.
- Resize, scroll, tmux pane switch, Kitty tab switch.
- Sixel path remains unchanged where supported.

## Candidate Checks

- Run the debug binary with movement debug mode enabled.
- Confirm the horizontal right-lane patrol has enough travel distance to be
  visible, does not reduce normal text wrap width, and switches between
  `move_left`/`running-left` and `move_right`/`running-right` frames.
- Trigger idle/running/waiting pet states.
- Observe whether movement worsens the rare divider/tool-call disappearance.
- Confirm transcript rendering remains readable when images do not render.

## Promotion Trigger

Promote to executable plan if validation finds a reproducible blocker that
cannot be fixed inside the active implementation plan.

## Close Without Implementation

Close when movement debug evidence is acceptable and no terminal-specific
workaround is needed.
