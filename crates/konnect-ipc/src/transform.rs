//! Rigid-body transforms for `FootprintInstance` round-trips (issue #23).
//!
//! KiCAD 10 serializes every child of a footprint (pads, fields, texts,
//! graphics, zones) in ABSOLUTE board coordinates — despite the proto comment
//! on `Pad.position` claiming footprint-relative coordinates — and
//! `FOOTPRINT::Deserialize` clears all children and re-creates them at exactly
//! the coordinates carried in the message. Round-tripping an instance with
//! only `position` or `orientation` changed therefore leaves the copper and
//! silkscreen at the old location, detached from the anchor. Before sending
//! `UpdateItems`, every child must get the same rigid-body transform KiCAD
//! would apply natively (`FOOTPRINT::SetPosition` / `SetOrientation`).

use crate::gen::kiapi;
use anyhow::Result;
use kiapi::board::types as board;
use kiapi::common::types as common;
use prost::Message;

/// The transform to apply to a footprint's children.
#[derive(Clone, Copy)]
pub enum Xform {
    /// Shift every child by (dx, dy) nanometers.
    Translate { dx_nm: i64, dy_nm: i64 },
    /// Rotate every child by `delta_deg` around (cx, cy), matching KiCAD's
    /// convention (Y axis down; positive angles counterclockwise on screen).
    Rotate {
        cx_nm: i64,
        cy_nm: i64,
        delta_deg: f64,
    },
}

impl Xform {
    pub(crate) fn point(&self, x: i64, y: i64) -> (i64, i64) {
        match *self {
            Xform::Translate { dx_nm, dy_nm } => (x + dx_nm, y + dy_nm),
            Xform::Rotate {
                cx_nm,
                cy_nm,
                delta_deg,
            } => {
                // KiCAD's RotatePoint (libs/kimath/src/trigo.cpp):
                //   x' = x·cosθ + y·sinθ ;  y' = y·cosθ − x·sinθ
                let (rx, ry) = ((x - cx_nm) as f64, (y - cy_nm) as f64);
                let (s, c) = delta_deg.to_radians().sin_cos();
                (
                    cx_nm + (rx * c + ry * s).round() as i64,
                    cy_nm + (ry * c - rx * s).round() as i64,
                )
            }
        }
    }

    fn angle_delta_deg(&self) -> f64 {
        match *self {
            Xform::Translate { .. } => 0.0,
            Xform::Rotate { delta_deg, .. } => delta_deg,
        }
    }

    /// True when the rotation keeps axis-aligned shapes axis-aligned.
    fn is_cardinal(&self) -> bool {
        let r = self.angle_delta_deg().rem_euclid(90.0);
        r.abs() < 1e-9 || (90.0 - r).abs() < 1e-9
    }
}

/// Normalize to (-180, 180], the range KiCAD's `EDA_ANGLE::Normalize180` uses.
fn normalize_deg(deg: f64) -> f64 {
    let mut d = deg % 360.0;
    if d <= -180.0 {
        d += 360.0;
    } else if d > 180.0 {
        d -= 360.0;
    }
    d
}

fn xform_vec2(v: &mut Option<common::Vector2>, xf: &Xform) {
    if let Some(p) = v.as_mut() {
        let (x, y) = xf.point(p.x_nm, p.y_nm);
        p.x_nm = x;
        p.y_nm = y;
    }
}

fn xform_angle(a: &mut Option<common::Angle>, xf: &Xform) {
    let delta = xf.angle_delta_deg();
    if delta != 0.0 {
        let base = a.as_ref().map(|x| x.value_degrees).unwrap_or(0.0);
        *a = Some(common::Angle {
            value_degrees: normalize_deg(base + delta),
        });
    }
}

fn xform_text(t: &mut Option<common::Text>, xf: &Xform) {
    if let Some(text) = t.as_mut() {
        xform_vec2(&mut text.position, xf);
        if let Some(attrs) = text.attributes.as_mut() {
            xform_angle(&mut attrs.angle, xf);
        }
    }
}

