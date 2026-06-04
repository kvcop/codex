---
parent_roadmap: ./_roadmap.md
plan_id: 01-movement-substrate
status: approved
planning_maturity: current-wave
depends_on: []
unblocks: [02-idle-screensaver-grill, 06-terminal-compat-validation]
---

# План изменений: Pet Movement Substrate

> **How to read this plan:** смотри `Decision Brief` и
> `Execution Contract`. `Machine Appendix` нужен исполнителю/ревьюеру.
>
> **Plan freeze:** после Claude approval и старта `phased-execution` этот файл
> read-only, кроме Drift Alert или явной пользовательской amendment.

## Decision Brief

**Parent roadmap:** `docs/local-fork/pet-movement-roadmap/_roadmap.md`
**Parent item:** `01-movement-substrate`
**Return condition:** debug movement substrate accepted manually or a blocker is
recorded in the roadmap.

**Goal:** добавить безопасный технический слой движения TUI-пета без решений про
idle/screensaver/composer personality.

**Non-negotiable constraints:**
- Default behavior stays visually identical unless hidden local debug movement
  is enabled.
- No public CLI/config schema changes.
- No stable Kitty placement caching (`a=p`) in this plan.
- Sixel remains home-only when movement debug mode is enabled.
- Debug targets must stay inside the right-side pet lane and be proven empty in
  the rendered buffer before drawing.
- If movement needs behavior decisions outside that safe lane, stop at
  `propose_grill:02-idle-screensaver-grill`.

**Design calls you must notice:**
- Movement is a positioning layer on top of the existing pet frame selection,
  not a new terminal image protocol.
- Hidden debug trigger: `CODEX_UNSAFE_TUI_PET_MOVEMENT=lane-patrol`.
- Movement ticks must compose with existing pet animation ticks and stop when
  `animations = false`.
- The implementation must reject unsafe target rectangles row-by-row, including
  footer/status/bottom-pane rows.

**Phase outcomes:**
- Phase 1: pure movement model with bounded interpolation and tests.
- Phase 2: draw-context wiring that preserves home placement by default and
  rejects unsafe target rectangles.
- Phase 3: hidden Kitty-only debug lane patrol, docs/audit, debug build for
  manual tmux validation.

**Assumptions:**
- A visible debug patrol inside the right-side lane is enough to validate timing,
  redraw, and tmux behavior before behavior-heavy modes.
- `CODEX_UNSAFE_TUI_PETS_PROTOCOL=kitty` remains the expected tmux test setup.
- The first patrol route is vertical movement within the rendered right-side pet
  reserve, from home up by a bounded number of rows and back. It is not the
  possibly-zero gap between composer-bottom and screen-bottom anchors.

**Non-goals:**
- No idle screensaver behavior.
- No composer curiosity behavior.
- No letter pickup/restore interactions.
- No Sixel movement.
- No release build until manual debug behavior is accepted.

## Execution Contract

### Текущее состояние

- **Git baseline:** branch `fix/md-tables-rust-v0.137.0-new`, clean before
  plan creation.
- **Ключевые файлы:**
  - `codex-rs/tui/src/pets/ambient.rs`: current `AmbientPet`, frame selection,
    `AmbientPetDraw`, and `schedule_next_frame()`.
  - `codex-rs/tui/src/chatwidget/pets.rs`: `ambient_pet_draw(...)`, current
    composer/screen-bottom anchor selection, wrap reservation helpers.
  - `codex-rs/tui/src/app.rs`: calls `ambient_pet_draw(...)` after rendering
    the chat widget inside the same frame closure.
  - `codex-rs/tui/src/tui.rs`: emits ambient pet image payload inside the same
    synchronized update as the chat frame.
  - `codex-rs/tui/src/chatwidget/tests/status_and_layout.rs`: existing ambient
    pet layout tests.
  - `FORK_NOTES.md`: local fork behavior and retest checklist.
- **Current source facts:** see
  `docs/local-fork/pet-movement-roadmap/REFERENCE.md`.

### Phase 1: Movement Model

**Goal:** introduce a small pure movement model that can be tested without
terminal image protocol output.

**Inputs:**
- Home rect: x/y/columns/rows from current ambient pet placement.
- Optional movement mode: disabled or lane patrol.
- Time source: elapsed duration since movement start.

**Outputs:**
- New focused movement module, preferably `codex-rs/tui/src/pets/movement.rs`.
- A `PetMovement` state object or equivalent that can:
  - return home when disabled;
  - compute a bounded intermediate rect for a vertical lane-patrol target;
  - report the next movement tick delay only while movement is active.

