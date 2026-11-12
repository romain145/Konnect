//! Wiring writers that join only what they were asked to (#145, #157, #158),
//! name exactly one pin (#98), and edit the item's own `(at …)` (#111).
//!
//! The sheets are KiCad-saved; see `route_obstacles_kicad10.README.md` and
//! `duplicate_reference_kicad10.README.md` for their groups and KiCad's
//! netlist of each result.

use super::*;
use serde_json::json;
use std::path::{Path, PathBuf};

const ROUTE: &str = include_str!("../../tests/fixtures/route_obstacles_kicad10.kicad_sch");
const ROUTE_SUB: &str = include_str!("../../tests/fixtures/route_obstacles_sub_kicad10.kicad_sch");
const DUPLICATE: &str = include_str!("../../tests/fixtures/duplicate_reference_kicad10.kicad_sch");

// Pin tips on the route sheet. Every part is `Device:R` at rotation 0, so pin
// 1 is 3.81 mm above the placement and pin 2 3.81 mm below it.
const R1_2: (f64, f64) = (101.6, 105.41);
const R2_1: (f64, f64) = (127.0, 107.95);
const R3_1: (f64, f64) = (114.3, 105.41);
const R11_2: (f64, f64) = (101.6, 54.61);
const R12_1: (f64, f64) = (127.0, 57.15);
const R13_1: (f64, f64) = (114.3, 54.61);
const R14_1: (f64, f64) = (120.65, 57.15);
const R21_2: (f64, f64) = (101.6, 156.21);
const R31_2: (f64, f64) = (50.8, 54.61);
const R32_1: (f64, f64) = (50.8, 57.15);
const R41_2: (f64, f64) = (177.8, 105.41);
const SHEET_PIN: (f64, f64) = (203.2, 105.41);
const R51_2: (f64, f64) = (177.8, 156.21);
const R53_2: (f64, f64) = (190.5, 143.51);
/// Where R53's wire ends mid-span on R51's, with no junction: two nets.
const TEE: (f64, f64) = (190.5, 156.21);

fn sheet(source: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sheet.kicad_sch");
    std::fs::write(&path, source).unwrap();
    std::fs::write(
        directory
            .path()
            .join("route_obstacles_sub_kicad10.kicad_sch"),
        ROUTE_SUB,
    )
    .unwrap();
    (directory, path)
}

/// Run a registered tool as the MCP dispatch does: `(is_error, body)`.
async fn call(name: &str, path: &Path, mut args: Value) -> (bool, Value) {
    args["schematic"] = json!(path.display().to_string());
    let tool = sch_wiring::tools()
        .into_iter()
        .chain(sch_batch::tools())
        .find(|tool| tool.name == name)
        .unwrap_or_else(|| panic!("no tool named {name}"));
    let context = Arc::new(ToolContext::new(
        ServerConfig::default(),
        Arc::new(ToolRouter::new()),
    ));
    let result = (tool.handler)(&args, context).await.unwrap();
    let crate::mcp::protocol::ToolContent::Text { text } = &result.content[0] else {
        panic!("expected text content from {name}");
    };
    let body = serde_json::from_str(text).unwrap_or_else(|_| json!(text));
    (result.is_error, body)
}

/// Whether `a` and `b` are on one net in the saved file, by the graph every
/// connectivity tool reads.
fn joined(path: &Path, a: (f64, f64), b: (f64, f64)) -> bool {
    let tree = konnect_sexp::parse_sexp(&std::fs::read_to_string(path).unwrap()).unwrap();
    let wires = konnect_sexp::schematic::extract_wires(&tree);
    let labels = konnect_sexp::schematic::extract_all_net_labels(&tree);
    let mut graph = sch_connectivity::net_graph_for(&tree, &wires, &labels);
    graph.root_at(a.0, a.1) == graph.root_at(b.0, b.1)
}