fn xform_board_text(bt: &mut board::BoardText, xf: &Xform) {
    xform_text(&mut bt.text, xf);
}

fn xform_field(f: &mut Option<board::Field>, xf: &Xform) {
    if let Some(field) = f.as_mut() {
        if let Some(text) = field.text.as_mut() {
            xform_board_text(text, xf);
        }
    }
}

fn xform_polyline(pl: &mut common::PolyLine, xf: &Xform) {
    for node in pl.nodes.iter_mut() {
        match node.geometry.as_mut() {
            Some(common::poly_line_node::Geometry::Point(p)) => {
                let (x, y) = xf.point(p.x_nm, p.y_nm);
                p.x_nm = x;
                p.y_nm = y;
            }
            Some(common::poly_line_node::Geometry::Arc(arc)) => {
                xform_vec2(&mut arc.start, xf);
                xform_vec2(&mut arc.mid, xf);
                xform_vec2(&mut arc.end, xf);
            }
            None => {}
        }
    }
}

fn xform_polyset(ps: &mut common::PolySet, xf: &Xform) {
    for poly in ps.polygons.iter_mut() {
        if let Some(outline) = poly.outline.as_mut() {
            xform_polyline(outline, xf);
        }
        for hole in poly.holes.iter_mut() {
            xform_polyline(hole, xf);
        }
    }
}

fn xform_graphic_shape(gs: &mut board::BoardGraphicShape, xf: &Xform) -> Result<()> {
    use common::graphic_shape::Geometry;

    let Some(shape) = gs.shape.as_mut() else {
        return Ok(());
    };
    match shape.geometry.as_mut() {
        Some(Geometry::Segment(seg)) => {
            xform_vec2(&mut seg.start, xf);
            xform_vec2(&mut seg.end, xf);
        }
        Some(Geometry::Rectangle(rect)) => {
            if xf.is_cardinal() {
                xform_vec2(&mut rect.top_left, xf);
                xform_vec2(&mut rect.bottom_right, xf);
            } else if rect.corner_radius.is_some() {
                // A rounded rectangle can't be represented off-axis; KiCAD
                // itself converts plain rectangles to polygons here, but has
                // no lossless form for rounded ones.
                anyhow::bail!(
                    "cannot rotate a footprint containing a rounded-rectangle \
                     graphic by a non-90° angle; use a multiple of 90°"
                );
            } else {
                // Mirror KiCAD's EDA_SHAPE::Rotate: an off-axis rectangle
                // becomes a closed polygon of its four rotated corners.
                let (tl, br) = (
                    rect.top_left.unwrap_or_default(),
                    rect.bottom_right.unwrap_or_default(),
                );
                let corners = [
                    (tl.x_nm, tl.y_nm),
                    (br.x_nm, tl.y_nm),
                    (br.x_nm, br.y_nm),
                    (tl.x_nm, br.y_nm),
                ];
                let nodes = corners
                    .iter()
                    .map(|&(x, y)| {
                        let (nx, ny) = xf.point(x, y);
                        common::PolyLineNode {
                            geometry: Some(common::poly_line_node::Geometry::Point(
                                common::Vector2 { x_nm: nx, y_nm: ny },
                            )),
                        }
                    })
                    .collect();
                shape.geometry = Some(Geometry::Polygon(common::PolySet {
                    polygons: vec![common::PolygonWithHoles {
                        outline: Some(common::PolyLine {
                            nodes,
                            closed: true,
                        }),
                        holes: vec![],
                    }],
                }));
            }
        }
        Some(Geometry::Arc(arc)) => {
            xform_vec2(&mut arc.start, xf);
            xform_vec2(&mut arc.mid, xf);
            xform_vec2(&mut arc.end, xf);
        }
        Some(Geometry::Circle(circle)) => {
            xform_vec2(&mut circle.center, xf);
            xform_vec2(&mut circle.radius_point, xf);
        }
        Some(Geometry::Polygon(polyset)) => {
            xform_polyset(polyset, xf);
        }
        Some(Geometry::Bezier(bez)) => {
            xform_vec2(&mut bez.start, xf);
            xform_vec2(&mut bez.control1, xf);
            xform_vec2(&mut bez.control2, xf);
            xform_vec2(&mut bez.end, xf);
        }
        None => {}
    }
    Ok(())
}

