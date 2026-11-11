---
name: kicad-hierarchy
description: |
  Hierarchical sheet and bus workflow for KiCAD schematics via MCP tools. Triggers on:
  "hierarchical sheet", "sub-sheet", "split the schematic", "sheet pins", "hierarchical label",
  "multi-page schematic", "page numbers", "reuse a sheet", "add a bus", "bus entry",
  "fan out to a bus", "page size".
argument-hint: "[sheet or bus task]"
---

# KiCAD Hierarchy & Bus Workflow

Structure a schematic into sheets and buses through Konnect MCP tools. These
tools edit schematic files directly and do not need KiCad running. If KiCad has
the schematic open, ask the user to reload it after the edits.

## Toolset loading

```
load_toolset('sch_hierarchy')    # sheets, sheet pins, page size and numbering
load_toolset('sch_bus')          # buses, bus entries, pin fan-out
load_toolset('sch_wiring')       # labels and wires on either side of a sheet pin
load_toolset('sch_export')       # ERC
```

## 1. Read the existing structure

Call `get_sheet_hierarchy` on the root schematic first. It returns each sheet's
name, file, position, size, page and pins, nested by parent. Decide from it
whether the task adds a sheet, changes one, or only fixes pins.

## 2. Create or link a sheet

- `add_hierarchical_sheet` inserts a sheet block in a parent and links it to a
  child file. It creates the child if it is missing and links to it if it
  exists, which is how one child is reused as several instances.
- `create_schematic` makes a standalone blank schematic. `set_schematic_page`
  sets its paper size. Content outside the page frame still nets up and still
  exports, so a page that is too small is a silent defect: check the size
  returned against the sheet's contents.
- `duplicate_sheet` copies a sheet and its child file under a new name, when
  the copy must diverge from the original.
- `edit_sheet` renames, resizes, moves or repoints a sheet. `move_sheet` only
  moves it. `delete_sheet` removes the block and keeps the child file.

After adding, duplicating or deleting sheets, call `renumber_sheet_pages` so
page numbers are sequential again.

## 3. Connect a sheet to its parent

A signal crosses a sheet boundary through a hierarchical label inside the child
and a sheet pin of the same name on the parent's sheet block.

1. Place the hierarchical labels in the child.
2. Call `import_sheet_pins` on the parent's sheet block. It creates a pin for
   every label that has none, on the edge you choose. This is the primary way
   pins are created.
3. Use `add_sheet_pin`, `edit_sheet_pin` and `delete_sheet_pin` only for
   single corrections. A pin must sit on the edge its side names; an off-edge
   position is refused.
4. Wire or label the parent side of each pin.

Call `validate_sheet_pins` when done. It is read-only and reports labels
without pins, pins without labels, and pins stored off their edge. It repairs
nothing; fix each finding with the pin tools and validate again.

## 4. Buses

A bus carries the members its label names, written as a range such as
`DATA[0..7]` or a group such as `{SDA SCL}`. Membership is by name.

- `add_bus` and `batch_add_bus` draw bus segments.
- A wire that merely touches a bus is not connected to it. Each wire joins
  through `add_bus_entry`, and the wire needs a label naming its member.
- `connect_pins_to_bus` does all three for a set of pins: a wire stub, a bus
  entry, and the member label for each pin. Prefer it over drawing entries by
  hand.

## 5. Verify

Run `run_erc` on the root schematic. Unmatched sheet pins, dangling hierarchical
labels and bus-to-wire conflicts appear there. Then call `get_sheet_hierarchy`
once more and confirm the tree matches what was asked for.

Completion criterion: `validate_sheet_pins` reports no finding, ERC reports no
hierarchy or bus error, and page numbers are sequential.
