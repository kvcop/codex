# План изменений: бесшовный рендер TUI pets в Kitty/tmux

> **How to read this plan:** смотри `Decision Brief` и `Execution Contract`.
> `Machine Appendix` нужен исполнителю/ревьюеру для деталей.
>
> **Plan freeze:** после approval или старта реализации этот файл read-only,
> кроме явных пользовательских amendments. Не переписывать план задним числом
> под уже сделанную реализацию.

## Decision Brief

**Goal:** убрать заметное мигание TUI-пета при частых redraw-кадрах Codex
(`Working`, ввод текста, resize), сохранив путь для будущих движущихся pets.

**Non-negotiable constraints:**
- После каждой фазы делать debug-сборку `cargo build -p codex-cli`.
- Вова вручную проверяет debug-бинарь и говорит, стало лучше или хуже.
- Не ломать текущий Sixel path и `/pets` picker preview.
- Не отключать tmux passthrough: `TMUX`/`TMUX_PANE` должны оставаться видимыми.
- Не смешивать локальные fork docs с upstream docs; этот план живет в
  `docs/local-fork/`.

**Design calls you must notice:**
- Phase 1 (`dedupe`) полезна независимо от финального Kitty-протокола: не надо
  слать image escape payload, если картинка, позиция и размер не изменились.
- Phase 2 (`double-buffer`) допустима только как тонкая промежуточная ступень.
  Не строить вокруг нее большую архитектуру, которую потом жалко выбрасывать.
- Phase 3 переводит Kitty на placement-aware path: текущий `delete + retransmit`
  заменяется на показ нового placement/frame до удаления старого placement.
- Sixel остается отдельным clear/redraw path.

**Known architectural risks:**
- Kitty graphics в tmux остаются unsafe-экспериментом: passthrough может
  отличаться между Kitty/tmux/SSH/телефонными клиентами.
- Если terminal image placement не переживает обычный ratatui redraw без
  повторного emit, Phase 1 может не помочь или ухудшить видимость пета.
- `q=2` подавляет ответы терминала, поэтому для cached `a=p` path нельзя
  надежно узнать, был ли image evicted, без отдельного handshake.

**Phase outcomes:**
- Phase 1: одинаковые pet draw requests не вызывают повторный Kitty delete/send.
- Phase 2: смена кадра в Kitty делает "show new, then remove old placement",
  без пустого интервала между delete и transmit.
- Phase 3: Kitty path использует стабильные placement/frame ids и минимизирует
  повторную передачу PNG во время анимации.
- Phase 4: cleanup, docs, финальные проверки, затем release build только после
  удачной ручной проверки debug-бинаря.

**Assumptions:**
- Основной наблюдаемый flicker вызван текущим порядком `kitty_delete_image()`
  перед каждой передачей кадра.
- Текущий manual target: Kitty inside tmux with `allow-passthrough on` and
  `CODEX_UNSAFE_TUI_PETS_PROTOCOL=kitty`.

**Non-goals:**
- Не реализовывать движение пета в этом плане.
- Не менять публичный CLI/config API.
- Не превращать pets в ratatui buffer content.

## Execution Contract

### Текущее состояние

- **Git baseline:** `fix/md-tables-rust-v0.137.0-new`, clean before planning.
- **Ключевые файлы:**
  - `codex-rs/tui/src/app.rs`: ambient pet рисуется после основного TUI frame на
    `TuiEvent::Draw | TuiEvent::Resize`.
  - `codex-rs/tui/src/tui.rs`: `draw_ambient_pet_image()` пишет image payload
    напрямую в terminal backend через `sync_update`.
  - `codex-rs/tui/src/pets/mod.rs`: `render_pet_image()` сейчас удаляет Kitty
    image id перед каждым новым request.
  - `codex-rs/tui/src/pets/image_protocol.rs`: Kitty helpers и tmux passthrough
    wrapper.
  - `codex-rs/tui/src/pets/ambient.rs`: `AmbientPetDraw`, frame selection,
    animation timing.
  - `codex-rs/tui/src/chatwidget/tests/status_and_layout.rs`: layout coverage
    для ambient pet.
  - `FORK_NOTES.md`: обновить после принятой реализации, не во время
    промежуточных экспериментов.
- **Source fact:** Kitty graphics protocol says same `image id + placement id`
  can replace placement without flicker, while re-transmitting data for an
  existing image id deletes its placements first:
  <https://sw.kovidgoyal.net/kitty/graphics-protocol/>.

### Phase 1: Dedupe identical redraws

**Goal:** убрать лишний `delete + transmit` при обычных redraw, когда pet frame,
позиция, размер и protocol не изменились.

**Inputs:**
- Current `PetImageRenderState`.
- Current `AmbientPetDraw`.

**Outputs:**
- State tracks last visible draw key for Kitty and Sixel separately enough to
  skip exact repeats safely.
- Repeated identical Kitty render emits no delete/transmit payload.
- Explicit clear (`request = None`) still deletes/clears the last visible image.