fn xform_zone(zone: &mut board::Zone, xf: &Xform) {
    if let Some(outline) = zone.outline.as_mut() {
        xform_polyset(outline, xf);
    }
    for filled in zone.filled_polygons.iter_mut() {
        if let Some(shapes) = filled.shapes.as_mut() {
            xform_polyset(shapes, xf);
        }
    }
}

fn xform_dimension(dim: &mut board::Dimension, xf: &Xform) {
    use board::dimension::DimensionStyle;

    xform_text(&mut dim.text, xf);
    match dim.dimension_style.as_mut() {
        Some(DimensionStyle::Aligned(a)) => {
            xform_vec2(&mut a.start, xf);
            xform_vec2(&mut a.end, xf);
        }
        Some(DimensionStyle::Orthogonal(o)) => {
            xform_vec2(&mut o.start, xf);
            xform_vec2(&mut o.end, xf);
        }
        Some(DimensionStyle::Radial(r)) => {
            xform_vec2(&mut r.center, xf);
            xform_vec2(&mut r.radius_point, xf);
        }
        Some(DimensionStyle::Leader(l)) => {
            xform_vec2(&mut l.start, xf);
            xform_vec2(&mut l.end, xf);
        }
        Some(DimensionStyle::Center(c)) => {
            xform_vec2(&mut c.center, xf);
            xform_vec2(&mut c.end, xf);
        }
        None => {}
    }
}

/// A footprint's `(point …)` as KiCad 10.0.8 sends it. 617 of the stock
/// footprints carry one, every `Package_DIP` among them.
pub const REFERENCE_POINT: &str = "kiapi.board.types.ReferencePoint";

/// Move a `ReferencePoint`'s position, leaving every other field as sent.
///
/// The vendored protos predate the message, so its layout is read from what
/// KiCad 10.0.8 sent for the stock `DIP-8_W7.62mm` point: 1 = id (KIID),
/// 2 = layer, 3 = position (`Vector2`, absolute like every other child),
/// 4 = size (`Distance`), 5 = locked, 6 = a second KIID. Only field 3 is
/// decoded; the rest is copied byte for byte, so a field this reading has
/// wrong, or one a later KiCad adds, is still returned to KiCad unchanged.
fn xform_reference_point(value: &[u8], xf: &Xform) -> Result<Vec<u8>> {
    use prost::encoding::{decode_key, decode_varint, encode_key, encode_varint, WireType};
    const POSITION: u32 = 3;

    let mut rest = value;
    let mut out = Vec::with_capacity(value.len() + 8);
    let mut moved = false;
    while !rest.is_empty() {
        let before = rest;
        let (tag, wire_type) = decode_key(&mut rest)?;
        let length = match wire_type {
            WireType::Varint => {
                decode_varint(&mut rest)?;
                0
            }
            WireType::SixtyFourBit => 8,
            WireType::ThirtyTwoBit => 4,
            WireType::LengthDelimited => decode_varint(&mut rest)? as usize,
            other => anyhow::bail!("reference point has a field of wire type {other:?}"),
        };
        anyhow::ensure!(rest.len() >= length, "reference point is truncated");
        let (payload, after) = rest.split_at(length);
        rest = after;
        if tag == POSITION && wire_type == WireType::LengthDelimited {
            let mut position = Some(common::Vector2::decode(payload)?);
            xform_vec2(&mut position, xf);
            let position = position.unwrap_or_default().encode_to_vec();
            encode_key(POSITION, WireType::LengthDelimited, &mut out);
            encode_varint(position.len() as u64, &mut out);
            out.extend_from_slice(&position);
            moved = true;
        } else {
            out.extend_from_slice(&before[..before.len() - rest.len()]);
        }
    }
    // proto3 leaves out a zero Vector2, and an absent position is one this
    // cannot move: the point would stay at the board origin.
    anyhow::ensure!(
        moved,
        "footprint contains a reference point with no position; refusing to \
         move/rotate it via IPC to avoid detaching it"
    );
    Ok(out)
}

