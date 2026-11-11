use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// KiCad design editor addressed by the typed IPC API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcEditorKind {
    Schematic,
    Pcb,
}

impl IpcEditorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schematic => "schematic",
            Self::Pcb => "pcb",
        }
    }
}

/// Runtime availability of one semantic editor capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcCapabilityAvailability {
    Available,
    Unsupported,
    Unknown,
}

/// One capability statement and the evidence used to make it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcCapability {
    pub availability: IpcCapabilityAvailability,
    pub evidence_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Capabilities relevant to the Priority 1 semantic navigation surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcEditorCapabilities {
    pub observe_documents: IpcCapability,
    pub observe_active_context: IpcCapability,
    pub read_selection: IpcCapability,
    pub mutate_selection: IpcCapability,
    pub activate_document: IpcCapability,
    pub activate_sheet: IpcCapability,
    pub reveal_object: IpcCapability,
    pub center_object: IpcCapability,
    pub fit_view: IpcCapability,
    pub cross_probe: IpcCapability,
}

/// Running KiCad version observed through `GetVersion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcKiCadVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub full_version: String,
}

/// Project identity carried by a live KiCad `DocumentSpecifier`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcProjectIdentity {
    pub name: String,
    pub path: String,
}

/// Canonical schematic instance identity carried by KiCad IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcSheetInstancePath {
    pub kiids: Vec<String>,
    pub human_readable: String,
}

/// One exact live document identity observed through KiCad IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcEditorDocument {
    pub editor: IpcEditorKind,
    pub project: Option<IpcProjectIdentity>,
    /// Exact board path when KiCad provides one. KiCad 10 schematic document
    /// specifiers carry a sheet path rather than a schematic filename, so this
    /// is deliberately null for schematics instead of being inferred from disk.
    pub document_path: Option<String>,
    pub sheet_instance_path: Option<IpcSheetInstancePath>,
}

/// Observation for one editor kind on the configured IPC endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcEditorObservation {
    pub editor: IpcEditorKind,
    pub addressable: bool,
    pub documents: Vec<IpcEditorDocument>,
    pub capabilities: IpcEditorCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

/// Result of observing the configured KiCad IPC endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcEditorStateObservation {
    pub kicad_version: IpcKiCadVersion,
    pub evidence_source: String,
    pub editors: Vec<IpcEditorObservation>,
    /// KiCad 10 has no stable typed foreground-frame or active-document query.
    /// These fields remain null rather than treating open-document order as
    /// active state.
    pub active_editor: Option<IpcEditorKind>,
    pub active_document: Option<IpcEditorDocument>,
    pub active_sheet_instance: Option<IpcSheetInstancePath>,
    pub limitations: Vec<String>,
}

/// One selected object observed from the exact requested live document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcSelectedObject {
    /// Stable KiCad object identifier (KIID/UUID), never a display reference.
    pub kiid: String,
    /// Semantic object kind derived from the exact protobuf type URL.
    pub object_type: String,
    /// Full protobuf type carried by KiCad, retained as decoding evidence.
    pub protocol_type: String,
}

/// Selection readback bound to one exact live editor/document/sheet context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcSelectionObservation {
    pub project: Option<IpcProjectIdentity>,
    pub document: IpcEditorDocument,
    pub editor: IpcEditorKind,
    pub sheet_instance_path: Option<IpcSheetInstancePath>,
    pub selected_objects: Vec<IpcSelectedObject>,
    pub evidence_source: String,
}

/// Semantic selection change requested from one exact editor context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcSelectionMutation {
    Clear,
    Add,
    Remove,
}

impl IpcSelectionMutation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }
}

/// Verified selection mutation. Success means the post-operation observation
/// exactly matched the requested set transition, not merely that IPC replied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcSelectionMutationResult {
    pub operation: IpcSelectionMutation,
    pub requested_kiids: Vec<String>,
    pub before: IpcSelectionObservation,
    pub after: IpcSelectionObservation,
    pub evidence_source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcSelectionMutationErrorKind {
    InvalidRequest,
    ReadbackMismatch,
}

/// A semantic selection mutation was invalid or its observed result did not
/// prove the requested change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcSelectionMutationError {
    pub kind: IpcSelectionMutationErrorKind,
    pub operation: IpcSelectionMutation,
    pub requested_kiids: Vec<String>,
    pub before_kiids: Vec<String>,
    pub after_kiids: Vec<String>,
    pub reason: String,
}

