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