**Invariants:**
- Movement model never reads terminal state, env vars, image files, or config.
- No unbounded timers; every scheduled delay has a minimum and maximum.
- Disabled movement always returns the home rect exactly.
- Movement does not choose behavior targets; it only follows a supplied safe
  target.
- The first lane-patrol route is supplied already bounded to the right-side
  reserved pet columns; buffer ownership is checked by the draw-context wiring,
  not by the pure movement model.

**Acceptance criteria:**
- [ ] Unit tests cover disabled movement returning home exactly.
- [ ] Unit tests cover target interpolation and return-home behavior.
- [ ] Unit tests cover bounded next-tick scheduling.
- [ ] Unit tests cover disabled animations causing no movement tick.

**Validation commands:**

```bash
cd codex-rs
just test -p codex-tui pets:: --lib
```

**Rollback:** remove the new module and tests; no user-visible behavior should
have changed in this phase.

**Forbidden shortcuts:**
- Do not parse env vars in the model.
- Do not access terminal buffers in the model.
- Do not add public config fields.

**Files likely touched:**
- `codex-rs/tui/src/pets/mod.rs`
- `codex-rs/tui/src/pets/movement.rs`

### Phase 2: Safe Draw Context Wiring

**Goal:** wire the movement model into ambient pet drawing while preserving
current home placement by default.

**Inputs:**
- Existing `AmbientPet::draw_request(...)` behavior.
- Rendered chat frame buffer available in the app draw closure.
- Current ambient pet protocol.

**Outputs:**
- Ambient pet draw path can receive enough context to validate a candidate
  movement target against the rendered buffer.
- Default/disabled movement still produces the same x/y/size as before.
- Sixel protocol ignores movement and stays at home.
- Candidate target rect is rejected unless every cell in the sprite rect is
  safely blank/owned for the current frame.

**Invariants:**
- The existing no-modal/no-popup pet hiding rule remains.
- Home placement remains available even when the debug target is rejected.
- The check must include footer/status/bottom-pane rows, not only transcript
  rows.
- Target and intermediate rects outside the rendered buffer area are rejected.
  Buffer default cells outside the rendered area must be treated as unknown, not
  as blank.
- A rejected target must not schedule an endless retry loop.

**Acceptance criteria:**
- [ ] Existing home-placement tests still pass with the same expected home x/y
      values; only test structure may change.
- [ ] Test: movement disabled produces exact current home placement.
- [ ] Test: Sixel protocol produces home placement even when movement is
      configured.
- [ ] Test: occupied target cells reject movement and draw home.
- [ ] Test: safe target cells allow movement x/y inside the right-side lane.
- [ ] Test: any target partly outside the rendered buffer area is rejected and
      draws home.

**Validation commands:**

```bash
cd codex-rs
just test -p codex-tui pets:: --lib
just test -p codex-tui ambient_pet --lib
```

**Rollback:** revert the wiring changes; Phase 1 pure model may stay only if it
is unused and harmless, otherwise revert both phases.

**Forbidden shortcuts:**
- Do not assume the right-side reserve is empty without inspecting the rendered
  buffer.
- Do not draw over nonblank cells to make the demo look lively.
- Do not re-enable cached Kitty placement.

**Files likely touched:**
- `codex-rs/tui/src/pets/ambient.rs`
- `codex-rs/tui/src/chatwidget/pets.rs`
- `codex-rs/tui/src/app.rs`
- `codex-rs/tui/src/chatwidget/tests/status_and_layout.rs`

### Phase 3: Hidden Debug Movement And Manual Gate

**Goal:** expose a local debug-only lane patrol so Вова can test real movement
inside Kitty/tmux before any behavior mode is planned.

**Inputs:**
- Phase 1 movement model.
- Phase 2 safe draw context.
- Hidden env var `CODEX_UNSAFE_TUI_PET_MOVEMENT`.

**Outputs:**
- `CODEX_UNSAFE_TUI_PET_MOVEMENT=lane-patrol` enables a small right-lane patrol
  only for Kitty/KittyLocalFile protocols. The patrol moves vertically inside
  the rendered right-side reserve, from home upward by a bounded distance and
  back.
- Unknown env values are silently ignored, matching the local unsafe pet
  protocol override style.
- `animations = false` disables movement scheduling.
- `FORK_NOTES.md` records the hidden debug flag, Sixel home-only behavior, and
  manual retest list.

**Invariants:**
- No public config schema update.
- No release build in this phase unless the user explicitly asks after manual
  debug validation.
