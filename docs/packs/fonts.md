# Fonts

`font/` holds the fonts the text is drawn in. A font is two files that
share a two-digit hex id: the **sheet**, an image of every glyph in a grid
of cells, and its **description**, which says how to read the sheet.

```text
font/00.bmp     a sheet
font/00.xml     its description
font/01.bmp
font/01.xml
```

Sheet formats, highest first: `webp`, `png`, `bmp`, `tim`. A sheet without a
description is not drawn. A mod may replace a sheet, a description or
both, or add a font under a new id.

## Which font is drawn

Every font is in one of two **scripts**, the byte spaces text is encoded
in: `latin` or `japanese`. The language picks the script: `ja` and `ja-*`
use Japanese, every other language Latin. The session then draws the
lowest font id of that script in the pack, or the lowest id of any script
if the pack has none.

So a mod that adds `font/02` in the Latin script is not drawn while a
`font/00` in that script exists; to replace that font, replace `font/00`.

## The sheet

The sheet is a grid of cells, 14 pixels high. Each byte of the encoded text
draws one cell, and the bytes `0xF8`, `0xF9` and `0xFA` pick a further page
of cells with the byte after them.

A sheet may use at most 255 opaque colours: text is coloured by palette
index, so a true-colour image is turned back into indexed colour, colour for
colour. A replacement sheet keeps the size of the one it replaces.

A character the sheet has no cell for is drawn as a small hollow box.

## The description

```xml
<font format="1" script="latin" advance="8" cellWidth="14"
      margin="0x30" textY="181" menuY="186">
  <glyph cell="0x02" advance="14"/>
  <glyph cell="0x05" advance="5" indent="3"/>
  <glyph cell="0x0A" advance="7" indent="1"/>
</font>
```

| Attribute | Meaning |
|---|---|
| `format` | `1` |
| `script` | `latin` or `japanese`: how the sheet's cells are encoded |
| `advance` | how far the pen moves after a glyph, in pixels, unless a `<glyph>` says otherwise |
| `cellWidth` | the width of a cell on the sheet |
| `margin` | the message window text's left margin |
| `textY`, `menuY` | the line the message window's text, and a menu's text, is drawn on |
| `yesX`, `noX`, `cursorX` | optional, all three together: draw a yes/no question's choices on its last line, `Yes` at `yesX`, `No` at `noX` and the cursor `cursorX` before its label |

Each `<glyph>` gives one cell its own width:

| Attribute | Meaning |
|---|---|
| `cell` | the cell: a plain byte, or `0xEA + nn` for `0xF8 nn` |
| `advance` | how far the pen moves after it |
| `indent` | how many pixels of the cell's left edge to skip; default `0` |

Every other cell, and every cell of the further pages, uses the font's
`advance` with no indent. Numbers are decimal or `0x` hex. Unknown elements
and attributes are refused, so a typo fails instead of drawing at the wrong
width.

## Latin text on a Japanese sheet

When the language is not Japanese but the font drawn is a Japanese-script
one, the engine measures each cell's ink and draws it tight: a cell's
indent is its first inked column, and its advance its inked width plus one,
at most 14. The space advances 8 pixels, and the digits keep the whole cell
so that counters stay aligned. Japanese text always draws whole cells.

## Previewing

```sh
arklay re1.akpak --ui font --language en-US --capture font.bmp
```

draws a sample of the font the session's language would use, so a
replacement sheet can be checked without starting a game.
