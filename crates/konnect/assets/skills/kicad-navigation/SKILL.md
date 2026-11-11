---
name: kicad-navigation
description: |
  Editor context and selection workflow for a running KiCAD session via MCP tools. Triggers on:
  "what do I have selected", "the selected component", "this part", "select R5 in KiCAD",
  "highlight", "cross-probe", "find it on the board", "show it in the schematic",
  "which editors are open", "which board is open".
argument-hint: "[object to find or select]"
---

# KiCAD Editor Navigation Workflow

Work from what the user is looking at in KiCad: read the open editors and the
selection, resolve a named object to its exact identity, and change the
selection so the user can see what you mean. These tools need KiCad running
with its API enabled.

## Toolset loading

```
load_toolset('editor_navigation')
```

## 1. Observe before assuming

Call `get_editor_state` first. It reports the running KiCad version, which
schematic and PCB editors can be addressed, the exact documents they hold, and
which capabilities are unavailable. Several documents can be open at once, so
never infer "the board" from a project name: use the document identity this
call returns in every later call.

The API does not report which window has focus. When the request depends on
that, ask the user rather than guess.

## 2. Resolve "this" and "that"

- When the user says "this", "the selected part" or similar, call
  `get_editor_selection` for the editor, document and sheet in question. It
  returns stable identities, and it checks that the document has not changed
  since it was read.
- When the user names an object, call `resolve_navigation_target`. A stable
  identifier always resolves exactly. A human reference such as a designator
  resolves only when the saved design yields a single candidate; if several
  match, present them and ask.
- An empty selection is an answer, not an error. Say so and ask what the user
  meant.

## 3. Show the user

`mutate_editor_selection` clears the selection, adds to it, or removes from
it. Use it to point at the objects you are about to change or have just
reported on. It reports success only when a fresh readback shows the complete
requested selection, so a partial result is a failure to report, not a
success.

Changing the selection is visible to the user and replaces what they had
selected. Do it when it helps them follow, not on every step.

## 4. Cross-probe between schematic and board

`resolve_cross_probe_target` maps a schematic symbol to its footprint or a
footprint to its symbol, using the saved link between them. It requires both
documents to be open and does not move the view or select anything. To show
the result, pass the resolved identity to `mutate_editor_selection` for the
other editor.

A footprint with no symbol link, or a symbol with no footprint on the board,
does not resolve. Report that as a schematic-to-board sync gap.

## Rules

- Carry identities, not names, from one call to the next.
- A refused call that names a stale document means the user changed something:
  observe again before retrying.
- These tools only read context and change the selection. Edits to the design
  go through the schematic and PCB toolsets, using the identities resolved
  here.

Completion criterion: every object the answer refers to was resolved to one
exact identity in a named open document, or the ambiguity was put to the user.
