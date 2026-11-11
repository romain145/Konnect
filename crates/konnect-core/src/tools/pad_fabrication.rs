//! A pad's fabrication property: the one `(property pad_prop_…)` clause KiCad
//! writes inside a pad for a BGA ball, a fiducial, a test point, a heatsink
//! pad, a castellated edge pad, a mechanical pad or a press-fit pin.
//!
//! It is not decoration. KiCad's DRC exempts a castellated pad from
//! copper-to-edge clearance, paste and assembly outputs treat a heatsink pad
//! and a BGA ball differently, and Gerber X2 marks each for the fab.
//!
//! The names here are the single mapping between what a tool argument or a
//! response says (`heatsink`) and what a footprint file holds
//! (`pad_prop_heatsink`). The tokens are the ones KiCad 10.0.8 writes for each
//! `PAD_PROP` value; `tests/fixtures/pad_property_kicad10.README.md` has the
//! files that show it.
//!
//! Two kinds of tool use it. `update_pcb_from_schematic` reads a library
//! footprint's pads to say which properties a board placed through KiCad 10's
//! API does not get (#193). `edit_footprint_pad` and `create_footprint` write
//! the clause into a footprint file, where nothing stands in the way (#195).

use konnect_sexp::parser::parse_sexp;
use konnect_sexp::writer::{apply_edits, find_direct_child_blocks, SexpEdit};
use konnect_sexp::SexpNode;
use serde::Serialize;

/// What a tool argument and a response say for a pad with no property.
pub(crate) const NONE: &str = "none";

/// KiCad's `PAD_PROP` values other than `NONE`, which a footprint file states
/// by having no `(property …)` clause in the pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PadFabricationProperty {
    Bga,
    FiducialGlob,
    FiducialLoc,
    Testpoint,
    Heatsink,
    Castellated,
    Mechanical,
    Pressfit,
}

impl PadFabricationProperty {
    /// In KiCad's own `PAD_PROP` order.
    pub(crate) const ALL: [Self; 8] = [
        Self::Bga,
        Self::FiducialGlob,
        Self::FiducialLoc,
        Self::Testpoint,
        Self::Heatsink,
        Self::Castellated,
        Self::Mechanical,
        Self::Pressfit,
    ];

    /// The name tools take and report.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Bga => "bga",
            Self::FiducialGlob => "fiducial_glob",
            Self::FiducialLoc => "fiducial_loc",
            Self::Testpoint => "testpoint",
            Self::Heatsink => "heatsink",
            Self::Castellated => "castellated",
            Self::Mechanical => "mechanical",
            Self::Pressfit => "pressfit",
        }
    }

    /// The atom KiCad writes in `(property …)`.
    pub(crate) fn token(self) -> &'static str {
        match self {
            Self::Bga => "pad_prop_bga",
            Self::FiducialGlob => "pad_prop_fiducial_glob",
            Self::FiducialLoc => "pad_prop_fiducial_loc",
            Self::Testpoint => "pad_prop_testpoint",
            Self::Heatsink => "pad_prop_heatsink",
            Self::Castellated => "pad_prop_castellated",
            Self::Mechanical => "pad_prop_mechanical",
            Self::Pressfit => "pad_prop_pressfit",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.name() == name)
    }

    pub(crate) fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.token() == token)
    }
}

/// The name a response uses for a `(property …)` atom: the tool name when the
/// atom is one of KiCad's, and the atom itself otherwise, so a token a newer
/// KiCad adds is still named instead of being read as no property.
pub(crate) fn reported_name(token: &str) -> &str {
    match PadFabricationProperty::from_token(token) {
        Some(property) => property.name(),
        None => token,
    }
}

/// Every value a `fabrication_property` argument takes: `none`, then KiCad's
/// eight in its own order. Tool schemas take their `enum` from here.
pub(crate) fn argument_names() -> Vec<&'static str> {
    std::iter::once(NONE)
        .chain(
            PadFabricationProperty::ALL
                .into_iter()
                .map(PadFabricationProperty::name),
        )
        .collect()
}