impl IpcSelectionMutationError {
    pub fn from_error(error: &anyhow::Error) -> Option<&Self> {
        error.chain().find_map(|cause| cause.downcast_ref::<Self>())
    }
}

impl std::fmt::Display for IpcSelectionMutationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot verify {} selection mutation: {}",
            self.operation.as_str(),
            self.reason
        )
    }
}

impl std::error::Error for IpcSelectionMutationError {}

/// Stable classification for a fail-closed selection observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcSelectionObservationErrorKind {
    WrongProject,
    WrongDocument,
    WrongSheetInstance,
    AmbiguousDocument,
    StaleEditorState,
    UnsupportedObjectType,
    MalformedSelectedObject,
}

/// A selection could not be attributed to the exact requested live context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcSelectionObservationError {
    pub kind: IpcSelectionObservationErrorKind,
    pub editor: IpcEditorKind,
    pub requested: String,
    pub candidates: Vec<String>,
    pub reason: String,
}

impl IpcSelectionObservationError {
    pub fn from_error(error: &anyhow::Error) -> Option<&Self> {
        error.chain().find_map(|cause| cause.downcast_ref::<Self>())
    }
}

impl std::fmt::Display for IpcSelectionObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot observe {} selection for {}: {}",
            self.editor.as_str(),
            self.requested,
            self.reason
        )
    }
}

impl std::error::Error for IpcSelectionObservationError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcVector2 {
    pub x: f64,
    pub y: f64,
}

/// One copper-layer shape from a KiCad pad stack.
///
/// `layer` is the native KiCad layer name.  A normal stack may expose one
/// representative copper entry while `IpcPad::layers` still names every layer
/// on which the pad exists. Front/inner/back stacks retain every distinct
/// entry here; custom shapes are identified as `custom` rather than reduced to
/// a guessed ordinary outline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcPadLayerGeometry {
    pub layer: String,
    pub shape: Option<String>,
    pub size: Option<IpcVector2>,
    pub offset: Option<IpcVector2>,
    pub corner_rounding_ratio: Option<f64>,
    pub chamfer_ratio: Option<f64>,
}

/// Native drill information for a pad, when it has a drilled hole.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcPadDrill {
    pub shape: Option<String>,
    pub size: Option<IpcVector2>,
    pub start_layer: Option<String>,
    pub end_layer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcFootprint {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub position: IpcVector2,
    pub rotation: f64,
    pub layer: String,
}

/// A pad of a footprint placed on the board, read back from KiCad.
///
/// Coordinates are absolute board millimetres: KiCad serializes a
/// `FootprintInstance`'s children in board space (see the `transform` module),
/// so no anchor or rotation transform is applied on the way out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcPad {
    /// Stable KiCad item identifier, or `None` when the source did not carry
    /// one.  Never synthesize an identifier for a pad that KiCad did not name.
    pub uuid: Option<String>,
    pub number: String,
    pub x: f64,
    pub y: f64,
    /// Net name, empty when the pad carries no net.
    pub net: String,
    /// KiCad layer names from the live pad stack.
    pub layers: Vec<String>,
    /// Native KiCad pad type (`smd`, `thru_hole`, `np_thru_hole`, or
    /// `edge_connector`), or `None` for an unknown future type.
    pub pad_type: Option<String>,
    /// Effective pad-stack rotation in board space, in degrees.
    pub rotation_deg: Option<f64>,
    /// Compact geometry for ordinary pads.  These mirror the first copper
    /// entry; callers needing custom/front-inner-back detail use
    /// `copper_layers` instead.
    pub shape: Option<String>,
    pub size: Option<IpcVector2>,
    pub drill: Option<IpcPadDrill>,
    /// Every copper geometry entry KiCad supplied.  This prevents custom and
    /// front/inner/back pad stacks from being flattened into a guessed shape.
    pub copper_layers: Vec<IpcPadLayerGeometry>,
}

/// The document's title block, which the board file also carries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IpcTitleBlock {
    pub title: String,
    pub date: String,
    pub revision: String,
    pub company: String,
}

