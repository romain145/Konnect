//! `set_design_rules` / `get_design_rules` against a KiCad-written project.
//!
//! Driven through served `tools/call` unless a test says otherwise, so the
//! schema, the dispatch and the handler are all under test. The project is the
//! placement fixture (`konnect-sexp/tests/fixtures/placement/`), whose
//! `.kicad_pro` KiCad 10 saved: its `board.design_settings.rules` holds every
//! key KiCad writes, which is what "unrelated settings are preserved" is
//! checked against.
//!
//! What these tests cannot show is KiCad's side of #196, that a value outside
//! the range is replaced by the default on load. That was measured on KiCad
//! 10.0.6 in the issue, and the ranges are the ones KiCad 10.0 registers in
//! `pcbnew/board_design_settings.cpp`.

use super::*;
use crate::mcp::handler::McpHandler;
use crate::mcp::protocol::ToolContent;
use crate::tools::ServerConfig;

const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../konnect-sexp/tests/fixtures/placement"
);

/// A copy of the fixture project, kept alive for the test's duration.
pub(super) struct Project {
    _dir: tempfile::TempDir,
    pub(super) board: PathBuf,
    pub(super) project_file: PathBuf,
}

impl Project {
    pub(super) fn text(&self) -> String {
        std::fs::read_to_string(&self.project_file).unwrap()
    }

    pub(super) fn json(&self) -> Value {
        serde_json::from_str(&self.text()).unwrap()
    }

    fn rules(&self) -> Value {
        self.json()["board"]["design_settings"]["rules"].clone()
    }
}

pub(super) fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    for extension in ["kicad_pcb", "kicad_pro"] {
        std::fs::copy(
            Path::new(FIXTURE_DIR).join(format!("placement_fixture.{extension}")),
            dir.path().join(format!("placement_fixture.{extension}")),
        )
        .unwrap();
    }
    Project {
        board: dir.path().join("placement_fixture.kicad_pcb"),
        project_file: dir.path().join("placement_fixture.kicad_pro"),
        _dir: dir,
    }
}

fn config(ipc_address: &str) -> ServerConfig {
    ServerConfig {
        kicad_cli: String::new(),
        kicad_binary: String::new(),
        ipc_address: ipc_address.to_string(),
        project_dir: None,
        jlcpcb_db_path: None,
        auto_load_toolsets: true,
        eager_toolsets: true,
    }
}

pub(super) async fn handler_talking_to(ipc_address: &str) -> McpHandler {
    McpHandler::new(config(ipc_address))
        .await
        .expect("handler builds")
}

/// A handler with no KiCad to talk to.
pub(super) async fn handler() -> McpHandler {
    handler_talking_to("").await
}

pub(super) fn direct_ctx() -> ToolContext {
    ToolContext::new(
        config(""),
        std::sync::Arc::new(crate::router::ToolRouter::new()),
    )
}

/// The body of a served `tools/call`, with `isError` folded in.
pub(super) async fn call(handler: &McpHandler, tool: &str, board: &Path, extra: Value) -> Value {
    let mut arguments = extra;
    arguments["board"] = json!(board.to_string_lossy());
    let response = handler
        .handle_message(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments }
        }))
        .await
        .expect("tools/call receives a response");
    assert!(response.error.is_none(), "tool errors are MCP results");
    let result: CallToolResult = serde_json::from_value(response.result.expect("a result"))
        .expect("result uses the advertised MCP shape");
    body_of(&result)
}

pub(super) fn body_of(result: &CallToolResult) -> Value {
    let text = match result.content.first() {
        Some(ToolContent::Text { text }) => text,
        other => panic!("expected text content, got {other:?}"),
    };
    let mut body: Value = serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("response is not JSON ({error}): {text}"));
    body["isError"] = json!(result.is_error);
    body
}

fn assert_invalid_argument(body: &Value, field: &str) {
    assert_eq!(body["isError"], true, "{body}");
    assert_eq!(body["error"]["kind"], "invalid_argument", "{body}");
    assert_eq!(body["error"]["field"], field, "{body}");
}

