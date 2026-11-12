//! `get_drc_severities` / `set_drc_severities` against a KiCad-written
//! project (#203).
//!
//! The oracle is the placement fixture's `.kicad_pro`
//! (`konnect-sexp/tests/fixtures/placement/`), which KiCad 10 saved: its
//! `board.design_settings.rule_severities` holds the 61 checks that KiCad
//! knows, four of them `ignore`. Driven through served `tools/call` unless a
//! test says otherwise.
//!
//! Not shown here, because no KiCad runs in these tests: that KiCad reads the
//! written block back as written. The file format is KiCad's own (a flat map
//! of check key to `error` / `warning` / `ignore`), and the tool only ever
//! changes the value of a key.

use super::design_rules_tests::{body_of, call, direct_ctx, handler, handler_talking_to, project};
use super::*;
use crate::tools::pcb_board::board_mock::spawn_kicad_holding_boards;

/// The checks the fixture's KiCad save has switched off.
const FIXTURE_IGNORED: [&str; 4] = [
    "footprint_filters_mismatch",
    "footprint_type_mismatch",
    "missing_courtyard",
    "tuning_profile_track_geometries",
];

fn assert_invalid_argument(body: &Value, field: &str) {
    assert_eq!(body["isError"], true, "{body}");
    assert_eq!(body["error"]["kind"], "invalid_argument", "{body}");
    assert_eq!(body["error"]["field"], field, "{body}");
}

fn severities_of(project_json: &Value) -> Value {
    project_json["board"]["design_settings"]["rule_severities"].clone()
}

/// Every key KiCad saved comes back with its value, the switched-off ones are
/// listed on their own, and the result says where it was read from.
#[tokio::test]
async fn get_returns_every_severity_kicad_saved_and_names_its_source() {
    let handler = handler().await;
    let project = project();

    let body = call(&handler, "get_drc_severities", &project.board, json!({})).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["severities"], severities_of(&project.json()));
    assert_eq!(body["severities"].as_object().unwrap().len(), 61);
    assert_eq!(body["severities"]["silk_edge_clearance"], "warning");
    assert_eq!(body["severities"]["clearance"], "error");
    assert_eq!(body["ignored_checks"], json!(FIXTURE_IGNORED));
    assert_eq!(body["source"], "project_file");
    assert_eq!(
        body["project"],
        json!(project.project_file.to_string_lossy())
    );
    // No KiCad to ask and no lock beside the board.
    assert_eq!(body["board_open_in_kicad"], false, "{body}");
    assert!(body.get("warning").is_none(), "{body}");
}

/// With the board open, the file can be older than what KiCad holds. The
/// read still answers from the file, and says both things.
#[tokio::test]
async fn get_says_when_kicad_holds_the_board_open() {
    let project = project();
    let kicad = spawn_kicad_holding_boards(&[&project.board], |_| None);
    let handler = handler_talking_to(kicad.address()).await;

    let body = call(&handler, "get_drc_severities", &project.board, json!({})).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["source"], "project_file");
    assert_eq!(body["board_open_in_kicad"], true, "{body}");
    assert!(
        body["warning"]
            .as_str()
            .is_some_and(|warning| warning.contains("older")),
        "{body}"
    );
    assert_eq!(body["severities"], severities_of(&project.json()));
}

/// A KiCad lock beside the board with no KiCad answering: open or not is not
/// known, and the result does not pretend it is.
#[tokio::test]
async fn get_does_not_claim_the_board_is_closed_beside_a_kicad_lock() {
    let handler = handler().await;
    let project = project();
    let lock = konnect_sexp::writer::kicad_editor_lock_path(&project.board).unwrap();
    std::fs::write(&lock, "{}").unwrap();

    let body = call(&handler, "get_drc_severities", &project.board, json!({})).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["board_open_in_kicad"], Value::Null, "{body}");
    assert!(body["warning"].is_string(), "{body}");
}

/// The issue's case: switch the two silkscreen checks back on. Exactly those
/// two values change; every other key KiCad wrote, in the block and outside
/// it, and the board are as they were. The response is the re-read file.
#[tokio::test]
async fn set_merges_the_named_checks_and_keeps_everything_else() {
    let handler = handler().await;
    let project = project();
    let before = project.json();
    let board_before = std::fs::read(&project.board).unwrap();

    let body = call(
        &handler,
        "set_drc_severities",
        &project.board,
        json!({ "severities": {
            "missing_courtyard": "warning",
            "silk_overlap": "error",
            "silk_edge_clearance": "ignore"
        }}),
    )
    .await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(
        body["changed"],
        json!([
            { "check": "missing_courtyard", "from": "ignore", "to": "warning" },
            { "check": "silk_edge_clearance", "from": "warning", "to": "ignore" },
            { "check": "silk_overlap", "from": "warning", "to": "error" }
        ])
    );

    let mut after = project.json();
    assert_eq!(body["severities"], severities_of(&after));
    assert_eq!(body["severities"]["silk_overlap"], "error");
    assert_eq!(
        body["ignored_checks"],
        json!([
            "footprint_filters_mismatch",
            "footprint_type_mismatch",
            "silk_edge_clearance",
            "tuning_profile_track_geometries"
        ])
    );

    // Put the three back: the document is then KiCad's, key for key.
    let block = &mut after["board"]["design_settings"]["rule_severities"];
    block["missing_courtyard"] = json!("ignore");
    block["silk_overlap"] = json!("warning");
    block["silk_edge_clearance"] = json!("warning");
    assert_eq!(after, before);
    assert_eq!(std::fs::read(&project.board).unwrap(), board_before);

    let read = call(&handler, "get_drc_severities", &project.board, json!({})).await;
    assert_eq!(read["severities"], body["severities"]);
}