/// Complete target placement for one existing footprint.
///
/// Keeping the four values together lets the IPC client transform all selected
/// footprints from one board snapshot and publish them in one undoable update,
/// instead of issuing a move and a rotation as separate round trips.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcFootprintPlacement {
    pub reference: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct IpcPadDefinition {
    pub number: String,
    pub pad_type: String,
    pub shape: String,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub size_x: f64,
    pub size_y: f64,
    pub drill_x: Option<f64>,
    pub drill_y: Option<f64>,
    pub drill_oval: bool,
    pub layers: Vec<String>,
    pub roundrect_ratio: f64,
}

/// A footprint graphic item in footprint-local coordinates (mm), parsed from
/// the library `.kicad_mod` source.
///
/// Points are pre-transform: `build_footprint_item` rotates and translates
/// them into absolute board coordinates before emission, because KiCAD
/// serializes `FootprintInstance` children in absolute board space (see the
/// `transform` module docs / issue #23).
#[derive(Debug, Clone, PartialEq)]
pub enum IpcGraphicDefinition {
    /// `fp_line` — straight segment.
    Line {
        start: (f64, f64),
        end: (f64, f64),
        layer: String,
        width: f64,
    },
    /// `fp_rect` — axis-aligned rectangle between two opposite corners.
    Rect {
        start: (f64, f64),
        end: (f64, f64),
        layer: String,
        width: f64,
        filled: bool,
    },
    /// `fp_circle` — center plus a point on the circumference.
    Circle {
        center: (f64, f64),
        end: (f64, f64),
        layer: String,
        width: f64,
        filled: bool,
    },
    /// `fp_arc` — start / mid / end points.
    Arc {
        start: (f64, f64),
        mid: (f64, f64),
        end: (f64, f64),
        layer: String,
        width: f64,
    },
    /// `fp_poly` — closed outline.
    Poly {
        points: Vec<(f64, f64)>,
        layer: String,
        width: f64,
        filled: bool,
    },
    /// Visible `fp_text` / `property` text.
    Text {
        text: String,
        position: (f64, f64),
        /// Text angle in degrees, footprint-local.
        rotation: f64,
        layer: String,
        /// Glyph size (width and height) in mm.
        size: f64,
        /// Font stroke width in mm.
        stroke_width_mm: f64,
    },
}

impl IpcGraphicDefinition {
    /// The KiCAD layer name this item draws on.
    pub fn layer(&self) -> &str {
        match self {
            Self::Line { layer, .. }
            | Self::Rect { layer, .. }
            | Self::Circle { layer, .. }
            | Self::Arc { layer, .. }
            | Self::Poly { layer, .. }
            | Self::Text { layer, .. } => layer,
        }
    }

    /// What this item is, for an error that has to name it.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Line { .. } => "fp_line",
            Self::Rect { .. } => "fp_rect",
            Self::Circle { .. } => "fp_circle",
            Self::Arc { .. } => "fp_arc",
            Self::Poly { .. } => "fp_poly",
            Self::Text { .. } => "fp_text",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcTrack {
    /// KIID of the track, needed to delete it via delete_track. Empty only if
    /// KiCAD returned a track without an id.
    pub uuid: String,
    pub net_name: String,
    pub layer: String,
    pub width: f64,
    pub start: IpcVector2,
    pub end: IpcVector2,
}

/// A graphic item inside a placed footprint — silkscreen, fabrication, or
/// courtyard artwork, not a pad.
///
/// `points` are footprint-local millimetres, matching what the `.kicad_mod`
/// shows, even though KiCad carries them in absolute board coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcFootprintGraphic {
    pub uuid: String,
    pub kind: String,
    pub layer: String,
    pub points: Vec<IpcVector2>,
    /// How many outlines a polygon's `PolySet` carries, and how many holes
    /// across them; `0` for every other kind. `points` reports the first
    /// outline only, so anything above `1` outline or above `0` holes means
    /// this listing is not the whole shape — hence stating it rather than
    /// letting the caller infer a simple triangle from three points.
    pub outlines: usize,
    pub holes: usize,
    /// Whether `edit_board_footprint_graphic` can address this item: a
    /// single-outline polygon with no holes, carrying a UUID.
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcNet {
    pub name: String,
    pub netcode: i32,
}

/// A board graphic — a shape, text, textbox, or dimension — read back from
/// KiCad.
///
/// `kind` is normalized (`line`, `rect`, `arc`, `circle`, `poly`, `curve`,
/// `text`, `textbox`, `dimension`) so the live and the file reader answer in
/// one vocabulary rather than protobuf names on one side and `gr_*` file tags
/// on the other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcGraphic {
    pub uuid: String,
    pub kind: String,
    pub layer: String,
    /// First defining point in mm: a segment's start, a rectangle's top-left,
    /// an arc's start, a circle's centre, a polygon's first vertex, a text's
    /// position. `None` when KiCad sent no geometry.
    pub origin: Option<IpcVector2>,
}

