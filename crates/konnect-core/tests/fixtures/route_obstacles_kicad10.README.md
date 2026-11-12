# Route obstacles fixture

`route_obstacles_kicad10.kicad_sch` and its child sheet
`route_obstacles_sub_kicad10.kicad_sch` put another net's pin, label or sheet
pin where the wiring tools draw: on the L-bend `connect_pins`,
`batch_connect_pins` and `add_schematic_connection` route between two pins
(#145), and at the end of a `connect_to_net` stub (#157). They also hold a T
with no junction, which every wire tool used to dot wherever it was on the
sheet (#158). `wiring_guard_tests.rs` drives each group.

## Provenance

The sheets were written as text by a script, with the `Device:R` definition
taken from `batch_connect_kicad10.kicad_sch`, then force-resaved by KiCad
10.0.8 on Windows:

```text
kicad-cli sch upgrade --force route_obstacles_sub_kicad10.kicad_sch
kicad-cli sch upgrade --force route_obstacles_kicad10.kicad_sch
```

Both files are that Eeschema serialization, byte for byte: `(generator
"eeschema")`, `(version 20260306)`, CRLF line ends. Every part is `Device:R`
at rotation 0, so pin 1 is at `y − 3.81` and pin 2 at `y + 3.81`.

| Group | Parts and items | Call | In the way |
|---|---|---|---|
| around | R1 (101.6, 101.6), R2 (127, 111.76), R3 (114.3, 109.22) | R1.2 → R2.1 | R3.1 at (114.3, 105.41), on the horizontal-first leg. The reproduction in #145. |
| blocked | R11 (101.6, 50.8), R12 (127, 60.96), R13 (114.3, 58.42), R14 (120.65, 60.96); label `OTHER` on R13.1 | R11.2 → R12.1 | R13.1 (net `OTHER`) on the horizontal-first leg, R14.1 on the vertical-first leg |
| clear | R21 (101.6, 152.4), R22 (127, 162.56) | R21.2 → R22.1 | nothing |
| stub | R31 (50.8, 50.8), R32 (50.8, 60.96) | `connect_to_net` R31 pin 2 | R32.1 at (50.8, 57.15), where the 2.54 mm downward stub ends. The reproduction in #157. |
| sheet | R41 (177.8, 101.6), R42 (203.2, 111.76); sheet `SUB` at (203.2, 96.52), 19.05 × 10.16, input pin `IN` at (203.2, 105.41) | R41.2 → R42.1 | sheet pin `IN` at the horizontal-first corner |
| tee | R51 (177.8, 152.4), R52 (203.2, 162.56), R53 (190.5, 139.7); wires (177.8, 156.21)→(203.2, 156.21)→(203.2, 158.75) and (190.5, 143.51)→(190.5, 156.21), no junction | any wire elsewhere | the junction-less T at (190.5, 156.21). The reproduction in #158. |

`kicad-cli sch export netlist` on the fixture names `/OTHER` (R13.1) and
`Net-(R51-Pad2)` (R51.2 R52.1). R53.2 is not on that net: KiCad does not join a
wire end that lands mid-span without a junction. Every other pin is
`unconnected-…`. `kicad-cli sch erc --severity-all` reports 30
`pin_not_connected`, 1 `isolated_pin_label` (`OTHER`), 1 `label_dangling` (the
child's `IN`) and 1 `unconnected_wire_endpoint` (R53's wire at the T).

## The oracle: KiCad's netlist of each result

Each sequence ran on a copy of the fixture through the tool handlers, built
from the commit before this change and from this change, and KiCad 10.0.8
netlisted the result. The batch sequence is `batch_connect_pins`
with the around, blocked, clear and sheet pairs in that order, then
`batch_connect_to_net` (net `N`, `stub_length` 2.54) on R31 pin 2, then
`batch_add_wire` (20.32, 20.32)→(30.48, 20.32). The single-call sequence is
`connect_pins` once per pair, `connect_to_net` R31 pin 2 net `N`, and
`add_wire` with the same wire. Both sequences give the same netlist. Pins left
unconnected are omitted; bold pins were joined without being asked.

| Group | Before this change | This change |
|---|---|---|
| around | `Net-(R1-Pad2)` R1.2 R2.1 **R3.1**; junction at (114.3, 105.41) | `Net-(R1-Pad2)` R1.2 R2.1; path (101.6, 105.41)→(101.6, 107.95)→(127, 107.95), no junction |
| blocked | `/OTHER` **R11.2 R12.1** R13.1; junction at (114.3, 54.61) | refused, nothing written; `/OTHER` R13.1 |
| clear | `Net-(R21-Pad2)` R21.2 R22.1 | the same, path (101.6, 156.21)→(127, 156.21)→(127, 158.75) |
| stub | `/N` R31.2 **R32.1** (stub down) | `/N` R31.2 (stub right, `skipped_directions` names R32 pin 1) |
| sheet | `/SUB/IN` **R41.2 R42.1** | `Net-(R41-Pad2)` R41.2 R42.1; path (177.8, 105.41)→(177.8, 107.95)→(203.2, 107.95) |
| tee | `Net-(R51-Pad2)` R51.2 R52.1 **R53.2**; junction at (190.5, 156.21) | `Net-(R51-Pad2)` R51.2 R52.1; no junction |

Before this change the batch reported `{"connected_count":4,"errors":[]}`.
This change's batch reported `connected_count` 3 and one error, also listed in
`failures` with `kind: "plan_blocked"`:

```text
R11.2 -> R12.1: refused, nothing written for it: the horizontal-first route would join R13 pin 1 and label OTHER at (114.3, 54.61) on net OTHER; the vertical-first route would join R14 pin 1 at (120.65, 57.15)
```

`connect_pins` refuses that pair with `plan_blocked`, one reason per bend.