/// Naming only what the file already holds, or nothing, changes nothing and
/// leaves KiCad's bytes alone.
#[tokio::test]
async fn set_that_changes_nothing_does_not_rewrite_the_project_file() {
    let handler = handler().await;
    for severities in [
        json!({}),
        json!({ "silk_overlap": "warning", "missing_courtyard": "ignore" }),
    ] {
        let project = project();
        let before = project.text();
        let modified = std::fs::metadata(&project.project_file)
            .unwrap()
            .modified()
            .unwrap();

        let body = call(
            &handler,
            "set_drc_severities",
            &project.board,
            json!({ "severities": severities }),
        )
        .await;

        assert_eq!(body["isError"], false, "{body}");
        assert_eq!(body["changed"], json!([]));
        assert_eq!(body["ignored_checks"], json!(FIXTURE_IGNORED));
        assert_eq!(project.text(), before);
        assert_eq!(
            std::fs::metadata(&project.project_file)
                .unwrap()
                .modified()
                .unwrap(),
            modified,
            "the project file was replaced by a no-op"
        );
    }
}

/// KiCad skips a key it does not know when it loads the block and drops it
/// when it saves, so a misspelt check would be reported and then lost. It is
/// refused by name, and the valid check named beside it is not written.
#[tokio::test]
async fn set_refuses_a_check_kicad_did_not_save() {
    let handler = handler().await;
    let project = project();
    let before = project.text();

    let body = call(
        &handler,
        "set_drc_severities",
        &project.board,
        json!({ "severities": { "silk_overlap": "error", "silk_overlaps": "error" } }),
    )
    .await;

    assert_invalid_argument(&body, "severities.silk_overlaps");
    assert_eq!(project.text(), before);
}

/// Only KiCad's three words are severities. The served schema refuses
/// anything else, and so does the handler when reached without it.
#[tokio::test]
async fn set_refuses_a_value_that_is_not_a_severity() {
    let handler = handler().await;
    let ctx = direct_ctx();
    for bad in [
        json!("info"),
        json!("Error"),
        json!(""),
        json!(2),
        json!(null),
    ] {
        let project = project();
        let before = project.text();
        let severities = json!({ "silk_overlap": bad });

        let served = call(
            &handler,
            "set_drc_severities",
            &project.board,
            json!({ "severities": severities }),
        )
        .await;
        assert_invalid_argument(&served, "severities.silk_overlap");

        let args = json!({ "board": project.board, "severities": severities });
        let direct = handle_set_drc_severities(&args, &ctx).await.unwrap();
        assert_invalid_argument(&body_of(&direct), "severities.silk_overlap");

        assert_eq!(project.text(), before, "silk_overlap = {bad}");
    }
}

/// `severities` itself must be a map.
#[tokio::test]
async fn set_refuses_severities_that_are_not_a_map() {
    let ctx = direct_ctx();
    for bad in [json!("ignore"), json!(["silk_overlap"]), json!(null)] {
        let project = project();
        let before = project.text();
        let args = json!({ "board": project.board, "severities": bad });

        let result = handle_set_drc_severities(&args, &ctx).await.unwrap();

        assert_invalid_argument(&body_of(&result), "severities");
        assert_eq!(project.text(), before);
    }
}

/// KiCad rewrites the project file from memory when it saves the board, so a
/// severity written while it holds the board would be reverted (the same
/// reason `set_design_rules` refuses, #791).
#[tokio::test]
async fn set_refuses_while_kicad_holds_the_board() {
    let project = project();
    let before = project.text();
    let board_before = std::fs::read(&project.board).unwrap();
    let kicad = spawn_kicad_holding_boards(&[&project.board], |_| None);
    let handler = handler_talking_to(kicad.address()).await;

    let body_text = {
        let mut arguments = json!({ "severities": { "silk_overlap": "error" } });
        arguments["board"] = json!(project.board.to_string_lossy());
        let response = handler
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "set_drc_severities", "arguments": arguments }
            }))
            .await
            .unwrap();
        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert!(result.is_error);
        match result.content.first() {
            Some(crate::mcp::protocol::ToolContent::Text { text }) => text.clone(),
            other => panic!("expected text content, got {other:?}"),
        }
    };

    assert!(
        body_text.contains("currently holds this board open"),
        "{body_text}"
    );
    assert_eq!(project.text(), before);
    assert_eq!(std::fs::read(&project.board).unwrap(), board_before);
}

