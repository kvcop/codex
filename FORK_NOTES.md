# Fork Notes

This branch carries local experiments and user-preference changes on top of
upstream Codex Rust releases. Keep this file focused on fork deltas that may
conflict with future upstream merges or need manual retesting after an update.

## Markdown Tables

- Branch lineage: `fix/md-tables-rust-v0.137.0-new`, merged from
  `fix/md-tables-rust-v0.136.0-new`.
- Upstream `rust-v0.137.0` still includes the primary key/value record fallback
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
- Upstream `rust-v0.136.0` added official table streaming/rendering changes, and
  `rust-v0.137.0` did not replace the local readability guards, so local
  streaming tests should be kept mainly as regression coverage for the
  raw-markdown flicker cases seen with larger models.
- Retest after each upstream merge:
  - wide Russian tables with many columns;
  - emoji-width alignment in centered cells;
  - fenced markdown tables while streaming;
  - very wide prose/path-heavy tables that should switch to records.

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
- Retest after changes:
  - pet appears in a Kitty terminal inside tmux;
  - image survives normal TUI redraws;
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