/// Effective PCB routing rules returned by KiCad for one net.
///
/// Values are optional because KiCad's protobuf permits an incomplete class.
/// A routing exporter must refuse an incomplete rule set rather than inventing
/// manufacturing constraints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcRoutingRules {
    pub class_name: String,
    pub constituents: Vec<String>,
    pub track_width_mm: Option<f64>,
    pub clearance_mm: Option<f64>,
    pub via_diameter_mm: Option<f64>,
    pub via_drill_mm: Option<f64>,
}

/// Net name to its effective (merged) KiCad routing rules.
pub type IpcEffectiveRoutingRules = BTreeMap<String, IpcRoutingRules>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcLayer {
    pub name: String,
    pub id: i32,
    pub kind: String,
    /// The name KiCad shows for the layer, which the user may have renamed.
    /// `None` when the enabled set was read without asking for it — an empty
    /// string is a name KiCad could have answered with, so absence needs its
    /// own spelling.
    pub display_name: Option<String>,
}

/// The enabled layer set as KiCad reports it.
///
/// `copper_layer_count` is the response's own field, not a count of `layers`
/// whose name ends in `.Cu` — the two agree on an ordinary stackup, and that
/// agreement is exactly what stops holding on an unusual one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnabledLayers {
    pub copper_layer_count: u32,
    pub layers: Vec<IpcLayer>,
}

/// A board's physical stackup, as KiCad's `GetBoardStackup` serves it.
///
/// KiCad answers with the board's own stackup, or with its default one when
/// the board defines none (`BOARD::GetStackupOrDefault`), and the message does
/// not say which. Values KiCad marks "Not specified" are kept as it sends them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcBoardStackup {
    /// The copper finish KiCad names, e.g. "ENIG"; empty or "Not specified"
    /// when the board names none.
    pub finish: String,
    pub impedance_controlled: bool,
    /// "none", "plain", "beveled" or "unknown".
    pub edge_connector: String,
    pub has_edge_plating: bool,
    /// Top to bottom, in KiCad's order.
    pub layers: Vec<IpcStackupLayer>,
}

/// One entry of a physical stackup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcStackupLayer {
    /// KiCad's canonical layer name, e.g. "F.Cu" or "F.Mask". `None` for a
    /// dielectric, which is not a board layer of its own.
    pub layer: Option<String>,
    /// The name the board gives the layer, when KiCad reports one.
    pub user_name: Option<String>,
    /// "copper", "dielectric", "soldermask", "silkscreen", "solderpaste" or
    /// "undefined".
    pub kind: String,
    pub enabled: bool,
    /// The entry's thickness in nanometres. A dielectric's own value is its
    /// first sub-layer's; `dielectric` carries every sub-layer.
    pub thickness_nm: i64,
    /// Empty when KiCad reports none. A dielectric's materials are per
    /// sub-layer, in `dielectric`.
    pub material: String,
    /// `#RRGGBBAA`, only when the board specifies a color.
    pub color: Option<String>,
    /// "core", "prepreg", "none", or "unknown" for a value this crate does not
    /// know, for a dielectric.
    pub dielectric_type: Option<String>,
    /// A dielectric's physical sub-layers, top to bottom.
    pub dielectric: Vec<IpcStackupDielectric>,
    /// A solder mask's relative permittivity.
    pub epsilon_r: Option<f64>,
    /// A solder mask's loss tangent.
    pub loss_tangent: Option<f64>,
}

/// One physical sub-layer of a dielectric slot. A slot between two copper
/// layers can be built from several.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcStackupDielectric {
    pub thickness_nm: i64,
    pub material: String,
    pub epsilon_r: f64,
    pub loss_tangent: f64,
    pub thickness_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcBoardExtents {
    pub min: IpcVector2,
    pub max: IpcVector2,
}

