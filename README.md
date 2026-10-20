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
   A line that names a color by a token (`background #222436`, `foreground = "#c8d3f5"`)
   is tracked under that name too, so it follows the token even when its value is not in
   the palette — this is what makes, say, kitty's `background` line respond to the
   `background` token. Known config keys that mean a token bind the same way: btop's
   `theme[main_bg]`, rofi's `text-color`/`background-color`, and the rest of their
   palettes.
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
- **New theme** — name it and it starts as a copy of the theme you are editing; tweak the
  colors, then Apply.
- **Add app** — a name plus one config file path per line (`~` works).
- **Detect apps** — adds every app themr ships a preset for whose config actually exists
  on this machine, skipping the ones already listed. Presets only seed a *new* config, so
  this is how an existing config picks up apps added since. Press *Apply to all apps* after.
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
reload = "pkill -SIGUSR1 -x kitty || true"   # optional: run after a write

[apps.waybar]
files = ["~/.config/waybar/style.css"]
reload = "pkill -SIGUSR2 -x waybar || true"
```

An app whose config only *names* a theme (btop's `color_theme`, rofi's `@theme`) keeps its
colours in a separate file — list that file too and themr recolors it in place:

```toml
[apps.btop]
files = ["~/.config/btop/btop.conf", "~/.config/btop/themes/tokyo-night-moon.theme"]