fn junction_at(path: &Path, (x, y): (f64, f64)) -> bool {
    let tree = konnect_sexp::parse_sexp(&std::fs::read_to_string(path).unwrap()).unwrap();
    konnect_sexp::schematic::extract_junctions(&tree)
        .iter()
        .any(|&(jx, jy)| (jx - x).abs() < 0.01 && (jy - y).abs() < 0.01)
}

fn assert_kind(body: &Value, kind: &str) {
    assert_eq!(body["error"]["kind"], kind, "{body}");
}

// ─── #145: routes ─────────────────────────────────────────────────────────────

/// The issue's reproduction: R3.1 lies on the horizontal-first leg of
/// R1.2 → R2.1, which joined it. The vertical-first bend touches nothing.
#[tokio::test]
async fn connect_pins_bends_around_another_pin_on_its_first_route() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "connect_pins",
        &path,
        json!({"ref1": "R1", "pin1": "2", "ref2": "R2", "pin2": "1"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(
        body["path"],
        json!([[101.6, 105.41], [101.6, 107.95], [127.0, 107.95]])
    );
    assert!(joined(&path, R1_2, R2_1));
    assert!(!joined(&path, R1_2, R3_1), "R3.1 was joined to R1's net");
    assert!(!junction_at(&path, R3_1));
}

/// Both bends meet another net: R13.1 (label OTHER) on the first, R14.1 on
/// the second. The call is refused, names both, and writes nothing.
#[tokio::test]
async fn connect_pins_refuses_when_both_bends_join_another_net() {
    let (_directory, path) = sheet(ROUTE);
    let before = std::fs::read(&path).unwrap();
    let (failed, body) = call(
        "connect_pins",
        &path,
        json!({"ref1": "R11", "pin1": "2", "ref2": "R12", "pin2": "1"}),
    )
    .await;
    assert!(failed, "{body}");
    assert_kind(&body, "plan_blocked");
    let reasons = body["error"]["reasons"].as_array().unwrap();
    // Both L-bends, then every dogleg: each runs across R13 or R14.
    assert_eq!(reasons.len(), 6, "{body}");
    for dogleg in &reasons[2..] {
        let dogleg = dogleg.as_str().unwrap();
        assert!(
            dogleg.starts_with("the dogleg at ") && dogleg.contains("would cross R1"),
            "{dogleg}"
        );
    }
    let first = reasons[0].as_str().unwrap();
    assert!(first.starts_with("the horizontal-first route"), "{first}");
    assert!(
        first.contains("R13 pin 1") && first.contains("on net OTHER"),
        "{first}"
    );
    let second = reasons[1].as_str().unwrap();
    assert!(second.starts_with("the vertical-first route"), "{second}");
    assert!(second.contains("R14 pin 1 at (120.65, 57.15)"), "{second}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

/// Two parts side by side, bottom pin to top pin: each L-bend runs down one
/// part's own pin column and would short it. The real-KiCad design loop
/// (`e2e_kicad`) is this layout. The wire goes through the channel between
/// them instead.
#[tokio::test]
async fn connect_pins_doglegs_between_two_parts_side_by_side() {
    let (_directory, path) = sheet(ROUTE);
    let r21_1 = (R21_2.0, R21_2.1 - 7.62);
    let r22_2 = (127.0, 158.75 + 7.62);
    let (failed, body) = call(
        "connect_pins",
        &path,
        json!({"ref1": "R21", "pin1": "1", "ref2": "R22", "pin2": "2"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(
        body["path"],
        json!([
            [r21_1.0, r21_1.1],
            [114.3, r21_1.1],
            [114.3, r22_2.1],
            [r22_2.0, r22_2.1]
        ])
    );
    assert!(joined(&path, r21_1, r22_2));
    assert!(!joined(&path, r21_1, R21_2), "R21 was shorted");
    assert!(!joined(&path, r22_2, (127.0, 158.75)), "R22 was shorted");
}

/// Negative control: a route over nothing is drawn as before, horizontal leg
/// first.
#[tokio::test]
async fn a_route_that_meets_nothing_is_drawn_as_before() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "connect_pins",
        &path,
        json!({"ref1": "R21", "pin1": "2", "ref2": "R22", "pin2": "1"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(
        body["path"],
        json!([[101.6, 156.21], [127.0, 156.21], [127.0, 158.75]])
    );
    assert!(joined(&path, R21_2, (127.0, 158.75)));
}

/// A leg ending on a sheet pin joins the sheet's net as surely as a pin's.
#[tokio::test]
async fn connect_pins_does_not_end_a_leg_on_a_sheet_pin() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "connect_pins",
        &path,
        json!({"ref1": "R41", "pin1": "2", "ref2": "R42", "pin2": "1"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(
        body["path"],
        json!([[177.8, 105.41], [177.8, 107.95], [203.2, 107.95]])
    );
    assert!(!joined(&path, R41_2, SHEET_PIN));
}

/// `add_schematic_connection` routes the same way. A point the caller names
/// on a wire is a tap, so that wire's net is not foreign; the T is dotted and
/// the junction-less T beside it is not (#158).
#[tokio::test]
async fn add_schematic_connection_shares_the_guard() {
    let (_directory, path) = sheet(ROUTE);
    let before = std::fs::read(&path).unwrap();
    let (failed, body) = call(
        "add_schematic_connection",
        &path,
        json!({"x1": R11_2.0, "y1": R11_2.1, "x2": R12_1.0, "y2": R12_1.1}),
    )
    .await;
    assert!(failed, "{body}");
    assert_kind(&body, "plan_blocked");
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let (failed, body) = call(
        "add_schematic_connection",
        &path,
        json!({"x1": 184.15, "y1": 148.59, "x2": 184.15, "y2": 156.21}),
    )
    .await;
    assert!(!failed, "{body}");
    assert!(junction_at(&path, (184.15, 156.21)));
    assert!(joined(&path, (184.15, 148.59), R51_2));
    assert!(!junction_at(&path, TEE));
    assert!(!joined(&path, R51_2, R53_2));
}

/// The batch draws what it can and lists the pair it refused.
#[tokio::test]
async fn batch_connect_pins_skips_and_reports_a_route_that_joins_another_net() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "batch_connect_pins",
        &path,
        json!({"connections": [
            {"ref1": "R1", "pin1": "2", "ref2": "R2", "pin2": "1"},
            {"ref1": "R11", "pin1": "2", "ref2": "R12", "pin2": "1"},
            {"ref1": "R21", "pin1": "2", "ref2": "R22", "pin2": "1"},
            {"ref1": "R41", "pin1": "2", "ref2": "R42", "pin2": "1"}
        ]}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["connected_count"], 3);
    assert_eq!(body["errors"].as_array().unwrap().len(), 1);
    let failures = body["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["index"], 1);
    assert_eq!(failures[0]["kind"], "plan_blocked");
    // Two L-bends and four doglegs, as in the single-pair refusal above.
    assert_eq!(failures[0]["reasons"].as_array().unwrap().len(), 6);
    assert!(joined(&path, R1_2, R2_1));
    assert!(!joined(&path, R1_2, R3_1));
    assert!(!joined(&path, R11_2, R12_1));
    assert!(!joined(&path, R11_2, R13_1));
    assert!(!joined(&path, R12_1, R14_1));
    assert!(!joined(&path, R41_2, SHEET_PIN));
    assert!(!junction_at(&path, TEE));
}

// ─── #157: stubs ──────────────────────────────────────────────────────────────

/// R31.2's outward stub (down) ends on R32.1. `auto` takes the next direction
/// across the pin and says why.
#[tokio::test]
async fn connect_to_net_auto_turns_a_stub_away_from_another_pin() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "connect_to_net",
        &path,
        json!({"reference": "R31", "pin_number": "2", "net": "N"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["direction"], "right");
    assert!(
        (body["label"]["x"].as_f64().unwrap() - 53.34).abs() < 1e-6,
        "{body}"
    );
    assert_eq!(body["label"]["y"], 54.61);
    let skipped = body["skipped_directions"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    let skipped = skipped[0].as_str().unwrap();
    assert!(
        skipped.starts_with("down:") && skipped.contains("R32 pin 1 at (50.8, 57.15)"),
        "{skipped}"
    );
    assert!(!joined(&path, R31_2, R32_1));
    assert!(!junction_at(&path, TEE));
}

/// A named direction is the only one tried.
#[tokio::test]
async fn connect_to_net_refuses_a_named_direction_that_joins_another_pin() {
    let (_directory, path) = sheet(ROUTE);
    let before = std::fs::read(&path).unwrap();
    let (failed, body) = call(
        "connect_to_net",
        &path,
        json!({"reference": "R31", "pin_number": "2", "net": "N", "direction": "down"}),
    )
    .await;
    assert!(failed, "{body}");
    assert_kind(&body, "plan_blocked");
    let reasons = body["error"]["reasons"].as_array().unwrap();
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].as_str().unwrap().contains("R32 pin 1"), "{body}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

/// Negative control: a stub that meets nothing keeps its outward direction
/// and reports no skipped one.
#[tokio::test]
async fn connect_to_net_draws_a_clear_stub_as_before() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "connect_to_net",
        &path,
        json!({"reference": "R21", "pin_number": "2", "net": "N"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["direction"], "down");
    assert!(body.get("skipped_directions").is_none(), "{body}");
    assert!(!junction_at(&path, TEE));
}

/// The batch refuses the one pin whose stub would join another, lists it,
/// and still labels the rest.
#[tokio::test]
async fn batch_connect_to_net_refuses_a_stub_that_joins_another_pin() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "batch_connect_to_net",
        &path,
        json!({"net_name": "N", "stub_length": 2.54, "direction": "down", "pins": [
            {"reference": "R31", "pin_number": "2"},
            {"reference": "R21", "pin_number": "2"}
        ]}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["added_count"], 1);
    assert_eq!(body["added"][0]["reference"], "R21");
    let failures = body["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["index"], 0);
    assert_eq!(failures[0]["kind"], "plan_blocked");
    assert!(!joined(&path, R31_2, R32_1));

    // With `auto`, R31.2's stub turns right instead.
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "batch_connect_to_net",
        &path,
        json!({"net_name": "N", "stub_length": 2.54, "pins": [
            {"reference": "R31", "pin_number": "2"}
        ]}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["added"][0]["direction"], "right");
    assert!(body.get("failures").is_none(), "{body}");
    assert!(!joined(&path, R31_2, R32_1));
}

// ─── #158: junctions only on the new wire ─────────────────────────────────────

/// A wire far from the junction-less T leaves it alone, in both wire tools.
/// Negative control: a wire that makes a T of its own still gets its dot.
#[tokio::test]
async fn wire_tools_dot_only_the_tees_the_new_wire_makes() {
    for tool in ["add_wire", "batch_add_wire"] {
        let (_directory, path) = sheet(ROUTE);
        let far = json!({"x1": 20.32, "y1": 20.32, "x2": 30.48, "y2": 20.32});
        let args = if tool == "add_wire" {
            far
        } else {
            json!({"wires": [far]})
        };
        let (failed, body) = call(tool, &path, args).await;
        assert!(!failed, "{tool}: {body}");
        assert!(!junction_at(&path, TEE), "{tool} dotted the far T");
        assert!(!joined(&path, R51_2, R53_2), "{tool} joined R53");

        let tap = json!({"x1": 184.15, "y1": 143.51, "x2": 184.15, "y2": 156.21});
        let args = if tool == "add_wire" {
            tap
        } else {
            json!({"wires": [tap]})
        };
        let (failed, body) = call(tool, &path, args).await;
        assert!(!failed, "{tool}: {body}");
        assert!(junction_at(&path, (184.15, 156.21)), "{tool}");
        assert!(!junction_at(&path, TEE), "{tool} dotted the far T");
    }
}

// ─── #98: one placed pin per name ─────────────────────────────────────────────

/// Two placed symbols are both `R4`. Each tool refuses with both candidates
/// and writes nothing for that pin.
#[tokio::test]
async fn a_duplicated_reference_is_ambiguous_for_every_pin_tool() {
    let (_directory, path) = sheet(DUPLICATE);
    let before = std::fs::read(&path).unwrap();
    for (tool, args) in [
        (
            "connect_to_net",
            json!({"reference": "R4", "pin_number": "1", "net": "SIG"}),
        ),
        (
            "connect_pins",
            json!({"ref1": "R4", "pin1": "1", "ref2": "R5", "pin2": "1"}),
        ),
    ] {
        let (failed, body) = call(tool, &path, args).await;
        assert!(failed, "{tool}: {body}");
        assert_kind(&body, "ambiguous_target");
        assert_eq!(body["error"]["target"], "R4 pin 1");
        let candidates = body["error"]["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2, "{tool}: {body}");
        let candidates: Vec<&str> = candidates.iter().map(|c| c.as_str().unwrap()).collect();
        assert!(
            candidates.iter().all(|c| c.starts_with("unit 1 ")),
            "{candidates:?}"
        );
        assert!(candidates
            .iter()
            .any(|c| c.ends_with("at (180.34, 116.84)")));
        assert!(candidates
            .iter()
            .any(|c| c.ends_with("at (200.66, 116.84)")));
        assert_eq!(std::fs::read(&path).unwrap(), before, "{tool} wrote");
    }

    let (failed, body) = call(
        "batch_connect_pins",
        &path,
        json!({"connections": [{"ref1": "R4", "pin1": "1", "ref2": "R5", "pin2": "1"}]}),
    )
    .await;
    assert!(failed, "{body}");
    assert_eq!(body["connected_count"], 0);
    assert_eq!(body["failures"][0]["kind"], "ambiguous_target");
    assert_eq!(
        body["failures"][0]["candidates"].as_array().unwrap().len(),
        2
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);

    // The batch labels the pin it can name and lists the one it cannot.
    let (failed, body) = call(
        "batch_connect_to_net",
        &path,
        json!({"net_name": "SIG", "pins": [
            {"reference": "R4", "pin_number": "1"},
            {"reference": "R5", "pin_number": "1"}
        ]}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["added_count"], 1);
    assert_eq!(body["added"][0]["reference"], "R5");
    assert_eq!(body["failures"][0]["index"], 0);
    assert_eq!(body["failures"][0]["kind"], "ambiguous_target");
}

/// A reference or pin that is not there is `stale_target`, not free text.
/// Negative control: R5 is placed once and wires as before.
#[tokio::test]
async fn a_missing_reference_or_pin_is_a_stale_target() {
    let (_directory, path) = sheet(DUPLICATE);
    for (tool, args, reason) in [
        (
            "connect_to_net",
            json!({"reference": "R9", "pin_number": "1", "net": "SIG"}),
            "Component 'R9' not found",
        ),
        (
            "connect_to_net",
            json!({"reference": "R5", "pin_number": "3", "net": "SIG"}),
            "Pin '3' not found on 'R5' (unit 1)",
        ),
        (
            "connect_pins",
            json!({"ref1": "R5", "pin1": "3", "ref2": "R5", "pin2": "1"}),
            "Pin '3' not found on 'R5' (unit 1)",
        ),
    ] {
        let (failed, body) = call(tool, &path, args).await;
        assert!(failed, "{tool}: {body}");
        assert_kind(&body, "stale_target");
        assert_eq!(body["error"]["reason"], reason, "{tool}");
    }

    let (failed, body) = call(
        "connect_to_net",
        &path,
        json!({"reference": "R5", "pin_number": "1", "net": "SIG"}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["wire"]["x1"], 190.5);
    assert_eq!(body["wire"]["y1"], 135.89);
}

// ─── #111: the item's own (at …) ──────────────────────────────────────────────

/// The direct `(at …)` child of a node, as `(x, y, rotation)`.
fn at_of(node: &konnect_sexp::SexpNode) -> (f64, f64, f64) {
    let at = node.find("at").unwrap();
    (
        at.get_f64(1).unwrap(),
        at.get_f64(2).unwrap(),
        at.get_f64(3).unwrap_or(0.0),
    )
}

/// A label named `(at 1 2)`: rotating it turns the label and keeps the name.
#[tokio::test]
async fn rotate_label_keeps_a_name_that_holds_at() {
    let (_directory, path) = sheet(ROUTE);
    let (failed, body) = call(
        "add_schematic_net_label",
        &path,
        json!({"net": "(at 1 2)", "x": 114.3, "y": 97.79}),
    )
    .await;
    assert!(!failed, "{body}");
    let (failed, body) = call(
        "rotate_schematic_label",
        &path,
        json!({"net": "(at 1 2)", "x": 114.3, "y": 97.79, "rotation": 180}),
    )
    .await;
    assert!(!failed, "{body}");

    let tree = konnect_sexp::parse_sexp(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let labels = tree.find_all("label");
    let label = labels
        .iter()
        .find(|label| label.get(1).and_then(konnect_sexp::SexpNode::as_str) == Some("(at 1 2)"))
        .expect("the label kept its name");
    assert_eq!(at_of(label), (114.3, 97.79, 180.0));
    let justify = label.find("effects").unwrap().find("justify").unwrap();
    assert_eq!(
        justify.get(1).and_then(konnect_sexp::SexpNode::as_str),
        Some("right")
    );
}

/// R1's Value is `(at 1 2)`: a move shifts the field and keeps the value.
#[tokio::test]
async fn bulk_move_keeps_a_value_that_holds_at() {
    let (_directory, path) = sheet(ROUTE);
    let content = std::fs::read_to_string(&path).unwrap();
    let (start, end) = find_symbol_instance_block(&content, "R1").unwrap();
    let block = content[start..end].replacen(
        "(property \"Value\" \"10k\"",
        "(property \"Value\" \"(at 1 2)\"",
        1,
    );
    std::fs::write(
        &path,
        format!("{}{}{}", &content[..start], block, &content[end..]),
    )
    .unwrap();
    let value_at = |path: &Path| {
        let tree = konnect_sexp::parse_sexp(&std::fs::read_to_string(path).unwrap()).unwrap();
        let symbol = tree
            .find_all("symbol")
            .into_iter()
            .find(|symbol| {
                symbol.find_all("property").iter().any(|p| {
                    p.get(1).and_then(konnect_sexp::SexpNode::as_str) == Some("Reference")
                        && p.get(2).and_then(konnect_sexp::SexpNode::as_str) == Some("R1")
                })
            })
            .unwrap()
            .clone();
        let value = symbol
            .find_all("property")
            .into_iter()
            .find(|p| p.get(1).and_then(konnect_sexp::SexpNode::as_str) == Some("Value"))
            .unwrap()
            .clone();
        (
            at_of(&symbol),
            value
                .get(2)
                .and_then(konnect_sexp::SexpNode::as_str)
                .unwrap()
                .to_string(),
            at_of(&value),
        )
    };
    let (symbol_before, _, field_before) = value_at(&path);

    let (failed, body) = call(
        "bulk_move_schematic_components",
        &path,
        json!({"references": ["R1"], "dx": 2.54, "dy": 0}),
    )
    .await;
    assert!(!failed, "{body}");
    assert_eq!(body["moved_count"], 1);
    let (symbol_after, value, field_after) = value_at(&path);
    assert_eq!(value, "(at 1 2)");
    assert!((symbol_after.0 - symbol_before.0 - 2.54).abs() < 1e-6);
    assert!((field_after.0 - field_before.0 - 2.54).abs() < 1e-6);
    assert_eq!(field_after.1, field_before.1);
    assert_eq!(field_after.2, field_before.2);
}
