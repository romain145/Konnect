---
name: kicad-templates
description: |
  Reference-circuit template workflow for KiCAD schematics via MCP tools. Triggers on:
  "use a template", "reference circuit", "standard circuit for", "add a USB-C port",
  "add an LDO", "add a buck converter", "starter circuit", "known-good circuit",
  "what templates are there".
argument-hint: "[circuit to instantiate]"
---

# KiCAD Reference-Circuit Templates Workflow

Start a common sub-circuit from Konnect's template library instead of from a
blank sheet, then adapt it to the project. A template is a starting point with
checked component values; it is not a verified design for this project's
voltages, currents and parts.

## Toolset loading

```
load_toolset('templates')        # search, inspect, instantiate
load_toolset('sch_analysis')     # check what was placed
load_toolset('sch_export')       # ERC
```

## 1. Find a template

- `list_template_categories` shows what the library holds and how many
  templates each category has. Call it when the user asks what is available.
- `search_templates` finds templates for a described sub-circuit.

If nothing fits, say so and build the circuit with the schematic workflow.
Do not stretch an unrelated template to cover the request.

## 2. Read it before placing it

Call `get_template` with the identifier from the search. Read its components,
its connections, and its design notes. Check against the request:

- input and output voltages, and load current;
- the exact parts and packages, and whether the project's fabricator stocks
  them;
- which of its nets are meant to join nets that already exist in the project,
  such as ground and supply rails.

State any mismatch to the user before placing anything.

## 3. Instantiate

Call `apply_template` on the target schematic. Use its net mappings to bind
the template's nets to the project's existing net names, so the new circuit
connects to the rails already there rather than creating parallel ones. Place
it clear of existing symbols; overlapping symbols can short pins.

## 4. Adapt and verify

1. Change values, parts and footprints that the request differs on, using the
   schematic component tools.
2. Call `list_schematic_components` and confirm every template part is
   present with a footprint assigned.
3. Call `get_net_connections` on each mapped net and confirm the template
   joined the intended project net.
4. Run `run_erc` and resolve what it reports.

Check any value that depends on the project against the part's datasheet:
feedback dividers, inductor and capacitor ratings, pull-up values, current
limits. Report which values were taken from the template unchanged.

Completion criterion: the template is placed, its nets are bound to the
project's nets, ERC is clean for the new circuit, and the user has been told
which values still rest on the template rather than on this project's
requirements.