/// What [`crate::client::KiCadIpcClient::get_board_bounds_in`] measured on a
/// live board.
#[derive(Debug, Clone)]
pub struct IpcBoardBounds {
    /// The union of every item's box; `None` only for a board with no items.
    pub extents: Option<IpcBoardExtents>,
    /// How many items of each class KiCad listed and measured, by the name a
    /// response reports the class under. Classes with no items are absent.
    pub measured: std::collections::BTreeMap<String, usize>,
    /// Items KiCad listed under a KIID it had already listed. Each KIID is
    /// measured once, so these have no box of their own.
    pub shared_kiid_count: usize,
    /// Board item classes whose items are not in `extents`, because KiCad
    /// would not list them or listed them in a form this protocol cannot read.
    pub unavailable: Vec<IpcUnavailableItemClass>,
}

/// A class of board items a live bounds read could not measure, and why. A
/// class is named here rather than left out, so an answer never reads as
/// complete when it is not (#688).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcUnavailableItemClass {
    /// The class, by the name a response reports it under (`tables`).
    pub class: String,
    pub reason: UnavailableReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnavailableReason {
    /// KiCad declined to list the class. KiCad 10.0.5 answers `GetItems` for
    /// tables and generators with `AS_BAD_REQUEST`.
    Refused { status: String, message: String },
    /// KiCad listed items of a class the bundled protocol has no message for,
    /// so their KIIDs cannot be read to measure them.
    Undecodable { listed_count: usize },
}

/// Footprint-local placement of the Reference and Value text fields, read
/// from the library footprint so placed parts keep the library's text
/// positions. A hardcoded offset put the Reference on top of the part's own
/// silkscreen (silk_overlap DRC warnings in live verification).
///
/// `reference_style`/`value_style` carry how the library draws each field.
/// Without them a placed part's fields were built from protobuf defaults, which
/// KiCad saved as `(unlocked yes)`, no font thickness and `(line_spacing 0)`,
/// with the Value hidden on the silkscreen instead of shown on `F.Fab` (#201).
#[derive(Debug, Clone, Default)]
pub struct IpcFieldPlacement {
    /// (x, y, rotation) of the Reference text, footprint-local mm/degrees.
    pub reference_at: Option<(f64, f64, f64)>,
    /// (x, y, rotation) of the Value text, footprint-local mm/degrees.
    pub value_at: Option<(f64, f64, f64)>,
    /// The library's presentation of the Reference text. `None` when the
    /// footprint did not come from a library (inline geometry).
    pub reference_style: Option<IpcFieldStyle>,
    /// The library's presentation of the Value text.
    pub value_style: Option<IpcFieldStyle>,
}

/// How a library footprint draws one of its fields: everything a
/// `(property …)` states besides its text and `(at …)`.
///
/// Each member holds what KiCad reads from the file, including KiCad's own
/// value for a clause the file leaves out, so the placed field saves back as
/// the library wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct IpcFieldStyle {
    /// KiCad layer name from `(layer …)`.
    pub layer: String,
    /// False for `(hide yes)`.
    pub visible: bool,
    /// Glyph (width, height) in mm from `(font (size h w))`.
    pub size: (f64, f64),
    /// `(font (thickness …))` in mm. `None` when the library names none, which
    /// KiCad keeps as a zero thickness and draws at a width derived from the
    /// glyph size.
    pub thickness_mm: Option<f64>,
    /// `(font (line_spacing …))`; KiCad's value when absent is 1.
    pub line_spacing: f64,
    /// `(font (face …))`; empty for KiCad's built-in stroke font.
    pub font_face: String,
    pub bold: bool,
    pub italic: bool,
    /// `(justify left|right)`; centered when absent.
    pub horizontal_alignment: crate::gen::kiapi::common::types::HorizontalAlignment,
    /// `(justify top|bottom)`; centered when absent.
    pub vertical_alignment: crate::gen::kiapi::common::types::VerticalAlignment,
    /// `(justify mirror)`.
    pub mirrored: bool,
    /// KiCad's "keep upright". The file states only the opposite, as
    /// `(unlocked yes)`, so this is true unless that clause is present.
    pub keep_upright: bool,
}