/// #196: every value KiCad would reset on load is refused by name, and the
/// project file is left byte for byte. The values are the ones the issue
/// measured on KiCad 10.0.6, just outside each end of the range.
///
/// One case here is Konnect's line rather than KiCad's: KiCad loads -0.01 for
/// `min_copper_edge_clearance` as its pre-6.0 flag, and the tool refuses it
/// because it is not a clearance.
#[tokio::test]
async fn a_value_kicad_would_reset_on_load_is_refused_and_nothing_is_written() {
    let handler = handler().await;
    for rule in design_rules() {
        let (minimum, maximum) = rule.range_mm;
        for outside in [minimum - 0.01, maximum + 0.01] {
            let project = project();
            let before = project.text();

            let body = call(
                &handler,
                "set_design_rules",
                &project.board,
                json!({ rule.argument: outside }),
            )
            .await;

            assert_invalid_argument(&body, rule.argument);
            assert_eq!(
                project.text(),
                before,
                "{} = {outside} was refused but the project file changed",
                rule.argument
            );
        }
    }
}

/// The ranges are not all alike: 10.01 mm loads for every other rule and is
/// reset for `min_hole_to_hole`, as #196's table shows. A shared bound would
/// get one of the two wrong.
#[tokio::test]
async fn hole_to_hole_has_its_own_narrower_range() {
    let handler = handler().await;
    for rule in design_rules() {
        let project = project();
        let body = call(
            &handler,
            "set_design_rules",
            &project.board,
            json!({ rule.argument: 10.01 }),
        )
        .await;

        if rule.argument == "min_hole_to_hole" {
            assert_invalid_argument(&body, "min_hole_to_hole");
            assert_eq!(project.rules()["min_hole_to_hole"], json!(0.25));
        } else {
            assert_eq!(body["isError"], false, "{}: {body}", rule.argument);
            assert_eq!(
                project.rules()[rule.project_key],
                json!(10.01),
                "{}",
                rule.argument
            );
        }
    }
}

/// Both ends of each range are values KiCad keeps, so they are written.
#[tokio::test]
async fn the_bounds_of_each_range_are_accepted_and_written() {
    let handler = handler().await;
    for rule in design_rules() {
        for bound in [rule.range_mm.0, rule.range_mm.1] {
            let project = project();
            let body = call(
                &handler,
                "set_design_rules",
                &project.board,
                json!({ rule.argument: bound }),
            )
            .await;
            assert_eq!(
                body["isError"], false,
                "{} = {bound}: {body}",
                rule.argument
            );
            assert_eq!(
                project.rules()[rule.project_key].as_f64(),
                Some(bound),
                "{}",
                rule.argument
            );
        }
    }
}

/// One refused value refuses the call: the good value named beside it is not
/// written, so the caller never has to work out which half landed.
#[tokio::test]
async fn one_refused_value_writes_none_of_the_others() {
    let handler = handler().await;
    let project = project();
    let before = project.text();

    let body = call(
        &handler,
        "set_design_rules",
        &project.board,
        json!({ "min_clearance": 0.15, "min_hole_to_hole": 12 }),
    )
    .await;

    assert_invalid_argument(&body, "min_hole_to_hole");
    assert_eq!(project.text(), before);
}

/// The schema is the first refusal a served call meets. The handler must hold
/// the same line on its own, for a caller that reaches it without the
/// dispatch, and must name a value of the wrong type rather than skip it.
#[tokio::test]
async fn the_handler_refuses_on_its_own_what_the_schema_refuses() {
    let ctx = direct_ctx();
    for rule in design_rules() {
        for bad in [
            json!(rule.range_mm.0 - 0.01),
            json!(rule.range_mm.1 + 0.01),
            json!("0.2"),
            json!(true),
        ] {
            let project = project();
            let before = project.text();
            let args = json!({ "board": project.board, rule.argument: bad });

            let result = handle_set_design_rules(&args, &ctx).await.unwrap();

            assert_invalid_argument(&body_of(&result), rule.argument);
            assert_eq!(project.text(), before, "{} = {bad}", rule.argument);
        }
    }
}

