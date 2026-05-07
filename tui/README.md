# Retrieval X-Ray TUI

A terminal UI debugger for the hybrid neural + lexical retrieval engine.

## Run

```bash
npm install
npm run tui
```

The web server (`cargo run --release --bin search_engine`) must be running first.

Optional env overrides:

| Var                              | Default                                |
| -------------------------------- | -------------------------------------- |
| `RETRIEVAL_XRAY_API_URL`         | `http://127.0.0.1:3000`                |
| `RETRIEVAL_XRAY_QRELS`           | `benchmarks/niche_db/qrels_100.tsv`    |
| `RETRIEVAL_XRAY_QUERIES`         | `benchmarks/niche_db/queries_100.tsv`  |
| `RETRIEVAL_XRAY_K`               | `1,3,5,10`                             |
| `RETRIEVAL_XRAY_INITIAL_QUERY`   | (empty)                                |

## Recommended terminal font: 0xProto

The TUI uses precise column alignment, score-table formatting, box-drawing
characters, and Unicode glyphs (`◆ ★ ⚠ ✓ › · ─ →`). For the cleanest look set
your terminal font to **[0xProto](https://github.com/0xType/0xProto)**, a
high-legibility programming font designed to minimize cognitive load.

```bash
# macOS via Homebrew Cask
brew install --cask font-0xproto
```

Then in your terminal preferences set the font family to `0xProto` (or
`0xProto Nerd Font` if you want icons too) at 13–14 pt with line height
1.0–1.1.

Other monospace fonts that work well: JetBrains Mono, Berkeley Mono, IBM Plex
Mono, Iosevka. Any monospace with full Unicode coverage is fine.

## Keys

| Key            | Action                                |
| -------------- | ------------------------------------- |
| Enter          | Run search                            |
| Ctrl+R         | Re-run last query                     |
| Ctrl+E         | Run batch eval                        |
| Ctrl+T         | Toggle dark / light theme             |
| Ctrl+P / Ctrl+N| Previous / next query in history      |
| ↑ / ↓          | Move selection in ranking             |
| Tab / S-Tab    | Cycle focus between panels            |
| N / D / M      | Normal · Detail · Metrics views       |
| ?              | Toggle help panel                     |
| /              | Focus query input from anywhere       |
| C              | Copy selected URL to clipboard        |
| Esc / Q        | Quit                                  |

Clipboard copy tries `pbcopy` (macOS) → `clip` (Windows) → `wl-copy` (Wayland) → `xclip` (X11) automatically.
