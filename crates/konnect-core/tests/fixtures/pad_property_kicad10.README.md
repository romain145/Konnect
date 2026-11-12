# Pad fabrication property footprints

Footprints whose pads carry `(property pad_prop_…)`, KiCad's fabrication
property: BGA ball, fiducial, test point, heatsink, castellated, mechanical or
press-fit. They are the oracle for the one mapping between the names Konnect's
tools use (`heatsink`) and the tokens a footprint file holds
(`pad_prop_heatsink`), in `src/tools/pad_fabrication.rs`.

## Stock footprints (#193)

The two footprints `update_pcb_from_schematic` placed without their pad
properties on a real board, without saying so.

Copied byte for byte from the KiCad 10.0.8 standard library on Windows 11.
Nothing was edited, and the repository stores the directory `-text`, so the
bytes (including CRLF line endings) are the library's.

| Fixture | Library file | SHA-256 (first 16) |
|---|---|---|
| `pad_property_wqfn28_kicad10.kicad_mod` | `Package_DFN_QFN.pretty/WQFN-28-1EP_4x4mm_P0.4mm_EP2.7x2.7mm.kicad_mod` | `e4b9dd4412724316` |
| `pad_property_usb_c_hctl_kicad10.kicad_mod` | `Connector_USB.pretty/USB_C_Receptacle_HCTL_HC-TYPE-C-16P-01A.kicad_mod` | `7800cae80002bb88` |

- The WQFN has 28 signal pads, an exposed pad numbered 29 that is
  `pad_prop_heatsink` (and `(zone_connect 2)`), and unnumbered paste-only pads
  under it. No other pad has a property.
- The USB-C receptacle has 16 signal pads, two unnumbered `np_thru_hole` pads,
  and four `thru_hole oval` shield pads that all share the number `SH` and are
  each `pad_prop_mechanical`.

The file name inside a `.pretty` directory has to equal the footprint name, so
a test that resolves them through a library table writes them out under their
library names.

## One footprint with every property

Written by KiCad 10.0.8's own footprint writer on Windows 11, through the
Python bindings it ships (`pcbnew.PCB_IO_KICAD_SEXPR`):
`FootprintLoad(library, name, True)` to keep UUIDs, `PAD.SetProperty`, then
`FootprintSave`. Nothing was edited by hand.

The footprint is `Connector_PinHeader_2.54mm.pretty/PinHeader_1x08_P2.54mm_Vertical`:
eight `thru_hole` pads numbered 1 to 8, none with a property in the library.

| Fixture | Change made in KiCad | SHA-256 (first 16) |
|---|---|---|
| `pad_property_none_kicad10.kicad_mod` | none (load and save) | `a4ec5562d0276a84` |
| `pad_property_each_kicad10.kicad_mod` | pad *n* set to the *n*th `PAD_PROP` value | `03386df1fbf84974` |

`pcbnew` 10.0.8 defines `PAD_PROP_NONE = 0`, then in order the eight values
below, which is the pad each one is on:

| Pad | `pcbnew` constant | Token KiCad wrote | Konnect name |
|---|---|---|---|
| 1 | `PAD_PROP_BGA` | `pad_prop_bga` | `bga` |
| 2 | `PAD_PROP_FIDUCIAL_GLBL` | `pad_prop_fiducial_glob` | `fiducial_glob` |
| 3 | `PAD_PROP_FIDUCIAL_LOCAL` | `pad_prop_fiducial_loc` | `fiducial_loc` |
| 4 | `PAD_PROP_TESTPOINT` | `pad_prop_testpoint` | `testpoint` |
| 5 | `PAD_PROP_HEATSINK` | `pad_prop_heatsink` | `heatsink` |
| 6 | `PAD_PROP_CASTELLATED` | `pad_prop_castellated` | `castellated` |
| 7 | `PAD_PROP_MECHANICAL` | `pad_prop_mechanical` | `mechanical` |
| 8 | `PAD_PROP_PRESSFIT` | `pad_prop_pressfit` | `pressfit` |

## An SMD pad (#195)

Written the same way from `Resistor_SMD.pretty/R_0603_1608Metric`, whose two
`smd roundrect` pads have no drill, to show where the clause goes without one.

| Fixture | Change made in KiCad | SHA-256 (first 16) |
|---|---|---|
| `pad_property_smd_none_kicad10.kicad_mod` | none (load and save) | `33eb180463ba5d08` |
| `pad_property_smd_heatsink_kicad10.kicad_mod` | pad 1 set to `PAD_PROP_HEATSINK` | `5f6c702bdc263a00` |

## What they show

- KiCad writes the property as one bare atom, `(property pad_prop_heatsink)`,
  on its own line inside the pad: after `(size …)` and `(drill …)`, before
  `(layers …)`.
- A pad with `PAD_PROP_NONE` has no `property` clause at all. Removing the
  eight clauses from `pad_property_each_kicad10` gives the pads of
  `pad_property_none_kicad10` exactly, UUIDs included.
- As in `pad_zone_connect_kicad10.README.md`, KiCad gives the mandatory fields
  new UUIDs on each save, so tests compare pads rather than whole files.