/// The range is advertised where a client reads it, and it is the table's.
#[test]
fn the_schema_advertises_each_rule_range() {
    let tools = tools();
    let schema = &tools
        .iter()
        .find(|tool| tool.name == "set_design_rules")
        .expect("set_design_rules is registered")
        .input_schema;

    for rule in design_rules() {
        let property = &schema["properties"][rule.argument];
        assert_eq!(property["type"], "number", "{}", rule.argument);
        assert_eq!(
            property["minimum"].as_f64(),
            Some(rule.range_mm.0),
            "{}",
            rule.argument
        );
        assert_eq!(
            property["maximum"].as_f64(),
            Some(rule.range_mm.1),
            "{}",
            rule.argument
        );
    }
    // The measured ranges of #196, stated here so a slip in the table fails.
    let range = |argument: &str| {
        let property = &schema["properties"][argument];
        (
            property["minimum"].as_f64().unwrap(),
            property["maximum"].as_f64().unwrap(),
        )
    };
    assert_eq!(range("min_clearance"), (0.0, 25.0));
    assert_eq!(range("min_trace_width"), (0.0, 25.0));
    assert_eq!(range("min_via_drill"), (0.0, 25.0));
    assert_eq!(range("min_via_size"), (0.0, 25.0));
    assert_eq!(range("min_hole_to_hole"), (0.0, 10.0));
    // #194's three, as KiCad 10.0 registers them. Text height alone goes to
    // 100 mm.
    assert_eq!(range("min_copper_edge_clearance"), (0.0, 25.0));
    assert_eq!(range("min_text_height"), (0.0, 100.0));
    assert_eq!(range("min_text_thickness"), (0.0, 25.0));
}

// ─── #194: copper-to-edge clearance and the silkscreen text minima ───────────

/// The three minima #194 adds, as argument and project key (they are the
/// same word), with the value the KiCad-saved fixture holds.
const FAB_MINIMA: &[(&str, f64)] = &[
    ("min_copper_edge_clearance", 0.5),
    ("min_text_height", 0.8),
    ("min_text_thickness", 0.08),
];

/// `get_design_rules` reports what KiCad saved for the three, beside the five
/// it already reported.
#[tokio::test]
async fn get_design_rules_reports_the_edge_clearance_and_text_minima_kicad_saved() {
    let handler = handler().await;
    let project = project();

    let body = call(&handler, "get_design_rules", &project.board, json!({})).await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(
        body["rules"],
        json!({
            "min_clearance": 0.0,
            "min_trace_width": 0.2,
            "min_via_drill": 0.3,
            "min_via_size": 0.5,
            "min_hole_to_hole": 0.25,
            "min_copper_edge_clearance": 0.5,
            "min_text_height": 0.8,
            "min_text_thickness": 0.08
        })
    );
}

/// Setting a fab's three values (JLCPCB's, from the issue) changes exactly
/// those keys. Everything else KiCad wrote, in `rules` and outside it, and the
/// board file are as they were; the response is what the file now holds.
#[tokio::test]
async fn setting_the_three_minima_changes_only_those_keys() {
    let handler = handler().await;
    let project = project();
    let before = project.json();
    let board_before = std::fs::read(&project.board).unwrap();

    let body = call(
        &handler,
        "set_design_rules",
        &project.board,
        json!({
            "min_copper_edge_clearance": 0.2,
            "min_text_height": 1.0,
            "min_text_thickness": 0.15
        }),
    )
    .await;

    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["changed"].as_array().map(Vec::len), Some(3), "{body}");
    assert_eq!(body["rules"]["min_copper_edge_clearance"], json!(0.2));
    assert_eq!(body["rules"]["min_text_height"], json!(1.0));
    assert_eq!(body["rules"]["min_text_thickness"], json!(0.15));
    // Adjacent constraints, read back from the same file.
    assert_eq!(body["rules"]["min_trace_width"], json!(0.2));
    assert_eq!(body["rules"]["min_hole_to_hole"], json!(0.25));

    // The file differs from KiCad's save in those three values and nowhere
    // else: put them back and the documents are equal.
    let mut after = project.json();
    let rules = &mut after["board"]["design_settings"]["rules"];
    assert_eq!(rules["min_copper_edge_clearance"], json!(0.2));
    assert_eq!(rules["min_text_height"], json!(1.0));
    assert_eq!(rules["min_text_thickness"], json!(0.15));
    for (key, saved) in FAB_MINIMA {
        rules[*key] = json!(saved);
    }
    assert_eq!(after, before);
    assert_eq!(std::fs::read(&project.board).unwrap(), board_before);

    // And `get_design_rules` agrees with the write's own readback.
    let read = call(&handler, "get_design_rules", &project.board, json!({})).await;
    assert_eq!(read["rules"], body["rules"]);
}

