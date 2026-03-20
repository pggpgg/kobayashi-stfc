//! Pre-combat mitigation sensitivity tables: how small stat deltas move mitigation and damage-through.
//!
//! See repository file `docs/COMBAT_TRACE.md` for how these relate to combat trace events.

use crate::combat::damage::compute_damage_through_factor;
use crate::combat::mitigation::{mitigation_for_hostile, pierce_damage_through_bonus};
use crate::combat::types::{AttackerStats, DefenderStats, ShipType};

/// One row of a sensitivity table (pre-combat, no crew phase bonuses unless you set `defense_mitigation_bonus`).
#[derive(Debug, Clone, PartialEq)]
pub struct MitigationSensitivityRow {
    pub label: &'static str,
    /// Total mitigation after hostile clamp (matches [`mitigation_for_hostile`]).
    pub mitigation: f64,
    /// `max(0, 1 - mitigation)` — same sense as trace `mitigation_calc.multiplier`.
    pub mitigation_multiplier: f64,
    /// Additive pierce damage-through term from [`pierce_damage_through_bonus`].
    pub pierce_additive: f64,
    /// Optional defense-phase mitigation bonus (usually 0 in this table; non-zero when modeling a known buff).
    pub defense_mitigation_bonus: f64,
    /// `compute_damage_through_factor(mitigation_multiplier, pierce_additive, defense_mitigation_bonus)`.
    pub damage_through_factor: f64,
}

/// Baseline stats for hostile defender sensitivity (mirrors ship-vs-hostile pre-combat resolution).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostileMitigationBaseline {
    pub defender: DefenderStats,
    pub attacker: AttackerStats,
    pub ship_type: ShipType,
    pub mystery_mitigation_factor: f64,
    pub mitigation_floor: f64,
    pub mitigation_ceiling: f64,
    pub defense_mitigation_bonus: f64,
}

impl HostileMitigationBaseline {
    /// Row for arbitrary defender/attacker stats (same hostile metadata as baseline).
    pub fn row_for(
        &self,
        label: &'static str,
        defender: DefenderStats,
        attacker: AttackerStats,
    ) -> MitigationSensitivityRow {
        let mitigation = mitigation_for_hostile(
            defender,
            attacker,
            self.ship_type,
            self.mystery_mitigation_factor,
            self.mitigation_floor,
            self.mitigation_ceiling,
        );
        let pierce_additive = pierce_damage_through_bonus(defender, attacker, self.ship_type);
        let mitigation_multiplier = (1.0 - mitigation).max(0.0);
        let damage_through_factor = compute_damage_through_factor(
            mitigation_multiplier,
            pierce_additive,
            self.defense_mitigation_bonus,
        );
        MitigationSensitivityRow {
            label,
            mitigation,
            mitigation_multiplier,
            pierce_additive,
            defense_mitigation_bonus: self.defense_mitigation_bonus,
            damage_through_factor,
        }
    }

    pub fn baseline_row(&self) -> MitigationSensitivityRow {
        self.row_for("baseline", self.defender, self.attacker)
    }
}

/// Default sensitivity rows: baseline plus ±`pct` multiplicative bumps on each defense and piercing scalar.
/// `pct` is a fraction (e.g. `0.1` for +10%).
pub fn default_percent_sensitivity_rows(
    base: &HostileMitigationBaseline,
    pct: f64,
) -> Vec<MitigationSensitivityRow> {
    let d = base.defender;
    let a = base.attacker;
    let mut rows = vec![base.baseline_row()];
    rows.push(base.row_for(
        "def_armor_up_pct",
        DefenderStats {
            armor: d.armor * (1.0 + pct),
            ..d
        },
        a,
    ));
    rows.push(base.row_for(
        "def_shield_def_up_pct",
        DefenderStats {
            shield_deflection: d.shield_deflection * (1.0 + pct),
            ..d
        },
        a,
    ));
    rows.push(base.row_for(
        "def_dodge_up_pct",
        DefenderStats {
            dodge: d.dodge * (1.0 + pct),
            ..d
        },
        a,
    ));
    rows.push(base.row_for(
        "atk_armor_pierce_up_pct",
        d,
        AttackerStats {
            armor_piercing: a.armor_piercing * (1.0 + pct),
            ..a
        },
    ));
    rows.push(base.row_for(
        "atk_shield_pierce_up_pct",
        d,
        AttackerStats {
            shield_piercing: a.shield_piercing * (1.0 + pct),
            ..a
        },
    ));
    rows.push(base.row_for(
        "atk_accuracy_up_pct",
        d,
        AttackerStats {
            accuracy: a.accuracy * (1.0 + pct),
            ..a
        },
    ));
    rows
}

/// TSV header + rows for terminal or CSV pipelines.
pub fn format_sensitivity_tsv(rows: &[MitigationSensitivityRow]) -> String {
    let mut s = String::from(
        "label\tmitigation\tmitigation_multiplier\tpierce_additive\tdefense_mitigation_bonus\tdamage_through_factor\n",
    );
    for r in rows {
        s.push_str(&format!(
            "{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\n",
            r.label,
            r.mitigation,
            r.mitigation_multiplier,
            r.pierce_additive,
            r.defense_mitigation_bonus,
            r.damage_through_factor
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::mitigation::{MITIGATION_CEILING, MITIGATION_FLOOR};

    #[test]
    fn baseline_row_matches_direct_mitigation_and_damage_through() {
        let defender = DefenderStats {
            armor: 1200.0,
            shield_deflection: 800.0,
            dodge: 400.0,
        };
        let attacker = AttackerStats {
            armor_piercing: 900.0,
            shield_piercing: 700.0,
            accuracy: 500.0,
        };
        let base = HostileMitigationBaseline {
            defender,
            attacker,
            ship_type: ShipType::Battleship,
            mystery_mitigation_factor: 0.05,
            mitigation_floor: MITIGATION_FLOOR,
            mitigation_ceiling: MITIGATION_CEILING,
            defense_mitigation_bonus: 0.0,
        };
        let row = base.baseline_row();
        let expect_mit = mitigation_for_hostile(
            defender,
            attacker,
            ShipType::Battleship,
            0.05,
            MITIGATION_FLOOR,
            MITIGATION_CEILING,
        );
        let expect_pierce = pierce_damage_through_bonus(defender, attacker, ShipType::Battleship);
        let expect_mult = (1.0 - expect_mit).max(0.0);
        let expect_dtf =
            compute_damage_through_factor(expect_mult, expect_pierce, 0.0);
        assert!((row.mitigation - expect_mit).abs() < 1e-9);
        assert!((row.pierce_additive - expect_pierce).abs() < 1e-9);
        assert!((row.damage_through_factor - expect_dtf).abs() < 1e-9);
    }

    #[test]
    fn armor_up_increases_mitigation_vs_baseline() {
        let defender = DefenderStats {
            armor: 1000.0,
            shield_deflection: 500.0,
            dodge: 300.0,
        };
        let attacker = AttackerStats {
            armor_piercing: 800.0,
            shield_piercing: 600.0,
            accuracy: 400.0,
        };
        let base = HostileMitigationBaseline {
            defender,
            attacker,
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            mitigation_floor: 0.0,
            mitigation_ceiling: 1.0,
            defense_mitigation_bonus: 0.0,
        };
        let rows = default_percent_sensitivity_rows(&base, 0.1);
        let b = &rows[0];
        let arm = rows.iter().find(|r| r.label == "def_armor_up_pct").unwrap();
        assert!(arm.mitigation >= b.mitigation);
        assert!(arm.damage_through_factor <= b.damage_through_factor + 1e-9);
    }
}
