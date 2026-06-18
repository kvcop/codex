# Fork Notes

This branch carries local experiments and user-preference changes on top of
upstream Codex Rust releases. Keep this file focused on fork deltas that may
conflict with future upstream merges or need manual retesting after an update.

## Markdown Tables

- Branch lineage: `fix/md-tables-rust-v0.141.0-new`, merged from
  `fix/md-tables-rust-v0.140.0-new`.
- Upstream `rust-v0.141.0` still includes the primary key/value record fallback
  for cramped markdown tables and preserves OSC 8 hyperlink metadata through
  that renderer. Keep the upstream renderer as the base unless a regression is
  proven locally.
- Local behavior still adds readability guards on top of upstream: when a grid
  technically fits but wrapped rows become too tall to scan, or when many
  headers are squeezed into very narrow columns, render rows as key/value
  records instead of preserving the grid.
- Local width measurement treats emoji variation-selector grapheme clusters as
  two terminal cells when needed, which keeps centered emoji table cells aligned
  in Kitty/tmux.
- Streaming behavior holds incomplete pipe-table chunks until the header
  delimiter confirms a table, then renders the live table instead of showing raw
  markdown line-by-line.
- Upstream `rust-v0.136.0` added official table streaming/rendering changes,
  and `rust-v0.141.0` still does not replace the local readability guards, so
  local streaming tests should be kept mainly as regression coverage for the
  raw-markdown flicker cases seen with larger models.
- Retest after each upstream merge:
  - wide Russian tables with many columns;
  - emoji-width alignment in centered cells;
  - fenced markdown tables while streaming;
  - very wide prose/path-heavy tables that should switch to records.

## Planned Desktop Rate Limit Resets

- Status: parked for the current `rust-v0.141.0` branch after the release merge
  is stable. Do not implement from this note directly; run `grill-me` and/or a
  focused plan first if the surface is still ambiguous.
- Source snapshot inspected statically: official Desktop DMG
  `Codex-26.611.62324-arm64`, `app.asar` built on 2026-06-17, DMG SHA-256
  `31d8e2666a0895a830df0832dc4083ae82a6e9bd26603141c0293acea6618211`.
- Official Desktop wording to preserve in any local UI/docs:
  - banner CTA: `Reset usage`;
  - modal CTA: `Reset rate limit`;
  - summary/menu row: `{# reset available}` / `{# resets available}`;
  - success toast: `Usage reset. You have {# left}`;
  - errors: `This reset was already used`, `No resets are available`,
    `Your usage does not need a reset right now`,
    `Couldn’t reset usage. Please try again`.
- Official Desktop backend shape observed in the webview bundle:
  - `GET /wham/rate-limit-reset-credits`;
  - `POST /wham/rate-limit-reset-credits/consume`;
  - request body: `{ "credit_id": <selected credit id>, "redeem_request_id": <uuid> }`.
- Planned app-server parity check: keep `idempotencyKey` mapped to
  `redeem_request_id`, but add an optional `creditId`/`credit_id` path before
  landing the feature if the backend still expects selected reset-card ids.
- Planned TUI placement: expose the reset affordance from `/status`, behind a
  confirmation modal. The modal must default to `Cancel` and must ignore
  confirm/cancel/navigation control keys for the first 500 ms after opening, so
  duplicated terminal `Enter`/`Esc`/`Tab`/`Backspace` events cannot accidentally
  consume a saved reset.
- Safety rule: development and automated tests must use mock app-server/backend
  fixtures only. Never run a test or manual smoke path that posts to a real
  ChatGPT/OpenAI backend with live auth. Any live redemption must be a separate,
  explicit manual action from Vladimir, because it can consume a real saved
  reset.

## Planned Usage Daily Week Ordering

- Status: planned after Desktop rate limit resets unless promoted earlier.
- Local fork expectation: the `usage daily` weekly chart/list should render
  Monday as the first row and Sunday as the last row.
- Implementation note: verify the upstream data semantics before changing the
  view, then normalize display order locally without changing stored usage
  values.

## TUI Pets In Tmux

- Local experiment: `CODEX_UNSAFE_TUI_PETS_PROTOCOL=kitty`.
- Purpose: allow `/pets` to try Kitty graphics while running inside tmux without
  unsetting `TMUX`/`TMUX_PANE`.
- Why this matters: unsetting tmux environment variables bypasses the safety
  warning, but it also prevents Codex from wrapping Kitty graphics in tmux DCS
  passthrough. Keeping `TMUX` visible lets the existing passthrough wrapper run.
- Tmux must allow passthrough for the wrapper to reach Kitty. Check it with:

  ```fish
  tmux show-options -gqv allow-passthrough
  ```

  Enable it for the current tmux server:

  ```fish
  tmux set -g allow-passthrough on
  ```

  If a pane has a local override, enable the current pane explicitly:

  ```fish
  tmux set -p allow-passthrough on
  ```

  Persist it in `~/.tmux.conf`:

  ```tmux
  set -g allow-passthrough on
  ```

  Tmux also supports `all`, which allows passthrough even for invisible panes;
  prefer `on` for testing because it only allows visible panes.
- Usage in fish:

  ```fish
  set -x CODEX_UNSAFE_TUI_PETS_PROTOCOL kitty
  codex
  ```

- This is intentionally unsafe. Kitty images in tmux may still corrupt
  scrollback, fail to stay pane-local, disappear on redraw, or behave badly
  during resize. Use it only for local testing.
- Current implementation only recognizes `kitty`; unknown values are ignored and
  normal detection is used.