/// Each of the three on its own: the other two keep KiCad's value.
#[tokio::test]
async fn naming_one_minimum_leaves_the_other_two_as_saved() {
    let handler = handler().await;
    for (named, _) in FAB_MINIMA {
        let project = project();
        let body = call(
            &handler,
            "set_design_rules",
            &project.board,
            json!({ *named: 0.3 }),
        )
        .await;
        assert_eq!(body["isError"], false, "{named}: {body}");
        assert_eq!(body["changed"], json!([format!("{named} = 0.3")]));

        let rules = project.rules();
        for (key, saved) in FAB_MINIMA {
            let expected = if key == named { 0.3 } else { *saved };
            assert_eq!(rules[*key], json!(expected), "{named} set, reading {key}");
        }
    }
}

/// A call naming only values the file already holds, or no value at all,
/// reports no change and leaves KiCad's bytes alone. A rewrite would
/// re-serialise the document in Konnect's formatting for nothing.
#[tokio::test]
async fn a_call_that_changes_nothing_does_not_rewrite_the_project_file() {
    let handler = handler().await;
    for arguments in [
        json!({}),
        json!({
            "min_copper_edge_clearance": 0.5,
            "min_text_height": 0.8,
            "min_text_thickness": 0.08,
            "min_trace_width": 0.2
        }),
    ] {
        let project = project();
        let before = project.text();
        let modified = std::fs::metadata(&project.project_file)
            .unwrap()
            .modified()
            .unwrap();

        let body = call(&handler, "set_design_rules", &project.board, arguments).await;

        assert_eq!(body["isError"], false, "{body}");
        assert_eq!(body["changed"], json!([]));
        assert_eq!(body["rules"]["min_text_height"], json!(0.8));
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

/// A project file with no `rules` yet: the named minimum is created, and
/// what is still absent reads as null rather than a number KiCad never
/// stored.
#[tokio::test]
async fn a_minimum_is_created_in_a_project_that_has_no_rules_yet() {
    let handler = handler().await;
    let project = project();
    let mut bare = project.json();
    bare["board"]["design_settings"]
        .as_object_mut()
        .unwrap()
        .remove("rules");
    std::fs::write(
        &project.project_file,
        serde_json::to_string_pretty(&bare).unwrap(),
    )
    .unwrap();

    let read = call(&handler, "get_design_rules", &project.board, json!({})).await;
    assert_eq!(read["rules"]["min_text_height"], Value::Null, "{read}");

    let body = call(
        &handler,
        "set_design_rules",
        &project.board,
        json!({ "min_text_height": 1.0 }),
    )
    .await;
    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["rules"]["min_text_height"], json!(1.0));
    assert_eq!(body["rules"]["min_text_thickness"], Value::Null);
    assert_eq!(project.rules(), json!({ "min_text_height": 1.0 }));
}

/// A value of the wrong type is refused by the served dispatch, naming the
/// field, before the file is touched.
#[tokio::test]
async fn a_minimum_of_the_wrong_type_is_refused_by_name() {
    let handler = handler().await;
    for (argument, _) in FAB_MINIMA {
        for bad in [json!("1.0"), json!(true), json!([1.0])] {
            let project = project();
            let before = project.text();
            let body = call(
                &handler,
                "set_design_rules",
                &project.board,
                json!({ *argument: bad }),
            )
            .await;
            assert_invalid_argument(&body, argument);
            assert_eq!(project.text(), before, "{argument} = {bad}");
        }
    }
}