**Invariants:**
- If request changes frame, protocol, position, dimensions, or clear area, it
  must render.
- Sixel clear semantics stay unchanged unless exact repeat skipping is proven
  safe by tests.
- Cursor save/restore behavior remains intact for non-skipped draws.

**Acceptance criteria:**
- [ ] Unit test: identical Kitty request after first render writes no delete and
      no PNG payload.
- [ ] Unit test: changed frame still renders.
- [ ] Unit test: `request = None` after skipped redraw still deletes the last
      Kitty image.
- [ ] Debug build succeeds.
- [ ] Manual check: typing text and watching `Working` timer flickers less or
      not worse.

**Validation commands:**

```bash
cd codex-rs
cargo test -p codex-tui pets:: --lib
cargo build -p codex-cli
```

**Manual command:**

```fish
cd /home/user/code/mine/rust/codex/codex-rs
set -x CODEX_UNSAFE_TUI_PETS_PROTOCOL kitty
target/debug/codex
```

**Rollback:** revert the Phase 1 commit if manual check says the pet disappears
or flicker becomes worse.

**Forbidden shortcuts:**
- Do not skip draws only based on frame path; position and size matter.
- Do not make skipped draws clear the image area.

**Files likely touched:**
- `codex-rs/tui/src/pets/mod.rs`
- Maybe `codex-rs/tui/src/pets/ambient.rs` if deriving/computing a draw key is
  cleaner there.

### Phase 2: Kitty show-before-delete frame swap

**Goal:** remove the empty interval when animation changes frames.

**Inputs:**
- Phase 1 state tracking.
- Kitty image id base for ambient pet and picker preview.

**Outputs:**
- Kitty frame changes use a new/stable image id for the new frame.
- New frame is displayed before old placement is removed.
- Old cleanup targets only the old placement/image, never the newly displayed
  frame.

**Invariants:**
- Ambient pet and picker preview use separate id ranges.
- A transition from frame A to frame B must not call global delete before B is
  visible.
- Same-frame redraw remains deduped by Phase 1.
- Clear on disable/shutdown still removes all placements/images owned by that
  pet renderer.

**Acceptance criteria:**
- [ ] Unit test: frame transition output orders new display before old delete.
- [ ] Unit test: deletion is placement-specific or old-image-specific and does
      not target the new image id.
- [ ] Unit test: ambient and picker preview do not reuse the same id range.
- [ ] Debug build succeeds.
- [ ] Manual check: animated `running` state no longer shows background gaps
      between frames, or gaps are materially reduced.

**Validation commands:**

```bash
cd codex-rs
cargo test -p codex-tui pets:: --lib
cargo build -p codex-cli
```

**Rollback:** revert Phase 2 only; Phase 1 can remain if it passed manual
checking.

**Forbidden shortcuts:**
- Do not solve this by disabling animation globally.
- Do not keep unlimited image ids without an ownership/cleanup rule.
- Do not use capital delete forms for old frames unless freeing image data is
  intentional and tested.

**Files likely touched:**
- `codex-rs/tui/src/pets/mod.rs`
- `codex-rs/tui/src/pets/image_protocol.rs`

### Phase 3: Placement-aware Kitty path

**Goal:** move from frame-swap workaround to the intended Kitty model: stable
placement ids and cached frame images, with no PNG re-transmit on ordinary
animation ticks when the frame is already known.

**Inputs:**
- Phase 2 id ownership model.
- Kitty protocol helpers for transmit-only, put/display existing image, and
  placement-specific delete.

**Outputs:**
- Renderer can distinguish:
  - first time a frame is seen: transmit and display;
  - known frame: put/display existing image by id and placement id;
  - clear: delete owned placements/images.
- Position-only changes update placement without re-sending PNG bytes when
  possible.
- Fallback remains: if cached put is too unreliable in tmux/SSH, keep Phase 2 as
  the active Kitty path and document why.

**Invariants:**
- The renderer owns a bounded id range per pet surface.
- No unbounded cache growth across arbitrary custom pet frames.
- Sixel path does not depend on Kitty placement state.
- No terminal response parsing is introduced unless it is bounded and
  non-blocking.

**Acceptance criteria:**
- [ ] Unit test: known frame uses `a=p`/put-style command or equivalent helper
      instead of re-transmitting PNG bytes.
- [ ] Unit test: position-only move does not resend the image payload.
- [ ] Unit test: clear removes all owned visible placements/images.
- [ ] Debug build succeeds.
- [ ] Manual check: `Working` timer, typing, scroll, resize, tmux pane switch,
      and SSH fallback are acceptable.

**Validation commands:**

```bash
cd codex-rs
cargo test -p codex-tui pets:: --lib
cargo build -p codex-cli
```

**Rollback:** if cached placement is unreliable, keep Phase 1 and Phase 2,
document Phase 3 as deferred in `FORK_NOTES.md`, and do not ship cached put.