/// Apply `xf` to every position-bearing child carried in `fp`: the four
/// mandatory fields plus all `definition.items`. Does NOT touch
/// `fp.position` / `fp.orientation` — the caller sets those.
pub fn transform_footprint_children(fp: &mut board::FootprintInstance, xf: &Xform) -> Result<()> {
    xform_field(&mut fp.reference_field, xf);
    xform_field(&mut fp.value_field, xf);
    xform_field(&mut fp.datasheet_field, xf);
    xform_field(&mut fp.description_field, xf);

    let Some(def) = fp.definition.as_mut() else {
        return Ok(());
    };

    for item in def.items.iter_mut() {
        // Discriminate on the declared type, never on whether a decode
        // happens to succeed — see `builders::any_is` for why that distinction
        // cost every synced footprint its graphics (#244).
        let url = crate::builders::any_type_name(item);
        if url == "kiapi.board.types.Pad" {
            let mut pad = board::Pad::decode(item.value.as_slice())?;
            xform_vec2(&mut pad.position, xf);
            if let Some(ps) = pad.pad_stack.as_mut() {
                xform_angle(&mut ps.angle, xf);
            }
            item.value = pad.encode_to_vec();
        } else if url == "kiapi.board.types.BoardText" {
            let mut text = board::BoardText::decode(item.value.as_slice())?;
            xform_board_text(&mut text, xf);
            item.value = text.encode_to_vec();
        } else if url == "kiapi.board.types.BoardTextBox" {
            if !xf.is_cardinal() {
                anyhow::bail!(
                    "cannot rotate a footprint containing a textbox by a \
                     non-90° angle; use a multiple of 90°"
                );
            }
            let mut tb = board::BoardTextBox::decode(item.value.as_slice())?;
            if let Some(textbox) = tb.textbox.as_mut() {
                xform_vec2(&mut textbox.top_left, xf);
                xform_vec2(&mut textbox.bottom_right, xf);
                if let Some(attrs) = textbox.attributes.as_mut() {
                    xform_angle(&mut attrs.angle, xf);
                }
            }
            item.value = tb.encode_to_vec();
        } else if url == "kiapi.board.types.Field" {
            let mut field = board::Field::decode(item.value.as_slice())?;
            if let Some(text) = field.text.as_mut() {
                xform_board_text(text, xf);
            }
            item.value = field.encode_to_vec();
        } else if url == "kiapi.board.types.BoardGraphicShape" {
            let mut shape = board::BoardGraphicShape::decode(item.value.as_slice())?;
            xform_graphic_shape(&mut shape, xf)?;
            item.value = shape.encode_to_vec();
        } else if url == "kiapi.board.types.Zone" {
            let mut zone = board::Zone::decode(item.value.as_slice())?;
            xform_zone(&mut zone, xf);
            item.value = zone.encode_to_vec();
        } else if url == "kiapi.board.types.Dimension" {
            let mut dim = board::Dimension::decode(item.value.as_slice())?;
            xform_dimension(&mut dim, xf);
            item.value = dim.encode_to_vec();
        } else if url == "kiapi.board.types.Footprint3DModel" || url == "kiapi.board.types.Group" {
            // No board-space geometry of their own — nothing to shift.
        } else if url == REFERENCE_POINT {
            item.value = xform_reference_point(&item.value, xf)?;
        } else {
            // KiCAD would re-create this item wherever the stale message
            // says it is; refusing beats silently corrupting the board.
            anyhow::bail!(
                "footprint contains an item of unsupported type '{url}'; \
                 refusing to move/rotate it via IPC to avoid detaching it"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders;

    fn pad_at(x_mm: f64, y_mm: f64, angle_deg: f64) -> board::Pad {
        board::Pad {
            position: Some(builders::vec2(x_mm, y_mm)),
            pad_stack: Some(board::PadStack {
                angle: Some(common::Angle {
                    value_degrees: angle_deg,
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn field_at(x_mm: f64, y_mm: f64) -> board::Field {
        board::Field {
            text: Some(board::BoardText {
                text: Some(common::Text {
                    position: Some(builders::vec2(x_mm, y_mm)),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn instance_with(items: Vec<prost_types::Any>) -> board::FootprintInstance {
        board::FootprintInstance {
            position: Some(builders::vec2(100.0, 100.0)),
            reference_field: Some(field_at(100.0, 98.0)),
            definition: Some(board::Footprint {
                items,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn decode_pad(any: &prost_types::Any) -> board::Pad {
        board::Pad::decode(any.value.as_slice()).unwrap()
    }

    /// The `ReferencePoint` KiCad 10.0.8 sent for U1 (stock `DIP-8_W7.62mm`,
    /// placed at (68.38, 63.88)) on `footprint_children_kicad10.kicad_pcb`:
    /// position (72.19, 67.69), size 1.27 mm.
    fn captured_reference_point() -> Vec<u8> {
        let hex = "0a260a2435383865343439322d643735622d346364372d613561652d6364363330\
                   6239356265623110341a0a08b090b6221090bca320220408f0c14d280132260a24\
                   63373930313234352d643063342d343736662d383231352d383566313138336430\
                   346465";
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    /// The position a reference point carries, read the way the transform
    /// reads it.
    fn reference_point_position(value: &[u8]) -> (i64, i64) {
        use prost::encoding::{decode_key, decode_varint, WireType};
        let mut rest = value;
        while !rest.is_empty() {
            let (tag, wire_type) = decode_key(&mut rest).unwrap();
            if wire_type == WireType::Varint {
                decode_varint(&mut rest).unwrap();
                continue;
            }
            let length = decode_varint(&mut rest).unwrap() as usize;
            let (payload, after) = rest.split_at(length);
            rest = after;
            if tag == 3 {
                let position = common::Vector2::decode(payload).unwrap();
                return (position.x_nm, position.y_nm);
            }
        }
        panic!("no position");
    }

    /// Moving or rotating a footprint used to be refused when it carried a
    /// `(point …)`, which every stock DIP does. The point moves with the
    /// footprint, and every other byte KiCad sent goes back as it came.
    #[test]
    fn a_reference_point_moves_with_its_footprint_and_keeps_its_other_fields() {
        let captured = captured_reference_point();
        assert_eq!(
            reference_point_position(&captured),
            (72_190_000, 67_690_000)
        );
        let point = || prost_types::Any {
            type_url: format!("type.googleapis.com/{REFERENCE_POINT}"),
            value: captured.clone(),
        };

        let mut moved = instance_with(vec![point()]);
        transform_footprint_children(
            &mut moved,
            &Xform::Translate {
                dx_nm: 10_000_000,
                dy_nm: -5_000_000,
            },
        )
        .unwrap();
        let value = &moved.definition.as_ref().unwrap().items[0].value;
        assert_eq!(reference_point_position(value), (82_190_000, 62_690_000));
        // Same length, and identical outside the ten bytes of the position:
        // the id (40 bytes) and layer (2) come first, then field 3's key and
        // length, so the position is bytes 44..54.
        assert_eq!(value.len(), captured.len());
        assert_eq!(value[..44], captured[..44]);
        assert_eq!(value[54..], captured[54..]);

        // Unchanged by a move of nothing: the copy is exact.
        let mut still = instance_with(vec![point()]);
        transform_footprint_children(&mut still, &Xform::Translate { dx_nm: 0, dy_nm: 0 }).unwrap();
        assert_eq!(still.definition.unwrap().items[0].value, captured);

        // A quarter turn about the footprint's anchor (68.38, 63.88): the
        // point at (+3.81, +3.81) lands at (+3.81, -3.81).
        let mut turned = instance_with(vec![point()]);
        transform_footprint_children(
            &mut turned,
            &Xform::Rotate {
                cx_nm: 68_380_000,
                cy_nm: 63_880_000,
                delta_deg: 90.0,
            },
        )
        .unwrap();
        assert_eq!(
            reference_point_position(&turned.definition.unwrap().items[0].value),
            (72_190_000, 60_070_000)
        );
    }

    #[test]
    fn a_reference_point_with_no_position_is_refused_not_left_behind() {
        let mut footprint = instance_with(vec![prost_types::Any {
            type_url: format!("type.googleapis.com/{REFERENCE_POINT}"),
            value: vec![0x10, 0x34],
        }]);
        let error =
            transform_footprint_children(&mut footprint, &Xform::Translate { dx_nm: 1, dy_nm: 1 })
                .unwrap_err();
        assert!(error.to_string().contains("no position"), "{error}");
    }

    #[test]
    fn translate_shifts_pads_fields_and_graphics() {
        let pad = builders::pack_any(&pad_at(100.0, 100.0, 0.0), "kiapi.board.types.Pad");
        let seg = builders::pack_any(
            &builders::board_segment("F.SilkS", 0.1, 99.0, 99.0, 101.0, 99.0),
            "kiapi.board.types.BoardGraphicShape",
        );
        let mut fp = instance_with(vec![pad, seg]);

        // Move 100,100 -> 50,50: delta is -50mm on both axes.
        let xf = Xform::Translate {
            dx_nm: builders::mm_to_nm(-50.0),
            dy_nm: builders::mm_to_nm(-50.0),
        };
        transform_footprint_children(&mut fp, &xf).unwrap();

        let items = &fp.definition.as_ref().unwrap().items;
        let pad = decode_pad(&items[0]);
        assert_eq!(pad.position.unwrap(), builders::vec2(50.0, 50.0));

        let shape = board::BoardGraphicShape::decode(items[1].value.as_slice()).unwrap();
        match shape.shape.unwrap().geometry.unwrap() {
            common::graphic_shape::Geometry::Segment(s) => {
                assert_eq!(s.start.unwrap(), builders::vec2(49.0, 49.0));
                assert_eq!(s.end.unwrap(), builders::vec2(51.0, 49.0));
            }
            other => panic!("expected segment, got {other:?}"),
        }

        let ref_text = fp.reference_field.unwrap().text.unwrap().text.unwrap();
        assert_eq!(ref_text.position.unwrap(), builders::vec2(50.0, 48.0));
    }

    #[test]
    fn rotate_90_matches_kicad_convention() {
        // Pad 1mm to the right of the anchor, at pad angle 0.
        let pad = builders::pack_any(&pad_at(101.0, 100.0, 0.0), "kiapi.board.types.Pad");
        let mut fp = instance_with(vec![pad]);

        let xf = Xform::Rotate {
            cx_nm: builders::mm_to_nm(100.0),
            cy_nm: builders::mm_to_nm(100.0),
            delta_deg: 90.0,
        };
        transform_footprint_children(&mut fp, &xf).unwrap();

        // KiCAD positive rotation is counterclockwise on screen (Y down):
        // (+1, 0) relative -> (0, -1) relative.
        let pad = decode_pad(&fp.definition.as_ref().unwrap().items[0]);
        assert_eq!(pad.position.unwrap(), builders::vec2(100.0, 99.0));
        assert_eq!(pad.pad_stack.unwrap().angle.unwrap().value_degrees, 90.0);
    }

    #[test]
    fn rotate_angle_normalizes_to_plus_minus_180() {
        let pad = builders::pack_any(&pad_at(100.0, 100.0, 170.0), "kiapi.board.types.Pad");
        let mut fp = instance_with(vec![pad]);

        let xf = Xform::Rotate {
            cx_nm: builders::mm_to_nm(100.0),
            cy_nm: builders::mm_to_nm(100.0),
            delta_deg: 20.0,
        };
        transform_footprint_children(&mut fp, &xf).unwrap();

        let pad = decode_pad(&fp.definition.as_ref().unwrap().items[0]);
        assert_eq!(pad.pad_stack.unwrap().angle.unwrap().value_degrees, -170.0);
    }

    #[test]
    fn non_cardinal_rotation_turns_rectangle_into_polygon() {
        let rect = builders::pack_any(
            &builders::board_rectangle("F.Fab", 0.1, 99.0, 99.0, 101.0, 101.0, false),
            "kiapi.board.types.BoardGraphicShape",
        );
        let mut fp = instance_with(vec![rect]);

        let xf = Xform::Rotate {
            cx_nm: builders::mm_to_nm(100.0),
            cy_nm: builders::mm_to_nm(100.0),
            delta_deg: 45.0,
        };
        transform_footprint_children(&mut fp, &xf).unwrap();

        let shape = board::BoardGraphicShape::decode(
            fp.definition.as_ref().unwrap().items[0].value.as_slice(),
        )
        .unwrap();
        match shape.shape.unwrap().geometry.unwrap() {
            common::graphic_shape::Geometry::Polygon(ps) => {
                let outline = ps.polygons[0].outline.as_ref().unwrap();
                assert!(outline.closed);
                assert_eq!(outline.nodes.len(), 4);
            }
            other => panic!("expected polygon after 45° rotation, got {other:?}"),
        }
    }

    #[test]
    fn cardinal_rotation_keeps_rectangle_a_rectangle() {
        let rect = builders::pack_any(
            &builders::board_rectangle("F.Fab", 0.1, 99.0, 99.0, 101.0, 101.0, false),
            "kiapi.board.types.BoardGraphicShape",
        );
        let mut fp = instance_with(vec![rect]);

        let xf = Xform::Rotate {
            cx_nm: builders::mm_to_nm(100.0),
            cy_nm: builders::mm_to_nm(100.0),
            delta_deg: 180.0,
        };
        transform_footprint_children(&mut fp, &xf).unwrap();

        let shape = board::BoardGraphicShape::decode(
            fp.definition.as_ref().unwrap().items[0].value.as_slice(),
        )
        .unwrap();
        assert!(matches!(
            shape.shape.unwrap().geometry.unwrap(),
            common::graphic_shape::Geometry::Rectangle(_)
        ));
    }

    #[test]
    fn unknown_item_type_is_rejected_not_corrupted() {
        let bogus = prost_types::Any {
            type_url: "type.googleapis.com/kiapi.board.types.SomethingNew".to_string(),
            value: vec![],
        };
        let mut fp = instance_with(vec![bogus]);

        let xf = Xform::Translate {
            dx_nm: 1000,
            dy_nm: 0,
        };
        let err = transform_footprint_children(&mut fp, &xf).unwrap_err();
        assert!(err.to_string().contains("SomethingNew"));
    }

    #[test]
    fn model_and_group_items_pass_through_untouched() {
        let model = builders::pack_any(
            &board::Footprint3DModel::default(),
            "kiapi.board.types.Footprint3DModel",
        );
        let original = model.value.clone();
        let mut fp = instance_with(vec![model]);

        let xf = Xform::Translate {
            dx_nm: builders::mm_to_nm(10.0),
            dy_nm: 0,
        };
        transform_footprint_children(&mut fp, &xf).unwrap();
        assert_eq!(fp.definition.unwrap().items[0].value, original);
    }
}
