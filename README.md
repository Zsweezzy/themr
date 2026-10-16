# themr

One window to set the colors of all your apps at once — or one app at a time.

Instead of hunting hex codes across `kitty.conf`, `waybar/style.css`, `hyprland.lua`,
`btop.conf` and friends every time you change your theme, you write a **variable** once:

```conf
background {{background}}
foreground {{foreground}}
accent     {{primary}}
```

and themr fills in the real values, keeps track of what it wrote, and rewrites it every
time you switch themes. Global theme, per-app overrides, community JSON imports — all in
one GUI.

## How it works

1. **Either read on Apply.** A file that already uses the current theme's exact hex values
   is *adopted* automatically — the values are left alone and themr starts tracking them
   (the status line says how many). No markup needed. Any file can also contain
   `{{token}}`, where *token* is a color name from the theme (`{{primary}}`, `{{background}}`, …).
2. **themr replaces markers with hex values** on Apply, and remembers what it wrote for each
   file in `~/.config/themr/state.json`.
3. **Switch theme → Apply →** the hex values themr tracks are swapped for the new
   theme's values. Hex you never put through themr is never touched, so `#000000` you
   meant literally stays put.
4. **Before the first edit of any file, themr copies it** to `<file>.themr.bak`, once,
   and never overwrites that backup. The **Restore** button puts the original back and
   stops tracking it.

Works for any app: if it has a text config with colors, themr can drive it.

## Install

Needs Rust, GTK 4.10+ and its development files (`gtk4-devel` / `libgtk-4-dev`).

```bash
git clone https://github.com/Zsweezzy/themr
cd themr
cargo build --release
install -Dm755 target/release/themr ~/.local/bin/themr

# desktop entry, so it shows up in your app menu
sed "s|^Exec=themr|Exec=$HOME/.local/bin/themr|" themr.desktop \
  > ~/.local/share/applications/themr.desktop
update-desktop-database ~/.local/share/applications
```

Run it:

```bash
themr
```

Or open it from your app menu (search "themr").

Or bind a key in your compositor, e.g. Hyprland:

```lua
hl.bind("SUPER", "T", hl.dsp.exec_cmd("themr"))
```

## Window

- **Theme for all apps** — pick the global theme, press *Apply to all apps*.
- **Each app row** — shows its files and its effective theme, with its own dropdown
  (`Global (…)` or a specific theme), *Apply*, *Restore*, and *✕* to remove it from themr.
- **Import JSON…** — load a community theme from a `.json` file.
- **Add app** — a name plus one config file path per line (`~` works).
- **Colors of &lt;theme&gt;** — swatch per token, edits save to disk instantly; press an
  Apply button to push them into the apps.

## Files themr owns

| Path | Purpose |
| --- | --- |
| `~/.config/themr/config.toml` | global theme, apps, per-app overrides, files |
| `~/.config/themr/themes/*.json` | themes (4 bundled: Tokyo Night Moon, Catppuccin Mocha, Gruvbox Dark, Nord) |
| `~/.config/themr/state.json` | what themr wrote, so it can rewrite it later |
| `<your config>.themr.bak` | pristine copy taken before the first edit |

`config.toml`:

```toml
theme = "tokyonight-moon"        # the global theme

[apps.kitty]
files = ["~/.config/kitty/current-theme.conf"]
theme = "catppuccin-mocha"      # optional per-app override

[apps.waybar]
files = ["~/.config/waybar/style.css"]
```

## Theme format

`colors` maps token names to hex. A flat map also works, so most community palettes
import as-is:

```json
{
  "name": "My Theme",
  "colors": {
    "background": "#1e2030",
    "foreground": "#c8d3f5",
    "primary": "#82aaff",
    "accent": "#bb9af7"
  }
}
```

Any token name works as long as your configs use the same name. The bundled themes share
this set: `background`, `foreground`, `surface`, `overlay`, `muted`, `red`, `green`,
`yellow`, `blue`, `magenta`, `purple`, `cyan`, `orange`, plus the synonyms `primary`
(=`blue`), `accent` (=`magenta`), `black` (=`overlay`) and `white` (=`foreground`).
Adoption prefers the palette name over a synonym, so a config follows `foreground` and
never `white`.

`#rgb`, `#rrggbb` and `#rrggbbaa` are accepted and normalized to lowercase.

## Known limits

- themr rewrites the hex it tracks, so a color you set by hand in a tracked file is
  left alone, but a *value* that two different tokens share can't be told apart — themr
  picks one and says so in the terminal.
- Adoption only recognises values that exactly match the theme, case-insensitively. A
  file whose palette is a *different* variant of the same theme (e.g. Tokyo Night Storm
  hexes against a Tokyo Night Moon theme) adopts only the few colors that coincide; the
  rest need `{{token}}` markers to join in.
- Colors written as function calls rather than hex (QML `Qt.rgba(…)`, CSS `rgba(…)`) are
  never touched. Give those a token by hand if the app accepts one.
- Plus and minus (`#000000` vs `#000000ff`) are treated as different colors, on purpose.
- Applying only rewrites files. Reloading the app is up to the app: some pick it up
  automatically (Quickshell hot-reloads a saved config), others need a restart or a
  signal.

## Development

```bash
cargo test      # 23 unit tests over parsing, rewriting, adoption, backups, per-app overrides
cargo clippy --all-targets -- -D warnings
```

MIT licensed.
