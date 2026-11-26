# restorekit brand

restorekit fixes macs with a cable and an open hardware board, so the brand looks like
the board. The page is soldermask green with white silkscreen type, copper traces
showing through the mask, and gold pads for anything you can press.

Tokens live in `src/app.css`. Use them instead of raw hex values.

## Color

| Token        | Hex       | On a board          | Use                                          |
| ------------ | --------- | ------------------- | -------------------------------------------- |
| `mask`       | `#103d2e` | soldermask          | page background                              |
| `mask-deep`  | `#0b2d21` | mask over ground    | alternate sections, code blocks, 3D wells    |
| `copper`     | `#1f5c45` | trace under mask    | rules, panel borders, hero traces            |
| `copper-hi`  | `#2b7457` | via ring            | input borders, secondary button borders      |
| `silk`       | `#f1f0e6` | silkscreen          | headings, emphasized text                    |
| `silk2`      | `#c4d3c8` |                     | body text                                    |
| `silk3`      | `#92b1a1` |                     | captions, fine print, table row labels       |
| `gold`       | `#dcb660` | ENIG pad            | links, primary buttons, step numbers         |
| `gold-ink`   | `#10301f` |                     | text on gold                                 |

Gold is the only accent, and it always means you can press it (or it's a step
number). Don't use it for decoration or to color a single word in a heading.

### Light mode (desktop app)

The desktop app follows the system appearance. Light mode is the same board in
white soldermask: `#f4f5ef` background, `#10301f` ink, `#cbd8cd` lines, and a
darker gold `#8a6414` so it passes contrast on white. Tokens live in
`apps/desktop/src/lib/theme.css`.

## Type

- **Archivo** (variable, self-hosted) for everything that isn't code.
  - Headings use the expanded width (`font-stretch: 125%`), bold to extra bold.
  - Body text stays at normal width, 17px, line height 1.6.
- **Spline Sans Mono** only for real code and machine data: commands, flags, file
  names, code blocks, and in the app ECIDs, serials, identifiers and logs (`.mono`).

| Class       | Size                   | Use                                   |
| ----------- | ---------------------- | ------------------------------------- |
| `t-display` | 36 to 56px, 800        | the hero headline, once per page      |
| `t-h2`      | 26 to 38px, 700        | every section heading                 |
| `t-h3`      | 19px, 700              | sub-headings, step titles             |
| `t-step`    | 44px, 800, gold        | numbers on steps that are a sequence  |
| `t-lead`    | 18px                   | the paragraph right under a heading   |
| `t-data`    | 15.5px                 | tables and definition lists           |
| `t-small`   | 14px, `silk3`          | captions and fine print               |

Sentence case everywhere. No all-caps labels, no eyebrow text above headings, no
middle-dot strings (`A · B · C`), no arrow bullets.

## Shapes

- Controls (buttons, inputs, copy buttons) are pads: 5px radius.
- Panels, images and 3D wells: 8px radius (`rounded-lg`) with a `copper` border.
  Screenshots get a `silk/15` border instead so the dark app UI has an edge.
- Lists use a small gold pad (7 × 11px, 2px radius) as the bullet.

## Buttons

- `btn-pad` is gold and is for the main action in a section. One per section at most.
- `btn-line` is an outlined button for everything else.
- Labels say what happens: "Install restorekit", "Join the batch 2 list",
  "Email frank@restorekit.org". Not "Get started" or "Learn more".

## Layout

- Max width `6xl` (1152px), 20px side padding, left aligned.
- Sections alternate `mask` and `mask-deep`, with no borders between them.
- Body copy tops out around 62 characters (`max-w-[62ch]`).
- Two-column sections put text on the left and the picture, table or code on the right.

## Silkscreen details

The board metaphor shows up in a few places and nowhere else:

- Copper traces with vias behind the hero, drawn at 45° like real routing.
- Callouts on the dongle render pointing at each port, in plain words
  ("To your computer", "To the mac you're fixing"). Never use board jargon like
  reference designators (J1, U3) on the site.
- Gold pad bullets.

Don't add more of these. No circuit patterns behind other sections, no component
labels.

## Motion

The 3D scenes are the only ambient motion. No entrance animations, no pulsing
dots, no blinking cursors. Hover changes color only. Reduced motion turns
everything off.

## Voice

Write like the README: first person, casual, plain sentences. Lowercase mac,
linux, windows and apple in prose. "bam!" and "I think this sucks" are on brand.

- No em dashes, no arrow chains, no colons joining two clauses.
- No marketing lines ("X, handled", "So we built it", rhetorical questions).
- Say what the thing does with a real number or name, not how it feels.
- Stay fair to competitors. Link to them and use their published numbers.
