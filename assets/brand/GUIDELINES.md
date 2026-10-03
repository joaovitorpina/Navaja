# Navaja brand guidelines

## The mark

**Concept:** a folding pocket knife (*navaja*) seen from the side, with its tools fanned out from a single pivot. Each blade stands for a kind of tool: a blade, a screwdriver tip, a pick and a hook. Together they read as "many tools, one handle".

**Shape:**
- A rounded handle and 3-5 fanned tools.
- One visible pivot rivet.
- Drawn on a 24 px grid in geometric strokes and fills, so it stays legible at 16 px.

**Master files:**
- `navaja.svg` for the app icon;
- `tray-template.svg` and `tray-color.svg` for the tray icons.

Every raster size and platform icon is generated from them by `cargo xtask icons`, which wraps `tauri icon`. PNG, ICO and ICNS files are never edited by hand.

**Do not:**
- Use a cross, a shield, a cross-and-shield emblem, or a red handle with a white emblem. The mark must never resemble Swiss Army knife branding.
- Copy, trace or closely imitate any existing knife maker's logo or silhouette.
- Show a blade pointing at the viewer, or blood or weapon imagery. The knife is a tool, not a weapon.

## Palette tokens

These are design tokens. The front end defines the same names as CSS custom properties.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--nv-steel-900` | `#1F2328` | `#E6E8EB` | Primary text, mark outline |
| `--nv-steel-600` | `#57606A` | `#9DA5AE` | Secondary text, handle |
| `--nv-steel-200` | `#D0D7DE` | `#30363D` | Decorative dividers only |
| `--nv-steel-50`  | `#F6F8FA` | `#0D1117` | Surfaces; text and icons on brass or danger fills |
| `--nv-brass-500` | `#975A16` | `#D69E2E` | Accent: pivot rivet, focus ring, primary action |
| `--nv-danger`    | `#CF222E` | `#F85149` | Destructive actions only (kill, stop) |

**Rules:**
- The accent is brass, never red.
- Red appears only on destructive controls and is never paired with a white cross or shield shape.
- Text and icons on a brass or danger fill use `--nv-steel-50`, never white. White fails contrast on the dark-theme fills.
- Outlines that identify an input, the editor or another control use `--nv-steel-600`. `--nv-steel-200` is too faint for that (under 3:1).

**Contrast:** these pairings meet WCAG AA, checked 2026-10-02.

| Pairing | Light | Dark |
|---|---|---|
| Body text (`--nv-steel-900` on `--nv-steel-50`) | 14.8:1 | 15.4:1 |
| Brass as text on `--nv-steel-50` | 5.2:1 | 7.9:1 |
| `--nv-steel-50` on a brass fill | 5.2:1 | 7.9:1 |
| `--nv-steel-50` on a danger fill | 5.0:1 | 5.7:1 |
| Control outlines (`--nv-steel-600`) | 6.0:1 | 7.6:1 |

## Tool icons (`tools/<id>/icon.svg`)

**Geometry:**
- Use a 24 × 24 `viewBox` with 1.5 px strokes, round caps and joins, and no fills unless needed for legibility.
- Draw everything with `currentColor`, so the shell can theme the icon through a CSS mask.

**Allowed SVG:**
- **Elements:** `svg`, `g`, `path`, `circle`, `ellipse`, `rect`, `line`, `polyline`, `polygon`.
- **Not allowed:** `script`, `style`, `foreignObject`, `image`, `use`, event attributes, or any `href`. The registry rejects icons that break these rules.

**Style:** one concept per icon, recognisable at 16 px, with no text inside the icon.

## Tray icons

| OS | Asset |
|---|---|
| macOS | `tray-template.svg`, rendered as a monochrome **template image** (black plus alpha) at 18 pt (@1x/@2x), so macOS can tint it for light and dark menu bars |
| Windows | `tray-color.svg`, rendered as a multi-size `.ico` (16, 20, 24, 32, 40, 48 px). It must be crisp at 100-200 % scaling |
| Linux | `tray-color.svg`, rendered as PNG at 22, 24 and 32 px, plus hicolor theme icons for the `.desktop` file |

## Copy

- **Name:** Navaja, always capitalised. The binary is `navaja`.
- **Descriptor:** "offline developer toolbox".
- **Never say:** "Swiss Army knife", "navaja suiza", or any comparison to a specific knife brand.
- **Tone:** plain and factual. Say what a tool does and what it never does, for example "never leaves your machine".
