# Duplicate reference fixture

`duplicate_reference_kicad10.kicad_sch` places two `Device:R` symbols that
are both `R4`, as a sheet looks before annotation or after a copy-paste, and
one `R5`. The pin-naming wiring tools must refuse `R4` rather than wire
whichever copy comes first in the file (#98). `wiring_guard_tests.rs` drives
it.

## Provenance

The sheet was written as text by a script, with the `Device:R` definition
taken from `batch_connect_kicad10.kicad_sch`, then force-resaved by KiCad
10.0.8 on Windows:

```text
kicad-cli sch upgrade --force duplicate_reference_kicad10.kicad_sch
```

The file is that Eeschema serialization, byte for byte: `(generator
"eeschema")`, `(version 20260306)`, CRLF line ends. KiCad kept both `R4`
references.

| Part | Placement | Pin 1 | Pin 2 |
|---|---|---|---|
| R4 | (180.34, 120.65) | (180.34, 116.84) | (180.34, 124.46) |
| R4 | (200.66, 120.65) | (200.66, 116.84) | (200.66, 124.46) |
| R5 | (190.5, 139.7) | (190.5, 135.89) | (190.5, 143.51) |

`kicad-cli sch export netlist` warns that the schematic has annotation errors
and lists every pin unconnected, the two R4 pins as `unconnected-(R4-Pad1)`
and `unconnected-(R4-Pad1)_1`. `kicad-cli sch erc --severity-all` reports 6
`pin_not_connected`.

## The oracle

Before this change, `connect_to_net` with `reference: "R4"`, `pin_number: "1"`
and `net: "SIG"` drew the stub from the R4 at (180.34, 120.65), the first in
the file, and reported success; KiCad 10.0.8's netlist then put that copy's
pin 1 on `/SIG` and left the other unconnected. `connect_pins` from R4 pin 2
to R5 pin 1 wired the same copy. With this change the call is refused as `ambiguous_target`
naming both candidates, as `unit 1 <uuid> at (180.34, 116.84)` and
`unit 1 <uuid> at (200.66, 116.84)`, and the file is unchanged. `connect_pins`
from R4 pin 2 to R5 pin 1 is refused the same way with the pin 2 positions.