/// The control for the refusal above: KiCad holding some other board does not
/// own this project file.
#[tokio::test]
async fn set_still_writes_while_kicad_holds_a_different_board() {
    let project = project();
    let elsewhere = tempfile::tempdir().unwrap();
    let other_board = elsewhere.path().join("other.kicad_pcb");
    std::fs::copy(&project.board, &other_board).unwrap();
    let kicad = spawn_kicad_holding_boards(&[&other_board], |_| None);
    let handler = handler_talking_to(kicad.address()).await;

    let body = call(
        &handler,
        "set_drc_severities",
        &project.board,
        json!({ "severities": { "silk_overlap": "error" } }),
    )
    .await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(severities_of(&project.json())["silk_overlap"], "error");
}

/// A save landing between the read and the write is refused as `conflict`
/// and kept byte for byte, through the guard #197 added for the other
/// project-file writers. The retry reads the newer file and writes on top.
#[tokio::test]
async fn set_refuses_a_project_file_saved_between_read_and_write() {
    use crate::tools::pcb_board::board_write_hook;

    let ctx = direct_ctx();
    let project = project();
    // The save in between: KiCad's file with one more rule changed.
    let mut newer = project.json();
    newer["board"]["design_settings"]["rules"]["min_track_width"] = json!(0.35);
    let newer = serde_json::to_string_pretty(&newer).unwrap();
    let args = json!({ "board": project.board, "severities": { "silk_overlap": "error" } });

    let intervening = newer.clone();
    board_write_hook::set(move |path| std::fs::write(path, &intervening).unwrap());
    let refused = handle_set_drc_severities(&args, &ctx).await.unwrap();

    assert!(
        !board_write_hook::armed(),
        "set_drc_severities bypassed the revision-checked write"
    );
    let body = body_of(&refused);
    assert_eq!(body["isError"], true, "{body}");
    assert_eq!(body["error"]["kind"], "conflict", "{body}");
    assert_eq!(
        body["error"]["paths"],
        json!([project.project_file.display().to_string()])
    );
    assert_eq!(project.text(), newer, "the newer save must be kept");

    let retried = handle_set_drc_severities(&args, &ctx).await.unwrap();
    assert!(!retried.is_error, "{}", body_of(&retried));
    let after = project.json();
    assert_eq!(severities_of(&after)["silk_overlap"], "error");
    assert_eq!(
        after["board"]["design_settings"]["rules"]["min_track_width"],
        json!(0.35)
    );
}

/// A project KiCad has not saved yet (Konnect's `create_project` writes an
/// empty `design_settings`) has no block to read: the read says so instead of
/// inventing defaults, and a write is checked against the checks KiCad 10
/// knows, since the file names none.
#[tokio::test]
async fn a_project_with_no_severities_block_reads_empty_and_takes_known_checks() {
    let handler = handler().await;
    let project = project();
    let mut bare = project.json();
    bare["board"]["design_settings"]
        .as_object_mut()
        .unwrap()
        .remove("rule_severities");
    std::fs::write(
        &project.project_file,
        serde_json::to_string_pretty(&bare).unwrap(),
    )
    .unwrap();
    let before = project.text();

    let read = call(&handler, "get_drc_severities", &project.board, json!({})).await;
    assert_eq!(read["isError"], false, "{read}");
    assert_eq!(read["severities"], json!({}));
    assert_eq!(read["ignored_checks"], Value::Null, "{read}");
    assert!(read["note"].is_string(), "{read}");

    let unknown = call(
        &handler,
        "set_drc_severities",
        &project.board,
        json!({ "severities": { "not_a_check": "ignore" } }),
    )
    .await;
    assert_invalid_argument(&unknown, "severities.not_a_check");
    assert_eq!(project.text(), before);

    let body = call(
        &handler,
        "set_drc_severities",
        &project.board,
        json!({ "severities": { "silk_overlap": "error" } }),
    )
    .await;
    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(
        body["changed"],
        json!([{ "check": "silk_overlap", "from": null, "to": "error" }])
    );
    assert_eq!(
        severities_of(&project.json()),
        json!({ "silk_overlap": "error" })
    );
}

/// The built-in list is the fallback for a project with no block, and it is
/// exactly what KiCad 10 saved into the fixture.
#[test]
fn the_built_in_checks_are_the_ones_kicad_10_saved() {
    let saved = severities_of(&project().json());
    let saved: Vec<&str> = saved
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(KICAD_10_DRC_CHECKS, saved.as_slice());
}

/// No project file at all is a refusal naming the path, for both tools.
#[tokio::test]
async fn a_missing_project_file_is_named() {
    let handler = handler().await;
    for (tool, arguments) in [
        ("get_drc_severities", json!({})),
        (
            "set_drc_severities",
            json!({ "severities": { "silk_overlap": "error" } }),
        ),
    ] {
        let project = project();
        std::fs::remove_file(&project.project_file).unwrap();

        let body = call(&handler, tool, &project.board, arguments).await;

        assert_eq!(body["isError"], true, "{tool}: {body}");
        assert_eq!(body["error"]["kind"], "file_not_found", "{tool}: {body}");
        assert!(!project.project_file.exists(), "{tool} created the file");
    }
}