impl IpcFieldStyle {
    /// The style a field gets when no library describes it: KiCad's own
    /// defaults for footprint text (1 mm glyphs, 0.15 mm thickness, single
    /// line spacing, centered, kept upright) on `layer`.
    pub fn kicad_default(layer: &str, visible: bool) -> Self {
        Self {
            layer: layer.to_string(),
            visible,
            size: (1.0, 1.0),
            thickness_mm: Some(0.15),
            line_spacing: 1.0,
            font_face: String::new(),
            bold: false,
            italic: false,
            horizontal_alignment: crate::gen::kiapi::common::types::HorizontalAlignment::HaCenter,
            vertical_alignment: crate::gen::kiapi::common::types::VerticalAlignment::VaCenter,
            mirrored: false,
            keep_upright: true,
        }
    }
}

/// A three-component value as KiCad's `Vector3D` message carries it.
///
/// The enclosing field names the quantity and unit. KiCad reuses this wire
/// shape for unitless scale, rotation in degrees, and offset in millimetres,
/// so a bare vector must never be exposed without that context.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IpcVector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// A footprint's associated 3D model, as read back from a placed instance.
/// `flip_footprint` reports these so a caller can independently confirm the
/// model transform native `FlipItems` applied, without Konnect reimplementing
/// KiCad's own 3D-model flip math.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcFootprint3DModel {
    pub filename: String,
    /// Footprint-local model offset in millimetres.
    pub offset_mm: IpcVector3,
    /// Model rotation around each axis in degrees.
    pub rotation_degrees: IpcVector3,
    /// Unitless model scale along each axis.
    pub scale: IpcVector3,
    pub visible: bool,
    pub opacity: f64,
}

/// One exact live footprint and the ordered 3D-model entries KiCad returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcFootprint3DModelSnapshot {
    pub reference: String,
    pub kiid: String,
    pub models: Vec<IpcFootprint3DModel>,
}

/// A bounded edit to the ordered 3D-model entries on one placed footprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpcFootprint3DModelEdit {
    Append {
        model: IpcFootprint3DModel,
    },
    Replace {
        index: usize,
        model: IpcFootprint3DModel,
    },
    Remove {
        index: usize,
    },
}

/// Readback-derived result of editing a placed footprint's 3D-model entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpcFootprint3DModelEditOutcome {
    Conflict {
        expected: IpcFootprint3DModelSnapshot,
        observed: IpcFootprint3DModelSnapshot,
    },
    Applied {
        before: IpcFootprint3DModelSnapshot,
        after: IpcFootprint3DModelSnapshot,
        changed: bool,
    },
    Uncertain {
        reference: String,
        kiid: String,
        expected_models: Vec<IpcFootprint3DModel>,
        observed_models: Option<Vec<IpcFootprint3DModel>>,
        reason: String,
    },
}

/// The result of a `flip_footprint` call against a live KiCad IPC session.
///
/// Three outcomes, not two (accepted contract for issue #604): a definite
/// success (`Flipped`), authoritative evidence that this KiCad build has no
/// native `FlipItems` handler (`Unsupported`), and a distinct "reconcile
/// before retrying" state for when the mutation looked like it went through
/// but a fresh readback could not confirm it (`Uncertain`). A hard failure
/// (footprint not found, wrong KIID/type, a non-`ISC_OK` per-item status, or
/// any other rejected request) is not represented here — it is returned as
/// `Err` and classified by the caller's normal IPC-failure handling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpcFlipOutcome {
    /// KiCad flipped the footprint (or it already sat on the requested
    /// layer, in which case no `FlipItems` call was made at all) and a fresh
    /// readback — never the mutation response — independently confirmed the
    /// requested layer and the footprint's identity.
    Flipped {
        reference: String,
        kiid: String,
        previous_layer: String,
        layer: String,
        already_on_layer: bool,
        models: Vec<IpcFootprint3DModel>,
    },
    /// KiCad answered `AS_UNHANDLED`/`AS_UNIMPLEMENTED` for `FlipItems`,
    /// which the 10.0.6 protocol treats as authoritative capability
    /// evidence. Never inferred from a version string.
    Unsupported {
        kicad_version: Option<IpcKiCadVersion>,
    },
    /// The mutation appeared to succeed, but a fresh post-flip readback
    /// disagreed with it or could not be resolved. The board may or may not
    /// have changed; the caller must reconcile before retrying rather than
    /// reporting either success or failure.
    Uncertain {
        reference: String,
        kiid: String,
        requested_layer: String,
        reason: String,
    },
}

