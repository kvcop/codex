# Fork Notes

This branch carries local experiments and user-preference changes on top of
upstream Codex Rust releases. Keep this file focused on fork deltas that may
conflict with future upstream merges or need manual retesting after an update.

## Markdown Tables

- Branch lineage: `fix/md-tables-rust-v0.135.0-new`.
- Local behavior keeps the wide-table readability fallback: when a table cannot
  fit or wrapped rows become too tall to scan, it renders rows as key/value
  records instead of preserving an unreadable grid.
- Streaming behavior holds incomplete pipe-table chunks until the header
  delimiter confirms a table, then renders the live table instead of showing raw
  markdown line-by-line.
- Upstream `rust-v0.135.0` added app-style table rendering and improved column
  allocation. The fork currently keeps upstream app-style rows for readable
  tables and applies the local record fallback for cramped tables.
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