/// A `fabrication_property` argument: `Some(None)` for `none`, which removes
/// the clause, `Some(Some(_))` for a property, and `None` for a name that is
/// neither.
pub(crate) fn parse_argument(name: &str) -> Option<Option<PadFabricationProperty>> {
    if name == NONE {
        Some(None)
    } else {
        PadFabricationProperty::from_name(name).map(Some)
    }
}

/// Why a `fabrication_property` argument was refused, for the caller.
pub(crate) fn unsupported_argument_reason(name: &str) -> String {
    format!(
        "unsupported fabrication property '{name}'; expected one of {}",
        argument_names().join(", ")
    )
}

/// Direct `(property …)` children of one `(pad …)` block.
fn property_children(pad_block: &str) -> Vec<(usize, usize)> {
    find_direct_child_blocks(pad_block, "pad")
        .into_iter()
        .filter(|&(start, end)| {
            parse_sexp(&pad_block[start..end]).is_ok_and(|child| child.head() == Some("property"))
        })
        .collect()
}

/// The fabrication property one `(pad …)` block carries, as a response names
/// it: `none` when the pad has no clause.
pub(crate) fn name_in_pad_block(pad_block: &str) -> String {
    property_children(pad_block)
        .first()
        .and_then(|&(start, end)| parse_sexp(&pad_block[start..end]).ok())
        .and_then(|child| {
            child
                .get(1)
                .and_then(SexpNode::as_str)
                .map(|token| reported_name(token).to_string())
        })
        .unwrap_or_else(|| NONE.to_string())
}

/// Whitespace immediately before `index`, so a clause can be added or removed
/// with the indentation and line endings its siblings already use.
fn leading_whitespace_start(source: &str, index: usize) -> usize {
    source[..index]
        .trim_end_matches(|c: char| c.is_ascii_whitespace())
        .len()
}

/// Children KiCad writes before `property` inside a pad. A new clause goes
/// after the last of these, which is where KiCad itself writes it: after the
/// size and the drill, before the layers.
const PAD_CHILDREN_BEFORE_PROPERTY: &[&str] = &["at", "locked", "size", "rect_delta", "drill"];

/// One `(pad …)` block with its fabrication property set, or removed for
/// `None`. Everything else in the block is left byte for byte.
pub(crate) fn set_in_pad_block(
    pad_block: String,
    property: Option<PadFabricationProperty>,
) -> String {
    let existing = property_children(&pad_block);
    let mut edits = Vec::new();
    // A pad has one property. Whatever is asked, a second clause goes.
    let delete = |edits: &mut Vec<SexpEdit>, clauses: &[(usize, usize)]| {
        for &(start, end) in clauses {
            edits.push(SexpEdit::delete(
                leading_whitespace_start(&pad_block, start),
                end,
            ));
        }
    };
    match (property, existing.split_first()) {
        (Some(property), Some((&(start, end), duplicates))) => {
            edits.push(SexpEdit::replace(
                start,
                end,
                format!("(property {})", property.token()),
            ));
            delete(&mut edits, duplicates);
        }
        (Some(property), None) => {
            let clause = format!("(property {})", property.token());
            let children = find_direct_child_blocks(&pad_block, "pad");
            let after = children.iter().rev().find(|&&(start, end)| {
                parse_sexp(&pad_block[start..end]).is_ok_and(|child| {
                    child
                        .head()
                        .is_some_and(|head| PAD_CHILDREN_BEFORE_PROPERTY.contains(&head))
                })
            });
            if let Some(&(start, end)) = after {
                let separator = &pad_block[leading_whitespace_start(&pad_block, start)..start];
                edits.push(SexpEdit::insert(end, format!("{separator}{clause}")));
            } else if let Some(&(start, _)) = children.first() {
                let separator = &pad_block[leading_whitespace_start(&pad_block, start)..start];
                edits.push(SexpEdit::insert(start, format!("{clause}{separator}")));
            } else if let Some(close) = pad_block.rfind(')') {
                edits.push(SexpEdit::insert(close, format!(" {clause}")));
            }
        }
        (None, _) => delete(&mut edits, &existing),
    }
    apply_edits(pad_block, edits)
}