**Forbidden shortcuts:**
- Do not block reading terminal acknowledgements in the TUI draw path.
- Do not assume Kitty cache persistence across alternate-screen resets or tmux
  server changes.

**Files likely touched:**
- `codex-rs/tui/src/pets/mod.rs`
- `codex-rs/tui/src/pets/image_protocol.rs`

### Phase 4: Polish, docs, release gate

**Goal:** land the accepted rendering path cleanly and preserve future upgrade
context.

**Outputs:**
- `FORK_NOTES.md` documents the accepted pet-rendering delta and retest list.
- Focused tests pass.
- Debug build has been manually accepted by Вова.
- Release build is produced only after manual acceptance.

**Acceptance criteria:**
- [ ] `just fmt` run from `codex-rs`.
- [ ] `just fix -p codex-tui` run if Rust code changed.
- [ ] `just test -p codex-tui pets` or narrower equivalent passes.
- [ ] `cargo build -p codex-cli` passes after the final code phase.
- [ ] Вова confirms manual debug behavior is acceptable.
- [ ] `cargo build --release -p codex-cli` passes.
- [ ] `target/release/codex --version` reports expected version.

**Rollback:** revert the last accepted code phase; keep this plan and update
`FORK_NOTES.md` only for behavior that actually remains.

### Миграции и совместимость

- **State-файлы:** no state migration.
- **БД/Alembic:** not applicable.
- **API/CLI/UI compatibility:** no public CLI/config changes planned.
- **Terminal compatibility:** Kitty path changes only when Kitty protocol is
  selected; Sixel keeps its current behavior.

### Логи и наблюдаемость

- Keep existing `tracing::warn!` error paths.
- Add debug/trace logging only if it helps manual diagnosis and stays bounded.
- Do not log raw image payloads.

### Тестирование

- Unit:
  - `codex-rs/tui/src/pets/mod.rs` protocol output ordering/state tests.
  - `codex-rs/tui/src/pets/image_protocol.rs` helper formatting tests.
- Integration/UI snapshots:
  - Existing ambient pet layout tests should remain valid.
  - Add snapshot only if visible ratatui text/layout changes.
- Manual:
  - Kitty in tmux with passthrough.
  - Type in composer while pet is visible.
  - Let `Working` timer tick for at least 1-2 minutes.
  - Trigger `running`, `waiting`, `review`, `failed` animations if practical.
  - Resize pane.
  - Scroll transcript.
  - Switch tmux panes.
  - SSH/mobile client fallback: acceptable behavior is no broken transcript
    rendering even if images do not show.

### Out of scope but tracked

- Real pet movement across the terminal. Promotion trigger: after flicker is
  acceptable, create a follow-up plan for movement state, collision/anchor
  rules, and redraw scheduling.
- Text bubble/notification labels. Promotion trigger: user asks to show pet
  status text in the CLI.
- Terminal-side Kitty animation protocol (`a=f`, `a=c`, `a=a`). Promotion
  trigger: app-driven frame scheduling remains visibly choppy after placement
  fixes.

### Риски и откат

- **Risk:** dedupe causes stale/disappearing images after ratatui redraw.
  **Mitigation:** manual Phase 1 gate; rollback Phase 1 if worse.
- **Risk:** multiple image ids leak terminal image memory.
  **Mitigation:** bounded id range and explicit clear.
- **Risk:** cached `a=p` path fails after tmux/SSH cache eviction.
  **Mitigation:** Phase 3 fallback to Phase 2; no blocking terminal ack parsing.
- **Risk:** Sixel behavior regresses.
  **Mitigation:** keep Sixel path separate and covered by existing tests.

### Execution

- **Recommended mode:** `phased-execution`.
- **Why:** each phase has a small code surface, a debug build, and a required
  manual visual gate before continuing.
- **Commit policy:** commit plan now; during implementation, commit each phase
  only after focused tests and manual debug acceptance, unless Вова asks to keep
  a phase uncommitted for quick iteration.
- **Plan freeze starts:** after approval or implementation start.

### Открытые вопросы

- Should Phase 3 ship cached `a=p` by default if it is better in Kitty but worse
  over SSH/mobile? Default: keep it behind protocol capability/state fallback,
  not a config flag.
- Should debug builds be copied into fish-selected release path during testing?
  Default: no; run `target/debug/codex` directly for each manual gate.

## Machine Appendix

- Current symptom points to draw-order/render-state, not pet asset loading:
  `app.rs` renders the TUI frame first, then emits the ambient pet image.
- Current Kitty helper uses one ambient image id (`0xC0DE`) and one preview id
  (`0xC0DF`), so every animation frame currently reuses the same id.
- Current `kitty_delete_image()` uses `d=I`, which frees image data as well as
  placements. That is hostile to flicker-free animation.
- Avoid putting this plan in `FORK_NOTES.md`; that file is for accepted local
  deltas and retest notes, not pending design.
