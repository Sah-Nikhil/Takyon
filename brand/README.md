# Brand assets

Every icon in this repo is generated from two files:

- `geometry.js` — the locked mark. Two shapes, copied verbatim from
  `docs/brand.md`. This is the only place the path data exists.
- `tokens.json` — colour. Deliberately placeholder values; see below.

Regenerate everything:

```
bun run --cwd brand build
```

Nothing under `svg/`, `apps/desktop/src-tauri/icons/` or `apps/desktop/public/`
should be hand-edited. Change the geometry or the tokens, re-run the build, and
commit the result.

## Colour is not locked yet

`docs/brand.md` leaves the palette open until v0.6, so `tokens.json` holds three
placeholder values (`fg`, `accent`, `plate`) chosen only so the assets can exist.
The accent is a Cherenkov-derived cyan, which is the one colour idea that
survived Direction I — treat it as a stand-in, not a decision.

When the real scheme lands, edit `tokens.json` and re-run the build. That is the
whole migration; no asset needs redrawing.

## Surfaces

| Surface | File | Notes |
|---|---|---|
| Installer, taskbar, Alt-Tab, Start, uninstall entry | `apps/desktop/src-tauri/icons/icon.ico` | 16/24/32/48/64/128/256, PNG frames |
| Tauri bundle set | `apps/desktop/src-tauri/icons/{32x32,128x128,128x128@2x,icon}.png` | names are fixed; `tauri.conf.json` refers to them literally |
| MSIX / Store tiles | `apps/desktop/src-tauri/icons/Square*Logo.png`, `StoreLogo.png` | |
| macOS, when that target exists | `apps/desktop/src-tauri/icons/icon.icns` | ic07–ic14, PNG payloads |
| System tray | `apps/desktop/src-tauri/icons/tray-{dark,light}.{png,ico}` | two polarities — see below |
| Palette / Chat Surface / Settings UI | `apps/desktop/src/components/Mark.tsx` | inherits `currentColor` and `--accent` |
| Palette input field, ~17px | `InputMark` in `Mark.tsx` | the slot a search icon would normally occupy |
| Settings header, About, first run | `apps/desktop/src/components/Lockup.tsx` | mark + lowercase wordmark |
| WebView tab / dev server | `apps/desktop/public/favicon.{svg,ico}` | |
| UI that wants the raw shape | `apps/desktop/public/mark.svg` | transparent, `currentColor` |
| Docs, site, README | `brand/svg/*.svg` | |

### The tray needs both polarities

Windows draws the notification area glyph over a taskbar that follows the system
theme. A single light glyph disappears the moment someone switches to the light
theme, so `tray.rs` has to read the theme and pick between `tray-dark` (light
glyph, dark taskbar) and `tray-light` (dark glyph, light taskbar), and swap when
the theme changes at runtime. Tauri's `Image::from_path` wants PNG on Windows, so
use the `.png` pair from Rust; the `.ico` pair is there for anything that needs
a multi-size handle.

Both polarities are verified legible at 16px — the size the brand brief calls the
hard constraint.

### tauri.conf.json

The bundle list, for when `apps/desktop/src-tauri/tauri.conf.json` is written:

```json
"icon": [
  "icons/32x32.png",
  "icons/128x128.png",
  "icons/128x128@2x.png",
  "icons/icon.icns",
  "icons/icon.ico"
]
```

Do not run `tauri icon` or let `tauri init` scaffold this directory — both
overwrite the generated set with Tauri's default artwork.

## There is no vector wordmark

`docs/brand.md` locks the mark but not the typeface, so a logotype would be
inventing a decision that has not been made. The wordmark is set in the app's own
UI font by `Lockup.tsx` instead. When a typeface is chosen, that component is the
only thing that changes.
