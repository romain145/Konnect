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

use konnect_sexp::SexpNode;
use serde::Serialize;

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
                PadFabricationProperty::from_token(property.token()),
                Some(property)
            );
        }
        // `none` is the absence of the clause, not a ninth token.
        assert_eq!(PadFabricationProperty::from_token("pad_prop_none"), None);
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
