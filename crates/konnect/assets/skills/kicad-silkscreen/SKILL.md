---
name: kicad-silkscreen
description: |
  Silkscreen clean-up workflow for KiCAD boards via MCP tools. Triggers on: "silkscreen",
  "silk warnings", "silk over copper", "silkscreen clipped by solder mask", "move reference
  designators", "tidy the refs", "hide the values", "board text", "resize the title text",
  "text overlaps pads".
argument-hint: "[board or DRC rule to clear]"
---

# KiCAD Silkscreen Clean-up Workflow

Clear silkscreen findings by moving, rotating, shrinking, or hiding text in
place on the board KiCad has open. A fix counts only when a fresh DRC of the
saved board no longer reports the finding.

## Toolset loading

```
load_toolset('pcb_board')        # text and field listing/editing, board text
load_toolset('pcb_components')   # pad geometry
load_toolset('verification')     # DRC
```

Every tool in this workflow that reads or edits text is live-only: KiCad must be
running with the exact board open.

## 1. Establish the findings

Save the board, then call `run_drc`. It reads the saved file, so an unsaved
board gives stale results. Collect the findings whose rule is about silkscreen
and note, for each, the offending item's UUID and its owner. Read the project's
minimum text height and thickness with `get_design_rules` before planning any
resize.

## 2. Measure before moving

- `list_footprint_fields` returns every field with its board-absolute position,
  absolute angle, glyph size, layer, visibility, and the bounding box KiCad
  measures. Filter by layer and visibility to get the text that is really
  printed.
- `list_board_texts` returns the same for board-level text, including the raw
  string with any text variable left unexpanded.
- `get_component_pads` returns pad centres, sizes and rotations. A rotated pad's
  obstacle is the axis-aligned box of the rotated rectangle.

Plan from bounding boxes, not from anchor points: a field anchor is the text
centre only for centre-aligned text.

## 3. Choose a new place

For each offending text, in order of preference:

1. Move it, at its current size and angle, to the nearest free spot beside its
   own footprint.
2. Rotate it by 90 degrees if that opens a spot.
3. Shrink it, never below the project's minimum text height and thickness.
4. Hide it, only when the user accepts an unlabelled part.

A free spot clears, on the same board side: every pad plus the solder-mask
expansion, through-hole pads from both sides, other visible silkscreen text,
the body area of every footprint (text under a part is invisible once the board
is assembled), and the board edge. Prefer a spot whose nearest footprint is the
text's own, so the label stays unambiguous. Place the largest texts first and
treat each placed text as an obstacle for the next.

## 4. Apply and verify

- `edit_footprint_field` changes a reference or value field. Coordinates are
  board-absolute millimetres and the rotation is the text's absolute angle, the
  same frame the listing reports.
- `edit_board_text` changes a board text selected by UUID. It preserves the
  font face and the raw variable text.
- Both tools also set `mirrored`, `horizontal_alignment`, `vertical_alignment`,
  `bold`, `italic`, `keep_upright`, `knockout`, `locked` and `font`. Text on a
  back layer must be mirrored to read correctly from the back; changing `layer`
  alone does not mirror it. Pass `mirror: "auto"` to set the flag from the side
  of the layer the text ends on (with `layer`, or alone for text already moved).

After each edit, compare the returned bounding box centre with the intended
centre and correct the position once if they differ. Then save, call `run_drc`
again, and confirm the silkscreen findings are gone and no new finding
appeared.

## Known limits

- An edit to a footprint field rewrites the whole footprint through KiCad's
  API. In KiCad 10.0 this drops the footprint's symbol-unit mapping and can
  rewrite a rule area defined inside the footprint, which then shows up as a
  library-mismatch warning. Tell the user, and recommend "Update PCB from
  Schematic" afterwards. Editing board text has no such effect.
- Shrinking text set in a TrueType font can fall below the minimum stroke
  weight and raise a text-thickness warning; prefer moving such text.
- There is no tool that edits a footprint's own silkscreen graphics in bulk;
  use `edit_board_footprint_graphic` for one graphic at a time.
- New board text is added with `add_board_text`, and artwork with
  `import_svg_logo`. Remove board-level text with `delete_graphics`, after a
  dry run.

Completion criterion: the saved board's DRC reports no silkscreen finding, and
every side effect listed above has been reported to the user.