// ─── Placed text: footprint fields and board-level text ──────────────────────

/// One text item's properties exactly as KiCad holds them.
///
/// Lengths stay in integer nanometres so a readback compares what KiCad
/// stored rather than a rounded millimetre value. `x_nm`/`y_nm` are
/// board-absolute even for a footprint field: KiCad serializes every child of
/// a footprint in board coordinates (see the `transform` module), and
/// `angle_deg` is the text's own absolute angle, which for a field already
/// includes the footprint's orientation.
///
/// `text` is the raw string KiCad sent. KiCad 10 serializes `EDA_TEXT` with
/// `GetText()`, not `GetShownText()`, so a `${VARIABLE}` arrives unexpanded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcTextState {
    pub text: String,
    pub x_nm: i64,
    pub y_nm: i64,
    pub angle_deg: f64,
    pub size_x_nm: i64,
    pub size_y_nm: i64,
    pub stroke_width_nm: i64,
    pub layer: String,
    /// Empty for KiCad's built-in stroke font.
    pub font_name: String,
    pub horizontal_alignment: String,
    pub vertical_alignment: String,
    pub italic: bool,
    pub bold: bool,
    pub underlined: bool,
    pub mirrored: bool,
    pub multiline: bool,
    pub keep_upright: bool,
    pub line_spacing: f64,
    pub knockout: bool,
    pub locked: bool,
}

/// An axis-aligned box in board nanometres, as KiCad's `GetBoundingBox`
/// measured it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcBox2 {
    pub min_x_nm: i64,
    pub min_y_nm: i64,
    pub max_x_nm: i64,
    pub max_y_nm: i64,
}

/// Which of a footprint's four mandatory fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcFootprintFieldSlot {
    Reference,
    Value,
    Datasheet,
    Description,
}

impl IpcFootprintFieldSlot {
    pub const ALL: [Self; 4] = [
        Self::Reference,
        Self::Value,
        Self::Datasheet,
        Self::Description,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Value => "value",
            Self::Datasheet => "datasheet",
            Self::Description => "description",
        }
    }
}

/// One field of a placed footprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcFootprintField {
    pub footprint_reference: String,
    pub footprint_kiid: String,
    /// `reference`, `value`, `datasheet`, `description`, or `user` for a
    /// field that is not one of the four mandatory ones.
    pub slot: String,
    /// The field name KiCad reported.
    pub name: String,
    /// The text item's own KIID, empty if KiCad sent none.
    pub kiid: String,
    pub visible: bool,
    /// `None` when KiCad's message lacked a part needed to describe the text;
    /// `unreadable_reason` then says which. Never a fabricated default.
    pub state: Option<IpcTextState>,
    pub unreadable_reason: Option<String>,
    /// `None` when KiCad returned no box for this KIID.
    pub bounding_box: Option<IpcBox2>,
}

/// Every field of the footprints a listing covered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcFootprintFieldListing {
    pub fields: Vec<IpcFootprintField>,
    /// Set when the bounding-box request as a whole failed; individual boxes
    /// are then all `None` for that reason rather than because KiCad skipped
    /// their KIIDs.
    pub bounding_boxes_unavailable: Option<String>,
}

/// One board-level text item (`PCB_TEXT` owned by the board, not a footprint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcBoardTextItem {
    pub kiid: String,
    pub state: Option<IpcTextState>,
    pub unreadable_reason: Option<String>,
    pub bounding_box: Option<IpcBox2>,
}

/// Every board-level text item on a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcBoardTextListing {
    pub texts: Vec<IpcBoardTextItem>,
    pub bounding_boxes_unavailable: Option<String>,
}

