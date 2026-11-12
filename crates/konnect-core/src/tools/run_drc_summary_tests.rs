//! The `run_drc` summary, through served `tools/call`.
//!
//! No KiCad runs here. A stand-in `kicad-cli` exits 0 and the report it would
//! have written is put in place beforehand, so what is under test is what
//! `run_drc` makes of a report, not what KiCad finds on a board.
//!
//! The report is `tests/fixtures/drc_ownership_j1.drc.json` (see its README):
//! kicad-cli 10.0.5 output for the board beside it, to which two violations
//! were added by hand. Its `ignored_checks` list is KiCad's own. Counted
//! independently of this code, it holds:
//!
//! | rule                    | errors | warnings |
//! |-------------------------|--------|----------|
//! | `copper_edge_clearance` | 5      | 0        |
//! | `lib_footprint_issues`  | 0      | 1        |
//! | `silk_edge_clearance`   | 0      | 1        |

use super::*;
use crate::mcp::handler::McpHandler;
use crate::tools::ServerConfig;

const BOARD: &str = include_str!("../../tests/fixtures/drc_ownership_j1.kicad_pcb");
const REPORT: &str = include_str!("../../tests/fixtures/drc_ownership_j1.drc.json");

/// `run_drc` on a copy of the fixture board, with `report` as what kicad-cli
/// wrote. Returns the response body with `isError` folded in.
async fn run_drc_on(report: &str, extra: Value) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let board = dir.path().join("j1.kicad_pcb");
    std::fs::write(&board, BOARD).unwrap();
    // Where `cli::run_drc` asks kicad-cli to write, and then reads.
    std::fs::write(board.with_extension("drc.json"), report).unwrap();
    let kicad_cli = crate::tools::cli::test_support::noop_cli(dir.path());

    let handler = McpHandler::new(ServerConfig {
        kicad_cli: kicad_cli.display().to_string(),
        kicad_binary: String::new(),
        ipc_address: String::new(),
        project_dir: None,
        jlcpcb_db_path: None,
        auto_load_toolsets: true,
        eager_toolsets: true,
    })
    .await
    .unwrap();
    let mut arguments = extra;
    arguments["board"] = json!(board.display().to_string());
    let result = handler
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "run_drc", "arguments": arguments }
        }))
        .await
        .unwrap()
        .result
        .unwrap();
    let text = result["content"][0]["text"].as_str().unwrap();
    let mut body: Value = serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("response is not JSON ({error}): {text}"));
    body["isError"] = result["isError"].clone();
    body
}

/// The fixture report with one top-level key taken out or replaced.
fn report_with(key: &str, value: Option<Value>) -> String {
    let mut report: Value = serde_json::from_str(REPORT).unwrap();
    match value {
        Some(value) => report[key] = value,
        None => {
            report.as_object_mut().unwrap().remove(key);
        }
    }
    report.to_string()
}

// ─── #203: which checks were switched off ────────────────────────────────────

/// The checks KiCad says it ignored are named in the summary, by rule key
/// and in KiCad's order, whatever the severity filter or the limit.
#[tokio::test]
async fn the_summary_names_the_checks_kicad_ignored() {
    let kicad_listed = json!([
        "missing_courtyard",
        "track_not_centered_on_via",
        "tuning_profile_track_geometries",
        "footprint_filters_mismatch",
        "footprint_type_mismatch"
    ]);
    for arguments in [json!({}), json!({ "severity": "error", "limit": 1 })] {
        let body = run_drc_on(REPORT, arguments).await;
        assert_eq!(body["isError"], false, "{body}");
        assert_eq!(body["ignored_checks"], kicad_listed, "{body}");
    }
}

/// The case the issue is about: a clean result beside a switched-off check.
/// "0 violations" and the check that could not have produced one are in the
/// same response.
#[tokio::test]
async fn a_clean_result_still_names_what_was_not_checked() {
    let mut report: Value = serde_json::from_str(REPORT).unwrap();
    report["violations"] = json!([]);
    report["ignored_checks"] = json!([
        { "key": "silk_edge_clearance", "description": "Silkscreen clipped by board edge" },
        { "key": "silk_overlap", "description": "Silkscreen overlap" }
    ]);

    let body = run_drc_on(&report.to_string(), json!({})).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["total_violations"], 0, "{body}");
    assert_eq!(
        body["ignored_checks"],
        json!(["silk_edge_clearance", "silk_overlap"])
    );
}

/// A report with no `ignored_checks` did not say nothing is ignored: null,
/// not an empty list. An empty list from KiCad is an empty list.
#[tokio::test]
async fn a_report_without_the_list_is_null_not_empty() {
    let absent = run_drc_on(&report_with("ignored_checks", None), json!({})).await;
    assert_eq!(absent["isError"], false, "{absent}");
    assert_eq!(absent["ignored_checks"], Value::Null, "{absent}");

    let none = run_drc_on(&report_with("ignored_checks", Some(json!([]))), json!({})).await;
    assert_eq!(none["ignored_checks"], json!([]), "{none}");
}

// ─── #188: a count per rule ──────────────────────────────────────────────────

/// The table in this file's header, as `by_rule`.
fn fixture_by_rule() -> Value {
    json!({
        "copper_edge_clearance": { "errors": 5, "warnings": 0 },
        "lib_footprint_issues": { "errors": 0, "warnings": 1 },
        "silk_edge_clearance": { "errors": 0, "warnings": 1 }
    })
}