/// One pad of a footprint and the fabrication property it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PadWithProperty {
    /// The pad number as the footprint has it. Several pads can share one
    /// (a connector's shield pads), and it can be empty.
    pub(crate) pad: String,
    pub(crate) fabrication_property: String,
}

/// Every pad of a footprint that carries a fabrication property, in file
/// order. `root` is a `(footprint …)` node, from a library file or a board.
pub(crate) fn pads_with_property(root: &SexpNode) -> Vec<PadWithProperty> {
    root.find_all("pad")
        .into_iter()
        .filter_map(|pad| {
            // A pad's `(property …)` holds one bare atom. A footprint's own
            // `(property "Reference" …)` fields are siblings of the pads, not
            // children, so they are never met here.
            let token = pad.find_str("property")?;
            Some(PadWithProperty {
                pad: pad
                    .get(1)
                    .and_then(SexpNode::as_str)
                    .unwrap_or_default()
                    .to_string(),
                fabrication_property: reported_name(token).to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(pairs: &[(&str, &str)]) -> Vec<PadWithProperty> {
        pairs
            .iter()
            .map(|(pad, property)| PadWithProperty {
                pad: pad.to_string(),
                fabrication_property: property.to_string(),
            })
            .collect()
    }

    #[test]
    fn every_name_maps_to_its_token_and_back() {
        for property in PadFabricationProperty::ALL {
            assert_eq!(
                property.token(),
                format!("pad_prop_{}", property.name()),
                "KiCad's tokens are the names with one prefix"
            );
            assert_eq!(
                PadFabricationProperty::from_name(property.name()),
                Some(property)
            );
            assert_eq!(
                PadFabricationProperty::from_token(property.token()),
                Some(property)
            );
            assert_eq!(parse_argument(property.name()), Some(Some(property)));
        }
        // `none` is the absence of the clause, not a ninth token.
        assert_eq!(PadFabricationProperty::from_name("none"), None);
        assert_eq!(PadFabricationProperty::from_token("pad_prop_none"), None);
        assert_eq!(parse_argument("none"), Some(None));
        assert_eq!(parse_argument("pad_prop_bga"), None);
        assert_eq!(
            argument_names(),
            [
                "none",
                "bga",
                "fiducial_glob",
                "fiducial_loc",
                "testpoint",
                "heatsink",
                "castellated",
                "mechanical",
                "pressfit"
            ]
        );
    }

    #[test]
    fn a_single_line_pad_gets_the_property_after_its_drill() {
        let pad =
            r#"(pad "1" thru_hole circle (at 0 0) (size 2 2) (drill 1) (layers "*.Cu" "*.Mask"))"#;
        let castellated =
            set_in_pad_block(pad.to_string(), Some(PadFabricationProperty::Castellated));
        assert_eq!(
            castellated,
            r#"(pad "1" thru_hole circle (at 0 0) (size 2 2) (drill 1) (property pad_prop_castellated) (layers "*.Cu" "*.Mask"))"#
        );
        assert_eq!(name_in_pad_block(&castellated), "castellated");
        assert_eq!(set_in_pad_block(castellated, None), pad);
        assert_eq!(name_in_pad_block(pad), "none");
    }

    #[test]
    fn a_pad_keeps_one_property_however_many_it_had() {
        let pad = r#"(pad "1" smd rect (at 0 0) (size 2 1) (property pad_prop_bga) (property pad_prop_testpoint) (layers "F.Cu"))"#;
        assert_eq!(
            set_in_pad_block(pad.to_string(), Some(PadFabricationProperty::Heatsink)),
            r#"(pad "1" smd rect (at 0 0) (size 2 1) (property pad_prop_heatsink) (layers "F.Cu"))"#
        );
        assert_eq!(
            set_in_pad_block(pad.to_string(), None),
            r#"(pad "1" smd rect (at 0 0) (size 2 1) (layers "F.Cu"))"#
        );
    }

    /// The clause is placed by the pad's own children, so a quoted pad number
    /// that looks like one is not mistaken for it.
    #[test]
    fn a_pad_number_that_looks_like_a_clause_is_not_an_anchor() {
        let pad = r#"(pad "(drill 1)" smd rect (layers "F.Cu") (size 2 1) (at 0 0))"#;
        assert_eq!(
            set_in_pad_block(pad.to_string(), Some(PadFabricationProperty::Testpoint)),
            r#"(pad "(drill 1)" smd rect (layers "F.Cu") (size 2 1) (at 0 0) (property pad_prop_testpoint))"#
        );
    }

    /// One pin header saved by KiCad with a different `PAD_PROP` on each of
    /// its eight pads, in `PAD_PROP` order: the oracle for every token.
    #[test]
    fn kicad_writes_each_property_under_the_token_this_module_names() {
        let each = konnect_sexp::parse_sexp(include_str!(
            "../../tests/fixtures/pad_property_each_kicad10.kicad_mod"
        ))
        .unwrap();
        let expected = PadFabricationProperty::ALL
            .iter()
            .enumerate()
            .map(|(index, property)| PadWithProperty {
                pad: (index + 1).to_string(),
                fabrication_property: property.name().to_string(),
            })
            .collect::<Vec<_>>();
        assert_eq!(pads_with_property(&each), expected);

        let none = konnect_sexp::parse_sexp(include_str!(
            "../../tests/fixtures/pad_property_none_kicad10.kicad_mod"
        ))
        .unwrap();
        assert_eq!(pads_with_property(&none), Vec::new());
    }

    /// The two stock footprints #193 met on a real board, byte for byte from
    /// KiCad's library: provenance in
    /// `tests/fixtures/pad_property_kicad10.README.md`.
    #[test]
    fn stock_library_pads_are_read_with_their_properties() {
        let wqfn = konnect_sexp::parse_sexp(include_str!(
            "../../tests/fixtures/pad_property_wqfn28_kicad10.kicad_mod"
        ))
        .unwrap();
        assert_eq!(pads_with_property(&wqfn), numbered(&[("29", "heatsink")]));

        let usb_c = konnect_sexp::parse_sexp(include_str!(
            "../../tests/fixtures/pad_property_usb_c_hctl_kicad10.kicad_mod"
        ))
        .unwrap();
        assert_eq!(
            pads_with_property(&usb_c),
            numbered(&[
                ("SH", "mechanical"),
                ("SH", "mechanical"),
                ("SH", "mechanical"),
                ("SH", "mechanical")
            ])
        );

        let plain = konnect_sexp::parse_sexp(include_str!(
            "../../tests/fixtures/c_0603_1608metric_kicad10.kicad_mod"
        ))
        .unwrap();
        assert_eq!(pads_with_property(&plain), Vec::new());
    }

    #[test]
    fn a_token_this_build_does_not_know_is_named_as_written() {
        let footprint = konnect_sexp::parse_sexp(
            r#"(footprint "X" (property "Reference" "REF**" (at 0 0))
                 (pad "1" smd rect (at 0 0) (size 1 1) (property pad_prop_future) (layers "F.Cu"))
                 (pad "2" smd rect (at 2 0) (size 1 1) (layers "F.Cu")))"#,
        )
        .unwrap();
        assert_eq!(
            pads_with_property(&footprint),
            numbered(&[("1", "pad_prop_future")])
        );
    }
}
