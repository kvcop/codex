# Reference: TUI Pet Movement

This reference preserves the source facts and decisions that the roadmap relies
on. The roadmap remains the control surface; this file is the evidence appendix.

## Current Source Facts

- Current branch: `fix/md-tables-rust-v0.137.0-new`.
- Existing local fork notes live in `FORK_NOTES.md`.
- The flicker mitigation plan lives in
  `docs/local-fork/pet-rendering-flicker-plan.md`.
- `ChatWidget::ambient_pet_draw(...)` currently returns one
  `AmbientPetDraw` anchored either to composer bottom or screen bottom,
  depending on `tui.pet_anchor`.
- `AmbientPet::draw_request(...)` computes the current pet frame and places it
  at the right edge of the supplied screen area.
- The existing text layout reserves right-side wrapping width through
  `ambient_pet_wrap_reserved_cols()` and `history_wrap_width(...)`.
- Existing tests cover:
  - pet hidden until selected;
  - composer-bottom and screen-bottom anchors;
  - reserved wrap width;
  - the fact that ambient pet draw uses full terminal screen area rather than
    the short inline viewport.
- Existing pet animation scheduling runs through `ChatWidget::pre_draw_tick()`
  into `AmbientPet::schedule_next_frame()` and
  `FrameRequester::schedule_frame_in(...)`. `AmbientPet::next_frame_delay()`
  returns no delay when terminal image protocol is unavailable or animations
  are disabled.
- Current rendering emits ambient pet image payload inside the same terminal
  synchronized update as the chat frame.
- Current renderer forces one pet redraw after finalized history/tool-call rows
  are flushed into scrollback.
- Phase 3 cached Kitty placement (`a=p`) is intentionally deferred because it
  was unreliable with tmux tab switching.
- Known residual: Kitty inside tmux can still rarely hide the pet briefly during
  divider or tool-call history bursts.

## User Ideas Preserved

- **Composer curiosity mode:** while the user types a multiline prompt, the pet
  can run to a stable line or word, inspect it, then panic/escape home on
  submit. The implementation must track composer growth upward as wrapped lines
  appear.
- **Transcript screensaver mode:** after idle time, the pet can find blank space
  to the right of shorter rendered rows, dance there, and avoid active UI.
- **Letter interactions:** future animations may pick up, rotate, and return
  text glyphs. This needs a separate safety contract for restoring text exactly.

## Decisions

- Use a parent roadmap because pet movement is long-horizon, evidence-dependent,
  and splits into behavior work, terminal rendering work, layout ownership, and
  compatibility validation.
- Require `grill-me` before behavior-heavy plans such as idle screensaver,
  composer curiosity, and letter interactions.
- Do not require `grill-me` before the first technical movement substrate plan:
  the first plan should create safe primitives, not decide behavior personality.
- Do not re-enable cached Kitty placement as part of the first movement wave.
  Treat it as a refactor gate that can be promoted only if movement performance
  or flicker evidence requires it.
- First-wave debug movement is Kitty-first. Sixel keeps the pet at home unless
  a later manual validation explicitly promotes Sixel movement.
- The first debug movement lane extends left from the right-side pet reserve as
  a candidate area, but it does not shrink transcript/composer wrap width beyond
  the normal static pet reserve. It is not the gap between composer-bottom and
  screen-bottom anchors because that gap can be zero in the normal inline
  viewport.
- Candidate movement rects outside the rendered buffer are unsafe. Codex cannot
  inspect terminal scrollback cells above the rendered viewport, so unknown cells
  must be rejected rather than treated as blank.
- The `FORK_NOTES.md` residual-flicker revisit note is handled by Plan 01 manual
  validation plus Validation 06 before behavior-heavy work is promoted.

## Review Artifacts

- Roadmap review with Claude:
  - initial review `CHANGES_REQUESTED`, 2026-06-05;
  - follow-up review `APPROVED`, 2026-06-05.
- Plan 01 review with Claude:
  - initial executable-plan review `CHANGES_REQUESTED`, 2026-06-05;
  - follow-up executable-plan review `APPROVED`, 2026-06-05.