fn listed_rules(body: &Value) -> Vec<&str> {
    body["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|violation| violation["rule"].as_str().unwrap())
        .collect()
}

/// The issue's situation in small: one rule fills the listing. With
/// `limit: 2` both entries shown are `copper_edge_clearance`, and `by_rule`
/// is the only place the other two rules appear.
#[tokio::test]
async fn by_rule_counts_every_rule_however_short_the_listing() {
    let body = run_drc_on(REPORT, json!({ "limit": 2 })).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["by_rule"], fixture_by_rule(), "{body}");
    assert_eq!(
        listed_rules(&body),
        ["copper_edge_clearance", "copper_edge_clearance"]
    );
    assert_eq!(body["shown"], 2);
    assert_eq!(body["truncated"], true);
    // The per-rule counts add up to the totals beside them.
    assert_eq!(body["errors"], 5);
    assert_eq!(body["warnings"], 2);
    assert_eq!(body["filtered_count"], 7);
    assert_eq!(body["rules_filter"], Value::Null);
}

/// `by_rule` is counted over the same set as `errors` and `warnings`: the
/// severity filter applies to it, so a rule with only warnings drops out at
/// `severity: error`.
#[tokio::test]
async fn by_rule_follows_the_severity_filter() {
    let body = run_drc_on(REPORT, json!({ "severity": "error" })).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(
        body["by_rule"],
        json!({ "copper_edge_clearance": { "errors": 5, "warnings": 0 } })
    );
    assert_eq!(body["errors"], 5);
    assert_eq!(body["warnings"], 0);
}

/// Every category is counted, not only `violations`: an unrouted net is
/// reported under `unconnected_items` with the rule of that name (#245).
#[tokio::test]
async fn by_rule_counts_unconnected_items_too() {
    let unconnected = json!([{
        "type": "unconnected_items",
        "severity": "error",
        "description": "Missing connection between items",
        "items": []
    }]);
    let report = report_with("unconnected_items", Some(unconnected));

    let body = run_drc_on(&report, json!({})).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(
        body["by_rule"]["unconnected_items"],
        json!({ "errors": 1, "warnings": 0 })
    );
    assert_eq!(body["by_rule"]["copper_edge_clearance"]["errors"], 5);
}

/// `rules` lists only the named rules, and the limit then applies to those.
/// The summary is not narrowed: it still says what was left out.
#[tokio::test]
async fn rules_lists_only_the_named_rules_and_leaves_the_counts_whole() {
    let body = run_drc_on(
        REPORT,
        json!({ "rules": ["silk_edge_clearance", "lib_footprint_issues"], "limit": 5 }),
    )
    .await;

    assert_eq!(body["isError"], false, "{body}");
    let mut listed = listed_rules(&body);
    listed.sort_unstable();
    assert_eq!(listed, ["lib_footprint_issues", "silk_edge_clearance"]);
    assert_eq!(body["shown"], 2);
    assert_eq!(body["truncated"], false);
    assert_eq!(
        body["rules_filter"],
        json!(["silk_edge_clearance", "lib_footprint_issues"])
    );
    assert_eq!(body["by_rule"], fixture_by_rule(), "{body}");
    assert_eq!(body["filtered_count"], 7);
    assert_eq!(body["errors"], 5);
}

/// `truncated` describes the listing `rules` selected, not the whole report.
#[tokio::test]
async fn truncation_is_measured_on_the_listed_rules() {
    let all = run_drc_on(
        REPORT,
        json!({ "rules": ["copper_edge_clearance"], "limit": 5 }),
    )
    .await;
    assert_eq!(all["shown"], 5, "{all}");
    assert_eq!(all["truncated"], false, "{all}");

    let cut = run_drc_on(
        REPORT,
        json!({ "rules": ["copper_edge_clearance"], "limit": 4 }),
    )
    .await;
    assert_eq!(cut["shown"], 4, "{cut}");
    assert_eq!(cut["truncated"], true, "{cut}");
}

/// A rule nothing broke lists nothing, and `by_rule` shows that the board is
/// not clean for all that.
#[tokio::test]
async fn a_rule_with_no_violations_lists_nothing_beside_whole_counts() {
    let body = run_drc_on(REPORT, json!({ "rules": ["clearance"] })).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["violations"], json!([]));
    assert_eq!(body["shown"], 0);
    assert_eq!(body["by_rule"], fixture_by_rule());
}

/// A malformed `rules` is refused by name. An empty list would otherwise list
/// nothing and read as a clean board.
#[tokio::test]
async fn a_malformed_rules_filter_is_refused_by_name() {
    for (bad, field) in [
        (json!([]), "rules"),
        (json!("clearance"), "rules"),
        (json!(["clearance", 3]), "rules[1]"),
    ] {
        let body = run_drc_on(REPORT, json!({ "rules": bad })).await;
        assert_eq!(body["isError"], true, "{bad}: {body}");
        assert_eq!(body["error"]["kind"], "invalid_argument", "{bad}: {body}");
        assert_eq!(body["error"]["field"], field, "{bad}: {body}");
    }
}

/// The handler holds the same line when reached without the served schema.
#[test]
fn the_handler_checks_the_rules_filter_itself() {
    assert_eq!(rules_argument(&json!({})).unwrap(), None);
    assert_eq!(
        rules_argument(&json!({ "rules": ["clearance"] })).unwrap(),
        Some(vec!["clearance"])
    );
    for bad in [json!([]), json!("clearance"), json!(["clearance", 3])] {
        let refusal = rules_argument(&json!({ "rules": bad })).unwrap_err();
        assert!(refusal.is_error);
        assert_eq!(
            crate::mcp::error::extract_error_kind(&refusal).as_deref(),
            Some("invalid_argument")
        );
    }
}