- Movement must return home when modal/popup blocks ambient pet drawing.
- The debug flag must not affect `/pets` picker preview.

**Acceptance criteria:**
- [ ] Unit test or focused widget test covers hidden/debug movement selection
      without mutating process env directly in tests.
- [ ] `FORK_NOTES.md` documents the debug movement flag and residual risk.
- [ ] `just fmt` passes.
- [ ] `just fix -p codex-tui` passes.
- [ ] `just test -p codex-tui pets:: --lib` passes.
- [ ] `just test -p codex-tui ambient_pet --lib` passes.
- [ ] `cargo build -p codex-cli` produces `target/debug/codex`.
- [ ] Manual Kitty/tmux check: pet patrol is visible, bounded to the right
      lane, does not cover text, and flicker is not worse than accepted
      residual.

**Manual command:**

```fish
cd /home/user/code/mine/rust/codex/codex-rs
set -x CODEX_UNSAFE_TUI_PETS_PROTOCOL kitty
set -x CODEX_UNSAFE_TUI_PET_MOVEMENT lane-patrol
target/debug/codex
```

**Rollback:** revert Phase 3 if manual movement is distracting, unstable, or
visibly worsens tmux flicker. Keep earlier phases only if they remain inert.

**Forbidden shortcuts:**
- Do not make movement default-on.
- Do not add a `/pets` UI switch yet.
- Do not ship release build before manual acceptance.

**Files likely touched:**
- `codex-rs/tui/src/pets/ambient.rs`
- `codex-rs/tui/src/chatwidget/pets.rs`
- `codex-rs/tui/src/chatwidget/tests/status_and_layout.rs`
- `FORK_NOTES.md`

### Миграции и совместимость

- **State-файлы:** no state migration.
- **Config/schema:** no public config fields; do not run
  `just write-config-schema` unless the implementation violates this plan and
  adds config, which should be treated as Drift Alert.
- **CLI/API compatibility:** no public CLI/API changes.
- **Terminal compatibility:** Kitty/KittyLocalFile only for movement. Sixel
  remains home-only.

### Логи и наблюдаемость

- Keep logs bounded. Do not log image payloads or full terminal buffers.
- Trace/debug only mode selection and target rejection summaries if useful.
- Manual visual validation remains the main evidence for Kitty/tmux behavior.

### Тестирование

- Use `just test` commands from this plan so checks follow the repo test recipe.
- Unit:
  - movement model tests in `pets::movement`;
  - existing pet image protocol tests.
- Widget/layout:
  - ambient pet home placement unchanged;
  - safe/occupied movement target checks;
  - Sixel home-only behavior.
- Manual:
  - Kitty inside tmux with passthrough;
  - normal typing;
  - `Working` timer;
  - horizontal divider/tool-call burst if practical;
  - resize and pane/tab switch.

### Out of scope but tracked

- Idle screensaver behavior: `02-idle-screensaver-grill`.
- Composer curiosity: `03-composer-curiosity-grill`.
- Letter interactions: `04-letter-interactions`.
- Cached Kitty placement lifecycle: `05-kitty-placement-lifecycle`.
- Terminal compatibility evidence after debug movement:
  `06-terminal-compat-validation`.

### Риски и откат

- **Risk:** movement reveals that repeated Kitty retransmit per position change
  is too flickery.
  **Response:** stop after manual debug validation and promote
  `05-kitty-placement-lifecycle` only if evidence requires it.
- **Risk:** right-side lane is not actually empty for some bottom-pane/status
  layouts.
  **Response:** reject targets row-by-row and stay home.
- **Risk:** hidden env flag becomes accidental public API.
  **Response:** document it as local unsafe debug only in `FORK_NOTES.md`.

### Execution

- **Recommended mode:** `phased-execution`.
- **Why:** three scoped phases, Rust code + tests + docs, manual debug gate, and
  Claude review requested by the user.
- **Commit policy:** commit each accepted phase; do not push unless asked.
- **Plan freeze starts:** after Claude approval and implementation start.

### Открытые вопросы

- None for Plan 01. Behavior choices are intentionally deferred to roadmap
  `requires_grill` items.

## Machine Appendix

- Roadmap review artifacts:
  - `.tmp/claude-reviews/pet-movement-roadmap-review.md`
  - `.tmp/claude-reviews/pet-movement-roadmap-followup.md`
- Roadmap commit: `217ac09475 Add pet movement roadmap`.
- Plan 01 Claude review:
  - initial executable-plan review `CHANGES_REQUESTED`, 2026-06-05;
  - follow-up executable-plan review `APPROVED`, 2026-06-05.
