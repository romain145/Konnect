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

/// The ranges are not all alike: 10.01 mm loads for the four 25 mm rules and
/// is reset for `min_hole_to_hole`, as #196's table shows. A shared bound
/// would get one of the two wrong.
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
}
