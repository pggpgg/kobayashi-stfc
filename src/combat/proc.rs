//! Combat **proc** rolls: weapon intrinsic proc vs officer `Proc*` / `ProcPierceBonus`.
//!
//! # RNG contracts (intentionally two paths)
//!
//! ## [`roll_weapon_intrinsic_proc`] — combatant `proc_chance` / `weapon.proc_chance`
//!
//! Used for outbound shots and defender counter-fire **per hit**. Always consumes **one**
//! [`Rng::next_u64`] sample per call (mapped to \([0,1]\) via `÷ u64::MAX`) so full-fight
//! deterministic seeds stay stable and traces keep one `proc_triggers` row per weapon hit.
//!
//! Success uses clamped chance `c = clamp(chance, 0, 1)`:
//! - `c <= 0` → never procs (draw still consumed).
//! - `c >= 1` → always procs (draw still consumed; avoids `roll < 1.0` edge cases when `roll` rounds to `1.0`).
//! - else → `roll < c`.
//!
//! ## [`roll_proc_chance_short_circuit`] — officer [`AbilityEffect::ProcAttackMultiplier`] / [`ProcPierceBonus`](crate::combat::abilities::AbilityEffect::ProcPierceBonus)
//!
//! Used in the hostile counter loop (and anywhere else we chain officer proc rows). Matches the
//! legacy helper: **no** RNG draw when `chance <= 0` or `chance >= 1` (after clamp), so proc
//! chance `1.0` does not advance the global combat RNG — only effects that actually sample do.
//!
//! # Stacking (current engine)
//!
//! Multiple `ProcAttackMultiplier` rows on the **same counter hit** multiply in stable effect-list
//! order when each roll succeeds. There is **no** per-round global proc cap: each hit re-rolls.
//!
//! # “Caps” and durations
//!
//! There is no separate per-round proc **cap** in the engine today; clamping to `[0,1]` is the
//! only bound. Officer proc rows have no duration — they are instant rolls. Duration refresh/extend
//! applies to other mechanics (e.g. burning, shots bonus), not these proc helpers.

use crate::combat::abilities::{AbilityEffect, ActiveAbilityEffect};
use crate::combat::rng::Rng;

/// Open-\([0,1]\) sample: `next_u64 / u64::MAX` (same convention as crit / legacy weapon proc traces).
#[inline]
pub(crate) fn uniform_open_01(rng: &mut Rng) -> f64 {
    (rng.next_u64() as f64) / (u64::MAX as f64)
}

/// Intrinsic weapon proc for one hit. Always draws [`uniform_open_01`]; returns `(triggered, roll)` for traces.
pub(crate) fn roll_weapon_intrinsic_proc(chance: f64, rng: &mut Rng) -> (bool, f64) {
    let roll = uniform_open_01(rng);
    let c = chance.clamp(0.0, 1.0);
    let triggered = if c <= 0.0 {
        false
    } else if c >= 1.0 {
        true
    } else {
        roll < c
    };
    (triggered, roll)
}

/// Officer-style proc: may **skip** RNG when chance is pinned to 0 or 1 after clamp.
pub(crate) fn roll_proc_chance_short_circuit(chance: f64, rng: &mut Rng) -> bool {
    let c = chance.clamp(0.0, 1.0);
    if c <= 0.0 {
        return false;
    }
    if c >= 1.0 {
        return true;
    }
    uniform_open_01(rng) < c
}

/// Evaluate `Proc*` / `ProcPierceBonus` rows in list order; multipliers multiply, pierce bonuses add.
pub(crate) fn accumulate_proc_attack_effects<'a>(
    effects: impl Iterator<Item = &'a ActiveAbilityEffect>,
    rng: &mut Rng,
) -> (f64, f64) {
    let mut mult = 1.0_f64;
    let mut pierce_bonus = 0.0_f64;
    for e in effects {
        match e.effect {
            AbilityEffect::ProcAttackMultiplier { chance, multiplier }
                if roll_proc_chance_short_circuit(chance, rng) =>
            {
                mult *= multiplier.max(0.0);
            }
            AbilityEffect::ProcPierceBonus { chance, bonus }
                if roll_proc_chance_short_circuit(chance, rng) =>
            {
                pierce_bonus += bonus;
            }
            _ => {}
        }
    }
    (mult, pierce_bonus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::abilities::ActiveAbilityEffect;

    #[test]
    fn weapon_intrinsic_proc_always_consumes_one_draw_even_when_chance_zero() {
        let mut with_proc = Rng::new(42);
        let mut baseline = Rng::new(42);
        let (ok, _) = roll_weapon_intrinsic_proc(0.0, &mut with_proc);
        assert!(!ok);
        let _ = baseline.next_u64();
        assert_eq!(
            with_proc.next_u64(),
            baseline.next_u64(),
            "weapon proc path must consume one RNG step even when chance is 0"
        );
    }

    #[test]
    fn weapon_intrinsic_proc_chance_one_always_triggers() {
        let mut rng = Rng::new(7);
        let (ok, _) = roll_weapon_intrinsic_proc(1.0, &mut rng);
        assert!(ok);
    }

    #[test]
    fn short_circuit_proc_chance_one_does_not_consume_rng() {
        let mut a = Rng::new(5);
        let mut b = Rng::new(5);
        assert!(roll_proc_chance_short_circuit(1.0, &mut a));
        assert_eq!(
            a.next_u64(),
            b.next_u64(),
            "short-circuit at 1.0 must not advance RNG"
        );
    }

    #[test]
    fn short_circuit_proc_chance_zero_does_not_consume_rng() {
        let mut a = Rng::new(11);
        let mut b = Rng::new(11);
        assert!(!roll_proc_chance_short_circuit(0.0, &mut a));
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn accumulate_proc_attack_multipliers_multiply_in_stable_order_when_all_guaranteed() {
        let rows = [
            ActiveAbilityEffect {
                weapon_scope: Default::default(),
                ability_name: "a".into(),
                officer_id: None,
                effect: AbilityEffect::ProcAttackMultiplier {
                    chance: 1.0,
                    multiplier: 2.0,
                },
                boosted: false,
                condition: None,
            },
            ActiveAbilityEffect {
                weapon_scope: Default::default(),
                ability_name: "b".into(),
                officer_id: None,
                effect: AbilityEffect::ProcAttackMultiplier {
                    chance: 1.0,
                    multiplier: 1.5,
                },
                boosted: false,
                condition: None,
            },
        ];
        let (m, pierce) = accumulate_proc_attack_effects(rows.iter(), &mut Rng::new(3));
        assert!((m - 3.0).abs() < 1e-9);
        assert!((pierce - 0.0).abs() < 1e-12);
    }
}