/// A partial edit to one text item. `None` leaves that property exactly as
/// KiCad holds it; nothing is defaulted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IpcTextEdit {
    /// Board-absolute position in nanometres.
    pub position_nm: Option<(i64, i64)>,
    pub angle_deg: Option<f64>,
    pub size_x_nm: Option<i64>,
    pub size_y_nm: Option<i64>,
    pub stroke_width_nm: Option<i64>,
    pub layer: Option<String>,
    /// Raw replacement text, written verbatim. Board text only.
    pub text: Option<String>,
    /// Field visibility. Footprint fields only.
    pub visible: Option<bool>,
    /// Draw the glyphs mirrored, as text on a back layer must be to read
    /// correctly from that side.
    pub mirrored: Option<bool>,
    /// Set `mirrored` from the side of the layer the text ends up on: true on
    /// a back layer, false on a front layer, untouched on a layer with no
    /// side. That layer is the one this edit names, or the one KiCad holds
    /// when it names none. Cannot be combined with `mirrored`.
    pub mirror_to_match_layer: bool,
    pub horizontal_alignment: Option<IpcHorizontalAlignment>,
    pub vertical_alignment: Option<IpcVerticalAlignment>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub keep_upright: Option<bool>,
    pub knockout: Option<bool>,
    pub locked: Option<bool>,
    /// Font face name; the empty string selects KiCad's stroke font.
    pub font_name: Option<String>,
}

/// The mirror flag text on the layer named `layer` needs to read correctly
/// from its own side of the board: set on a back layer (`B.SilkS`), clear on
/// a front one (`F.Fab`). `None` for a layer with no side (inner copper,
/// `Edge.Cuts`, the user layers), where neither value is the right one.
///
/// This is the end state KiCad's Flip leaves a normally-drawn text in. Flip
/// itself toggles the flag; setting it from the side instead also repairs a
/// text that was moved to the back without being mirrored, which a toggle
/// from the same layer could not.
pub fn mirrored_for_layer_name(layer: &str) -> Option<bool> {
    if layer.starts_with("B.") {
        Some(true)
    } else if layer.starts_with("F.") {
        Some(false)
    } else {
        None
    }
}

/// A horizontal text alignment a caller may request. KiCad's
/// `HA_INDETERMINATE` describes a mixed selection in its dialogs and is not a
/// value one text can be given, so it has no variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcHorizontalAlignment {
    Left,
    Center,
    Right,
}

impl IpcHorizontalAlignment {
    /// The name `IpcTextState::horizontal_alignment` reports for this value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Left, Self::Center, Self::Right]
            .into_iter()
            .find(|alignment| alignment.as_str() == name)
    }
}

/// A vertical text alignment a caller may request; see
/// [`IpcHorizontalAlignment`] for why there is no indeterminate variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcVerticalAlignment {
    Top,
    Center,
    Bottom,
}

impl IpcVerticalAlignment {
    /// The name `IpcTextState::vertical_alignment` reports for this value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Center => "center",
            Self::Bottom => "bottom",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Top, Self::Center, Self::Bottom]
            .into_iter()
            .find(|alignment| alignment.as_str() == name)
    }
}

impl IpcTextEdit {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// One field a batch edit covered, before the edit and as a fresh read holds
/// it afterwards. The two are equal for a field that already held every
/// requested value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcFootprintFieldChange {
    pub before: IpcFootprintField,
    pub after: IpcFootprintField,
}

/// A field a whole-board batch edit left alone because it could not be
/// edited and proven; never a field the caller named explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcFootprintFieldSkip {
    pub footprint_reference: String,
    pub footprint_kiid: String,
    pub slot: String,
    pub reason: String,
}

/// Readback-derived result of applying one edit to fields of many footprints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpcFootprintFieldBatchOutcome {
    /// Every covered field holds the requested values, proven by a fresh
    /// read. `sent` is false when they all did already and nothing was sent.
    Settled {
        fields: Vec<IpcFootprintFieldChange>,
        skipped: Vec<IpcFootprintFieldSkip>,
        sent: bool,
    },
    /// The update may have changed the board, but a fresh read did not prove
    /// the requested state for every footprint. Inspect before retrying.
    Uncertain {
        mismatches: Vec<String>,
        reason: String,
    },
}

/// Readback-derived result of editing one text item in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpcTextEditOutcome<T> {
    /// The item already held every requested value; nothing was sent.
    Unchanged { current: T },
    /// KiCad applied the edit and a fresh read proved every requested value
    /// and every preserved one.
    Applied { before: T, after: T },
    /// The update may have changed the board, but a fresh read did not prove
    /// the requested state. Inspect before retrying.
    Uncertain {
        before: T,
        expected: T,
        observed: Option<T>,
        mismatches: Vec<String>,
        reason: String,
    },
}
