---
name: kicad-autoroute
description: |
  Autorouting workflow for KiCAD boards with a local Freerouting installation via MCP tools.
  Triggers on: "autoroute", "auto-route", "Freerouting", "route the whole board",
  "Specctra", "DSN", "SES", "import the session file", "finish the routing automatically".
argument-hint: "[board to autoroute]"
---

# KiCAD Autorouting Workflow (Freerouting)

Route a placed board with a local Freerouting engine and bring the result back
as one reviewable step. Nothing is uploaded: the engine runs on this machine.
The routed result is a candidate until KiCad's DRC is clean on it.

## Toolset loading

```
load_toolset('integration')      # Freerouting discovery and routing
load_toolset('pcb_export')       # Specctra DSN export
load_toolset('pcb_routing')      # session import
load_toolset('verification')     # DRC
```

## 1. Check the preconditions

- Call `check_freerouting`. It reports whether an engine was found, whether its
  headless service is usable, and whether the KiCad bridge is available, as
  separate facts. If no engine is found, stop and tell the user how to install
  one; do not substitute hand routing silently.
- The board must be open in KiCad, saved, and fully placed. Placement quality
  decides routing quality, so settle placement first.
- Net classes, clearances and track widths must be set before export. The
  export refuses a board whose rules are incomplete.
- Critical routes the autorouter must keep are routed by hand first and
  locked. Locked straight tracks and through vias are preserved as fixed
  wiring. Unlocked existing routing, arcs, and unsupported geometry make the
  export refuse: lock what should stay, remove what should not.

## 2. Export

Call `export_specctra_dsn`. It writes the DSN and a manifest bound to the
board's current revision. The manifest is what later proves the session file
belongs to this exact board, so keep both files together and do not edit the
board between export and import.

## 3. Route

Call `route_specctra_dsn` with the exported DSN. It produces a new session
file and never replaces an existing one. Routing can take minutes on a dense
board; report the engine's own completion status, including unrouted
connections, rather than assuming success.

## 4. Plan, then apply

1. Call `plan_specctra_ses_import`. It validates the session against the
   manifest and the live board and lists every track and via it would add,
   without changing anything. Read the plan: check the layer usage and the
   count of unrouted connections.
2. If the plan is refused because the board changed, export again. Do not
   reuse a stale plan.
3. Call `apply_specctra_ses`. It applies the routes as one KiCad undo step,
   reads the board back, writes a separate candidate board, and reports DRC
   evidence for it.

## 5. Verify

Read the DRC evidence returned by the apply step, then run `run_drc` on the
saved board. Report unconnected items and clearance errors by net. Refill
copper zones with `refill_zones` before the final DRC, since new tracks change
the fills.

If the result is poor, undo in KiCad, improve placement or rules, and repeat
from the export. Hand-tuning a bad autoroute usually costs more than rerunning.

Completion criterion: the session applied against the revision it was exported
from, DRC on the saved board is clean or every remaining finding is reported,
and the user knows which nets, if any, are still unrouted.