- Local movement experiment: `CODEX_UNSAFE_TUI_PET_MOVEMENT=lane-patrol`.
  Unknown movement values are ignored. The patrol is debug-only, default-off,
  and only moves the ambient pet for Kitty-style protocols; Sixel stays at home.
  Usage in fish:

  ```fish
  set -x CODEX_UNSAFE_TUI_PETS_PROTOCOL kitty
  set -x CODEX_UNSAFE_TUI_PET_MOVEMENT lane-patrol
  codex
  ```

- Movement diagnostics use the `codex_tui::pets::movement` tracing target.
  For a focused trace run:

  ```fish
  set -x RUST_LOG codex_tui::pets::movement=trace
  set -x CODEX_UNSAFE_TUI_PETS_PROTOCOL kitty
  set -x CODEX_UNSAFE_TUI_PET_MOVEMENT lane-patrol
  target/debug/codex
  ```

- The debug patrol moves horizontally inside a right-side candidate lane. It
  does not reserve extra transcript/composer wrap width for movement; normal
  pet image reserve stays unchanged. Candidate target and in-flight sprite
  rectangles may extend left into the candidate lane only when every overlapping
  rendered-buffer cell is safe blank space. Background color on whitespace is
  allowed because the pet image covers that background anyway; non-whitespace
  symbols, skipped cells, visual text modifiers, wide-glyph continuations, and
  out-of-lane rectangles still keep the pet at home. Long lines may therefore
  shorten or suppress the patrol instead of losing text width.
- The patrol uses directional animation tracks while moving:
  `move_left`/`running-left` on the outbound leg and
  `move_right`/`running-right` on the return leg, falling back to the regular
  `running` track if a custom pet omits those animations. For generated
  animations that append idle frames after the action segment, movement loops
  the action segment so the pet does not slide in an idle pose.
- `animations = false` disables movement scheduling as well as frame animation.
  Rejected movement targets still keep the bounded movement cadence while the
  debug flag is on; identical draw dedupe suppresses redundant Kitty payloads.
  Revisit this only if manual testing shows visible churn or cost.
- Local flicker mitigation keeps the Phase 1+2 path from
  `docs/local-fork/pet-rendering-flicker-plan.md`: identical Kitty redraws are
  deduplicated, and frame changes display the new owned image id before deleting
  the previous image id without freeing image data.
- The ambient pet draw is emitted inside the same terminal synchronized update
  as the main chat frame so large transcript redraws do not expose an
  intermediate text-only frame before the pet image payload arrives.
- Upstream `rust-v0.141.0` moved the TUI onto the resize-reflow draw path
  without the old feature gate. The local merge keeps ambient pet drawing
  inside that same resize-reflow synchronized update through
  `draw_with_resize_reflow_and_ambient_pet_image`.
- When finalized history/tool-call rows are flushed into terminal scrollback,
  the ambient pet renderer forces one immediate redraw even if the pet frame and
  position did not change. This keeps the Phase 1 dedupe for ordinary frames but
  repairs most divider/tool-call repaint gaps.
- Phase 3 cached-placement rendering is intentionally deferred. It reduced PNG
  retransmits, but tmux tab switching left stale/foreign pet placements around
  and sometimes failed to repaint the current pet. Do not re-enable stable
  `a=p` placement caching until the fork has a tmux-safe placement lifecycle
  strategy.
- Known residual behavior: in Kitty inside tmux, the pet can still rarely
  disappear briefly during horizontal divider redraws or bursts of tool-call
  history. This is acceptable for now; revisit if it becomes frequent or before
  implementing real pet movement across transcript/composer rows.
- Current manual movement evidence: horizontal `lane-patrol` is acceptable as a
  test baseline. The pet patrols while its right-side lane is empty and returns
  home when rendered text enters that lane; the return is still a teleport, not
  final behavior.
- Retest after changes:
  - pet appears in a Kitty terminal inside tmux;
  - with `CODEX_UNSAFE_TUI_PET_MOVEMENT=lane-patrol`, pet visibly patrols only
    inside the right-side lane and never covers text;
  - image survives normal TUI redraws;
  - submitting a message does not introduce unacceptable blink;
  - horizontal section dividers before final-answer streaming do not introduce
    unacceptable blink;
  - switching Kitty/tmux tabs does not leave stale pets from other sessions;
  - resize behavior is acceptable or at least fails visibly;
  - `TMUX_PANE` without `TMUX` still uses tmux passthrough wrapping.

## TUI Pet Favorites

- Local `/pets` picker extension:
  - `Space` toggles a pet as favorite without selecting it.
  - `Enter` still selects the highlighted concrete pet.
  - the top `Random favorite each session` row toggles startup randomization.
- Persisted config keys:

  ```toml
  [tui]
  pet_favorites = ["codex", "custom:chefito"]
  pet_random_favorite = true
  ```

- Startup behavior:
  - when `pet_random_favorite` is true and favorites are present, Codex picks a
    fresh random favorite during TUI startup;
  - `tui.pet = "disabled"` has priority over random favorites;
  - the random choice is kept in memory for that process and does not rewrite
    `config.toml` on every start.
- Retest after upstream merges:
  - `/pets` opens with the random row and favorite checkboxes;
  - `Space` persists `pet_favorites`;
  - random startup picks from favorites;
  - disabling pets still prevents startup rendering even when random mode is
    enabled.

## TUI Plan Mode Nudge

- Local behavior disables the footer suggestion that appears when the draft
  contains the standalone word `plan`:

  ```text
  Create a plan?  shift + tab use Plan mode   esc dismiss
  ```

- Purpose: avoid noisy footer churn while writing ordinary prompts that mention
  plans. Plan mode itself remains available through the normal mode cycle and
  `/plan`.
- Retest after upstream merges:
  - typing `plan` in the composer does not replace the footer with the nudge;
  - `Shift+Tab` still cycles into Plan mode;
  - `/plan` still switches/submits through the Plan mode path.