[apps.rofi]
files = ["~/.config/rofi/config.rasi", "~/.local/share/rofi/themes/tokyonight-moon.rasi"]
```

### Bundled app presets

themr knows the usual config location and reload command for these apps. A preset is only
added when its file exists, so the ones you do not run cost nothing. Paths with several
entries list every one that exists. Every reload command is gated on the app actually
running, so an app that is not running is a silent no-op (and a daemon client that would
otherwise wait forever for its daemon can never hang an Apply).

| App | Config file(s) | Reload |
| --- | --- | --- |
| kitty | `~/.config/kitty/current-theme.conf` | `pkill -SIGUSR1 -x kitty` |
| alacritty | `alacritty.toml`, `alacritty.yml` | watches its own config |
| wezterm | `wezterm.lua` | watches its own config |
| foot | `foot.ini` | next launch |
| ghostty | `config` | next launch |
| waybar | `style.css`, `config.jsonc`, `config` | `pkill -SIGUSR2 -x waybar` |
| eww | `eww.scss`, `eww.css`, `bar/eww.scss` | `eww reload` |
| polybar | `config.ini`, `config` | `polybar-msg cmd restart` |
| ags | `style.scss` | next launch |
| rofi | `config.rasi` | next launch |
| wofi | `style.css` | next launch |
| fuzzel | `fuzzel.ini` | next launch |
| tofi | `config` | next launch |
| anyrun | `style.css` | next launch |
| dunst | `dunstrc` | `dunstctl reload` |
| mako | `config` | `makoctl reload` |
| swaync | `style.css` | `swaync-client -rs` |
| hypr | `hyprland.conf`, `hyprland.lua` | `hyprctl reload` |
| niri | `config.kdl` | niri reloads a saved config itself |
| sway | `config` | `swaymsg reload` |
| i3 | `config` | `i3-msg reload` |
| hyprlock / swaylock | `hyprlock.conf` / `swaylock/config` | next lock |
| wlogout | `style.css` | next launch |
| qt | `~/.config/Trolltech.conf` | next launch |
| kde | `~/.config/kdeglobals` | restart the app |
| konsole | `~/.local/share/konsole/*.colorscheme` | next tab |
| Xresources | `~/.Xresources` | `xrdb -merge ~/.Xresources` |
| btop / cava | `btop.conf` / `cava/config` | next launch |
| starship | `starship.toml` | next prompt |
| yazi | `theme.toml` | next launch |
| zellij | `config.kdl` | next launch |
| tmux | `tmux.conf` / `~/.tmux.conf` | `tmux source-file <your file>` |
| lsd / lf / bat / eza | `colors.yaml` / `colors` / `config` / `theme.yml` | next launch |
| zathura | `zathurarc` | next launch |

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
  left alone. Two *different tokens* can still share one value, though: a theme can give
  e.g. `magenta` and `purple` the same hex (Nord does). Value-bound lines (whose key is
  not a token name, like waybar's `#custom-gpu`) then can't be told apart — themr picks
  one and says so in the terminal, so a round trip through such a theme can leave such a
  line on its sibling's hue. Lines whose key *is* a token name are set line by line and
  are never affected.
- A line whose key is a token name (`background …`, `foreground = …`) now always follows
  that token, so its old value is rewritten on the first Apply. That is the one case where
  applying can change a hex themr has not written before.
- Adoption only recognises values that exactly match the theme, case-insensitively. A
  file whose palette is a *different* variant of the same theme (e.g. Tokyo Night Storm
  hexes against a Tokyo Night Moon theme) adopts only the few colors that coincide; the
  rest need `{{token}}` markers to join in.
- themr rewrites the colour syntaxes the real configs use: `#rgb`/`#rrggbb`, CSS
  `rgb(r, g, b)`/`rgba(r, g, b, a)`, hypr's `rgba(RRGGBBaa)`/`0xAARRGGBB`, and the bare
  decimal `R,G,B` triples KDE and Konsole write. A triple needs exactly three 0-255
  components, so `WindowGeometry=100,50,800,600`, `34,36,54,100`, `1.5,2,3` and
  `translate3d(0,0,0)` are never mistaken for a colour. Alpha is
  kept as-is. A `#rrggbbaa` in a *config* is treated as a colour of its own and is not
  touched by a `#rrggbb` binding.
- Plus and minus (`#000000` vs `#000000ff`) are treated as different colors, on purpose.
- No GTK preset, on purpose. A `gtk.css` is usually a whole *generated* theme with
  hundreds of distinct colours; themr maps colours onto a ~20-token palette, so pointing
  it at one collapses every accent onto the background colour and the UI goes flat. If
  your `gtk.css` is a small hand-written override it is fine to add by hand.
- KDE and Konsole are themed through their *keys*, because an `R,G,B` triple says nothing
  about what it means: `BackgroundNormal`, `ForegroundLink`, and Konsole's
  `[Background]`/`[Color1]` sections map onto palette tokens, so a KDE app follows the
  theme even when its current scheme shares no colour with it. Only `[Colors:Selection]`
  and `[Colors:Header]`/`[Colors:Complementary]` are treated differently, since their
  backgrounds are the highlight and the raised surface. An 18-token palette is coarser
  than a KDE scheme, so the six background shades collapse into `background`/`surface`.
  `~/.local/share/color-schemes/*.colors` files are *not* in the preset: they are the
  source `plasma-apply-colorscheme` copies *over* `kdeglobals`, so theming them would
  fight the app, and a scheme you are not using would be recoloured too. Konsole's
  wildcard themes every scheme in that directory; list one file instead if you want to
  keep another scheme as it is.
- Applying rewrites files, then runs the app's `reload` command if it has one, so the
  change is visible without a manual restart. Kitty needs `pkill -SIGUSR1 -x kitty`,
  waybar `pkill -SIGUSR2 -x waybar`, Hyprland `hyprctl reload`; alacritty, wezterm and niri
  watch their own config; Quickshell hot-reloads on its own, and btop only reads its theme
  at startup (restart it). An app with no `reload` is just written and left alone. The
  command runs through the shell, so `|| true` is honoured — use it when "app not running"
  should not be reported as a failure.

## Development

```bash
cargo test      # 48 unit tests over parsing, rewriting, adoption, key lines, reloads, presets, backups, per-app overrides
cargo clippy --all-targets -- -D warnings
```

MIT licensed.
