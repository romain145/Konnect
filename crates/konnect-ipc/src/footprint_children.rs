//! What a footprint owns that KiCad's API cannot carry through an update (#119).
//!
//! KiCad's API updates a footprint by replacing it with one built from the
//! `FootprintInstance` sent (`handleCreateUpdateItemsInternal`). That message
//! carries none of the footprint's groups or design variants, and carries its
//! `(point …)`s only when KiCad sent them: 10.0.8 sends them as
//! [`REFERENCE_POINT`] children, 10.0.6 does not. No item read lists groups or
//! variants, so the board as KiCad writes it (`SaveDocumentToString`) is the
//! only place they can be seen.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use konnect_sexp::SexpNode;
use prost::Message;

use crate::gen::kiapi;
use crate::transform::REFERENCE_POINT;

/// The groups, points and variants a footprint owns.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FootprintChildren {
    pub groups: usize,
    pub points: usize,
    /// Each variant's name, as KiCad writes it.
    pub variants: Vec<String>,
}

impl FootprintChildren {
    /// What a `(footprint …)` block of a board KiCad wrote owns.
    pub fn of(footprint: &SexpNode) -> Self {
        FootprintChildren {
            groups: footprint.find_all("group").len(),
            points: footprint.find_all("point").len(),
            variants: footprint
                .find_all("variant")
                .into_iter()
                // KiCad always writes a variant's name.
                .map(|variant| variant.find_str("name").unwrap_or_default().to_string())
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == FootprintChildren::default()
    }

    /// What an update loses when its message carries `carried_points` of the
    /// footprint's points.
    pub fn lost_with(&self, carried_points: usize) -> Self {
        FootprintChildren {
            points: self.points.saturating_sub(carried_points),
            ..self.clone()
        }
    }

    /// `1 group, 2 points and variant 'NoTimer'`.
    pub fn described(&self) -> String {
        fn counted(count: usize, noun: &str) -> Option<String> {
            match count {
                0 => None,
                1 => Some(format!("1 {noun}")),
                n => Some(format!("{n} {noun}s")),
            }
        }
        let quoted = || {
            self.variants
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let variants = match self.variants.len() {
            0 => None,
            1 => Some(format!("variant {}", quoted())),
            _ => Some(format!("variants {}", quoted())),
        };
        let mut parts = [
            counted(self.groups, "group"),
            counted(self.points, "point"),
            variants,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let last = parts.pop().unwrap_or_default();
        if parts.is_empty() {
            last
        } else {
            format!("{} and {last}", parts.join(", "))
        }
    }
}

/// Every footprint in a board KiCad wrote, by KIID, with what it owns. A KIID
/// written twice owns what both footprints own, so neither hides the other.
pub fn footprint_children_by_kiid(board_text: &str) -> Result<BTreeMap<String, FootprintChildren>> {
    let tree =
        konnect_sexp::parse_sexp(board_text).context("the board KiCad wrote did not parse")?;
    let mut owned = BTreeMap::<String, FootprintChildren>::new();
    for footprint in konnect_sexp::board::footprints(&tree) {
        let Some(kiid) = footprint.find_str("uuid") else {
            continue;
        };
        let found = FootprintChildren::of(footprint);
        let entry = owned.entry(kiid.to_string()).or_default();
        entry.groups += found.groups;
        entry.points += found.points;
        entry.variants.extend(found.variants);
    }
    Ok(owned)
}

/// How many of its points a footprint message carries.
pub fn reference_points_carried(footprint: &kiapi::board::types::FootprintInstance) -> usize {
    footprint.definition.as_ref().map_or(0, |definition| {
        definition
            .items
            .iter()
            .filter(|item| crate::builders::any_is(item, REFERENCE_POINT))
            .count()
    })
}

/// [`reference_points_carried`] for a packed `FootprintInstance`; 0 for any
/// other item or one that does not decode.
pub fn reference_points_carried_by(item: &prost_types::Any) -> usize {
    if !crate::builders::any_is(item, "kiapi.board.types.FootprintInstance") {
        return 0;
    }
    kiapi::board::types::FootprintInstance::decode(item.value.as_slice())
        .map_or(0, |footprint| reference_points_carried(&footprint))
}

/// One footprint an update would send.
pub(crate) struct SentFootprint {
    pub kiid: String,
    pub reference: String,
    pub carried_points: usize,
}

/// Each footprint in `sent` that would lose what it owns, described.
///
/// A footprint `board_text` does not hold, or a `board_text` that does not
/// parse, is passed: that is no evidence of a loss, and KiCad answers an
/// update of an unknown KIID itself.
pub(crate) fn updates_dropping_children(board_text: &str, sent: &[SentFootprint]) -> Vec<String> {
    let owned = match footprint_children_by_kiid(board_text) {
        Ok(owned) => owned,
        Err(error) => {
            tracing::debug!("footprint children not checked before update: {error:#}");
            return Vec::new();
        }
    };
    sent.iter()
        .filter_map(|footprint| {
            let lost = owned
                .get(&footprint.kiid)?
                .lost_with(footprint.carried_points);
            if lost.is_empty() {
                return None;
            }
            let name = if footprint.reference.is_empty() {
                footprint.kiid.clone()
            } else {
                format!("{} ({})", footprint.reference, footprint.kiid)
            };
            Some(format!("{name} owns {}", lost.described()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A footprint owning a group, two points and two variants, one owning
    /// one point, and one owning nothing.
    const BOARD: &str = "(kicad_pcb\n\
        (footprint \"Package_DIP:DIP-8_W7.62mm\" (uuid \"all\")\n\
          (property \"Reference\" \"U1\")\n\
          (point (at 0 0) (size 1.27) (layer \"F.Cu\") (uuid \"p1\"))\n\
          (point (at 1 0) (size 1.27) (layer \"F.Cu\") (uuid \"p2\"))\n\
          (group \"\" (uuid \"g\") (members \"m\"))\n\
          (variant (name \"NoTimer\") (dnp yes))\n\
          (variant (name \"Lite\") (dnp yes)))\n\
        (footprint \"Package_DIP:DIP-8_W7.62mm\" (uuid \"point\")\n\
          (point (at 0 0) (size 1.27) (layer \"F.Cu\") (uuid \"p3\")))\n\
        (footprint \"Resistor_SMD:R_0402\" (uuid \"none\")))";

    fn sent(kiid: &str, reference: &str, carried_points: usize) -> SentFootprint {
        SentFootprint {
            kiid: kiid.to_string(),
            reference: reference.to_string(),
            carried_points,
        }
    }

    #[test]
    fn what_a_footprint_owns_is_read_and_described() {
        let owned = footprint_children_by_kiid(BOARD).unwrap();

        assert_eq!(
            owned["all"],
            FootprintChildren {
                groups: 1,
                points: 2,
                variants: vec!["NoTimer".to_string(), "Lite".to_string()],
            }
        );
        assert_eq!(
            owned["all"].described(),
            "1 group, 2 points and variants 'NoTimer', 'Lite'"
        );
        assert_eq!(owned["point"].described(), "1 point");
        assert!(owned["none"].is_empty());
    }

    /// Whichever footprint comes last, neither hides what the other owns.
    #[test]
    fn a_kiid_written_twice_owns_what_both_footprints_own() {
        let owned = footprint_children_by_kiid(
            "(kicad_pcb\n\
             (footprint \"A\" (uuid \"twice\") (group \"\" (uuid \"g\") (members \"m\")))\n\
             (footprint \"B\" (uuid \"twice\") (point (at 0 0) (uuid \"p\"))))",
        )
        .unwrap();

        assert_eq!(
            owned["twice"],
            FootprintChildren {
                groups: 1,
                points: 1,
                variants: Vec::new(),
            }
        );
    }

    /// Groups and variants are always lost; points only when not sent.
    #[test]
    fn an_update_loses_groups_variants_and_the_points_it_does_not_carry() {
        let dropped = updates_dropping_children(
            BOARD,
            &[
                sent("all", "U1", 2),
                sent("point", "U2", 0),
                sent("point", "U3", 1),
                sent("none", "R1", 0),
            ],
        );

        assert_eq!(
            dropped,
            vec![
                "U1 (all) owns 1 group and variants 'NoTimer', 'Lite'",
                "U2 (point) owns 1 point",
            ]
        );
    }

    /// No evidence of a loss is not a reason to refuse.
    #[test]
    fn an_unknown_footprint_or_an_unreadable_board_is_passed() {
        assert!(updates_dropping_children(BOARD, &[sent("elsewhere", "U9", 0)]).is_empty());
        assert!(
            updates_dropping_children("(kicad_pcb (footprint", &[sent("all", "U1", 0)]).is_empty()
        );
    }
}
