# Calibration scoreboard

> **Generated.** Do not edit by hand. Regenerate with:
>
> ```bash
> cargo xtask calibration-scoreboard --write docs/CALIBRATION_SCOREBOARD.md
> ```

Measures how closely the combat engine matches reference bands on synthetic drift fixtures and (when populated) snapshot-bound recorded fights.

## Band-width targets

| Layer | Target | Notes |
| --- | --- | --- |
| Drift synthetic | all metrics σ ≤ 1.0 (in band) | CI gate |
| Drift composite | mean σ ≤ 0.35 | Informational |
| Recorded (post-freeze) | outcome exact; mean σ ≤ 2.0 initially | Holdout excluded from iteration composite |

## Iterate rule (post-freeze)

Engine changes are accepted when drift + non-holdout recorded composite improves **and** no non-holdout fight regresses beyond band. Run holdout fights before release; never tune directly to holdout.

See [RECORDED_FIGHT_SUITE_GUIDE.md](RECORDED_FIGHT_SUITE_GUIDE.md) and [CALIBRATION_ADD_FIGHT.md](CALIBRATION_ADD_FIGHT.md).

## Summary

| Metric | Value |
| --- | ---: |
| Drift fixtures passed | 22 |
| Drift fixtures failed | 0 |
| Recorded fights passed | 0 |
| Recorded fights failed | 0 |
| Recorded iteration passed | 0 |
| Recorded iteration failed | 0 |
| Metrics scored | 70 |
| Mean σ (composite) | 0.2902 |
| Max σ | 1.0000 |
| Worst metric | `drift_survey_soak` `attacker_hull_remaining` σ=1.0000 |

## Drift layer (synthetic)

### `drift_conqueror_borg_beam_suppressed`

Mathematical invariant: Borg Sphere attacker (id='borg_sphere') vs Conqueror Borg Obliterator (tag mask=4). The Borg Sphere hull identity provides effective suppression → no instant loss. Combat proceeds normally: attacker (500 atk) kills defender (2000 hull) in 4 rounds. Verifies beam suppression from Borg Sphere hull identity.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 2000.0000 | [1980.0000, 2020.0000] | 0.000 | ok |
| rounds_simulated | 4.0000 | [4.0000, 4.0000] | 0.000 | ok |
| defender_hull_remaining | 0.0000 | [0.0000, 20.0000] | 1.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_conqueror_borg_obliterator_instant_loss`

Mathematical invariant: Conqueror Borg Obliterator (hostile tag mask=4) Hyperthermic Resonance Beam. Non-Borg-Sphere attacker vs Obliterator → instant loss (100% kill rate on non-sphere). Attacker hull=9999, total_damage=0, rounds_simulated=0, attacker_hull_remaining=0.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |
| rounds_simulated | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |
| attacker_hull_remaining | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_conqueror_borg_suppressor_instant_loss`

Mathematical invariant: Conqueror Borg Suppressor (hostile tag mask=2) Quantum Resonance Beam. Non-Borg-Sphere attacker vs Suppressor → instant loss. Attacker hull=9999, total_damage=0, rounds_simulated=0, attacker_hull_remaining=0, attacker_won=false.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |
| rounds_simulated | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |
| attacker_hull_remaining | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_extreme_crit_5x`

Mathematical invariant: 100% crit chance with 5× crit multiplier, 50% mitigation, no pierce. Damage-through = 0.50, crit multiplier = 5.0 → effective = 2.5× per hit. Verifies extreme crit damage scaling.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1500.0000 | [1480.0000, 1520.0000] | 0.000 | ok |
| rounds_simulated | 6.0000 | [6.0000, 6.0000] | 0.000 | ok |
| defender_hull_remaining | 1500.0000 | [1480.0000, 1520.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_extreme_pierce_low_mitigation`

Mathematical invariant: 90% pierce on defender at mitigation floor (0.16). Damage-through = (1-0.16) + 0.90 = 1.74×. Verifies pierce is not spuriously clamped below the mitigation multiplier.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1740.0000 | [1700.0000, 1800.0000] | 0.200 | ok |
| rounds_simulated | 10.0000 | [9.0000, 10.0000] | 1.000 | ok |
| defender_hull_remaining | 1260.0000 | [1200.0000, 1300.0000] | 0.200 | ok |

**fixture_ok:** yes

### `drift_faction_gated_attack_multiplier`

Faction-gated combat in the calibration path: a synthetic Captain AttackMultiplier (+50%) gated on AbilityCondition::DefenderFactionIs(Klingon) via simulation.defender_faction. The defender's faction is supplied explicitly (mirrors the CLI --defender-faction flag), so the multiplier fires and boosts outgoing weapon damage. Single deterministic round (seed 7), defender hull large enough to survive so total_damage measures one round of boosted output. With defender_faction unset (Unknown) the condition would fail and the multiplier would not apply; see the drift.rs unit tests for that comparison.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1500.0000 | [1500.0000, 1500.0000] | 0.000 | ok |
| rounds_simulated | 1.0000 | [1.0000, 1.0000] | 0.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_high_mitigation_vs_high_pierce`

Mathematical invariant: 85% mitigation vs attacker with 80% pierce. Mitigation multiplier = 0.15, damage-through = 0.15 + 0.80 = 0.95×. Verifies high-mitigation high-pierce interaction.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 2850.0000 | [2800.0000, 2900.0000] | 0.000 | ok |
| rounds_simulated | 15.0000 | [14.0000, 15.0000] | 1.000 | ok |
| defender_hull_remaining | 2150.0000 | [2100.0000, 2200.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_hostile_burning_on_attacker`

Mathematical invariant: defender crew applies Burning (1% max hull/round, 5 rounds duration) to attacker via counter-fire. Attacker hull=3000 → 30 dmg/round from Burning. Defender crew refreshes duration to 5 each RoundStart. Attacker takes 30×5 = 150 Burning + 50×5 = 250 counter-fire = 400 total hull damage over 5 rounds. Final attacker hull: 3000 - 400 = 2600.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1000.0000 | [980.0000, 1020.0000] | 0.000 | ok |
| rounds_simulated | 5.0000 | [5.0000, 5.0000] | 0.000 | ok |
| attacker_hull_remaining | 2600.0000 | [2570.0000, 2630.0000] | 0.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_hostile_hull_breach_on_attacker`

Mathematical invariant: defender crew applies Hull Breach to attacker (duration 10 rounds). Defender has 100% crit chance, 2× multiplier. When attacker is hull-breached, defender's crit gets +1.5× HB bonus → effective crit = 3.5×. Defender deals 50×3.5 = 175 dmg/round vs 50×2.0 = 100 without HB. Attacker kills defender in round 5 (200×5=1000). Attacker takes 5 rounds of counter-fire: 175×5 = 875 total + includes Burning sub-procs from HB timing window.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1000.0000 | [980.0000, 1020.0000] | 0.000 | ok |
| rounds_simulated | 5.0000 | [5.0000, 5.0000] | 0.000 | ok |
| attacker_hull_remaining | 4300.0000 | [3900.0000, 4300.0000] | 1.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_hull_breach_crit_interaction`

Mathematical invariant: Hull Breach active on defender, attacker has 100% crit chance with 2× base crit multiplier. Effective crit = 2.0 × 1.5 (HB bonus) = 3.0×. Mitigation 0.50, no pierce → 0.50 through × 3.0 = 1.5× per hit.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1500.0000 | [1480.0000, 1520.0000] | 0.000 | ok |
| rounds_simulated | 10.0000 | [10.0000, 10.0000] | 0.000 | ok |
| defender_hull_remaining | 1500.0000 | [1480.0000, 1520.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_interceptor_dual_weapon`

Two-weapon attacker vs single-hull defender; exercises sub-round ordering.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 875.6000 | [300.0000, 4000.0000] | 0.689 | ok |
| rounds_simulated | 5.0000 | [1.0000, 15.0000] | 0.429 | ok |
| defender_hull_remaining | 0.0000 | [0.0000, 650.0000] | 1.000 | ok |
| defender_shield_remaining | 0.0000 | [0.0000, 150.0000] | 1.000 | ok |
| attacker_hull_remaining | 1500.0000 | [800.0000, 1500.0000] | 1.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_mitigation_floor_clamp`

Mathematical invariant: defender with mitigation 0.0 (below hostile floor 0.16). NOTE: hostile mitigation params are not wired through the drift harness yet; this fixture verifies that mitigation=0.0 passes through at 1.0× (no floor clamp). When hostile params are wired, expected damage-through would be 0.84×, and bands should tighten to [830, 850].

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1000.0000 | [990.0000, 1010.0000] | 0.000 | ok |
| rounds_simulated | 10.0000 | [10.0000, 10.0000] | 0.000 | ok |
| defender_hull_remaining | 2000.0000 | [1990.0000, 2010.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_morale_burning_hullbreach_stacking`

Mathematical invariant: Morale, Burning, and Hull Breach all active simultaneously. Morale adds +10% primary piercing (requires hostile mitigation params, not wired). Burning ticks 1% max hull/round = 30. Hull Breach adds 1.5× crit damage (0% crit here). Shot damage: 10×100×0.50 = 500 + 10×30 Burning = 800 total. Verifies all three status effects fire without conflicts.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 800.0000 | [780.0000, 820.0000] | 0.000 | ok |
| rounds_simulated | 10.0000 | [10.0000, 10.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_multi_shot_equal_damage`

Two-shot weapon with zero mitigation/crit vs huge-hull defender: each hit deals the same base damage (1000 + 1000 = 2000 in round 1). Guards against per-hit accumulation of the PreAttackDamage stack base (hit N must not deal N x base).

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 2000.0000 | [1999.9000, 2000.1000] | 0.000 | ok |
| rounds_simulated | 1.0000 | [1.0000, 1.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_multi_weapon_crit_ordering`

Mathematical invariant: Two weapons with different crit stats fire in order each round. Weapon A: 0% crit, 100 attack. Weapon B: 100% crit, 3× multiplier, 50 attack. Verifies sub-round weapon ordering is deterministic and per-weapon crit stats are used.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1250.0000 | [1240.0000, 1260.0000] | 0.000 | ok |
| rounds_simulated | 10.0000 | [10.0000, 10.0000] | 0.000 | ok |
| defender_hull_remaining | 2750.0000 | [2740.0000, 2760.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_on_kill_hull_regen`

Mathematical invariant: attacker has on_kill_hull_regen = 0.15 (15% of max hull healed on kill). Defender deals 200 damage/round (no mitigation on attacker). Attacker (200 atk) kills 1000-hull defender in round 5. Before kill: attacker took 200×4 = 800 damage from defender counter-fire. On kill: heals 0.15×3000 = 450. Final attacker hull: 3000 - 800 + 450 = 2650.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1000.0000 | [980.0000, 1020.0000] | 0.000 | ok |
| rounds_simulated | 5.0000 | [5.0000, 5.0000] | 0.000 | ok |
| attacker_hull_remaining | 2450.0000 | [2400.0000, 2500.0000] | 0.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_research_weapon_damage_additive_pool`

Synthetic calibration slice for merged research/profile weapon_damage: uses weapon_damage_profile_additive_pool with round-start AttackMultiplier (pre_attack path). Companion: drift_research_weapon_damage_layered_no_pool.json (same crew/stats, layered model, higher per-hit effective attack).

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 8064.0000 | [8040.0000, 8090.0000] | 0.040 | ok |
| rounds_simulated | 36.0000 | [36.0000, 36.0000] | 0.000 | ok |
| defender_hull_remaining | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |
| attacker_hull_remaining | 8000.0000 | [7999.0000, 8000.0000] | 1.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_research_weapon_damage_layered_no_pool`

Same combatants/seed/crew as drift_research_weapon_damage_additive_pool but weapon_damage_profile_additive_pool unset (layered (1+p)×pre_attack path). total_damage should exceed the pooled fixture when pre_attack_multiplier > 1.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 8160.0000 | [8140.0000, 8180.0000] | 0.000 | ok |
| rounds_simulated | 34.0000 | [34.0000, 34.0000] | 0.000 | ok |
| defender_hull_remaining | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |
| attacker_hull_remaining | 8000.0000 | [7999.0000, 8000.0000] | 1.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes

### `drift_shield_depletion_interaction`

Mathematical invariant: defender with 1000 shield (80% absorption). Attacker with 200 damage/hit, 0% pierce, 50% mitigation → 100 through/hit. Shield absorbs 80% (80 dmg) → shield lasts 12.5 hits. After depletion, all damage hits hull. Verifies shield → hull transition.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 3000.0000 | [2900.0000, 3100.0000] | 0.000 | ok |
| rounds_simulated | 30.0000 | [28.0000, 30.0000] | 1.000 | ok |
| defender_shield_remaining | 0.0000 | [0.0000, 0.0000] | 0.000 | ok |

**fixture_ok:** yes

### `drift_stall_margin`

Barely enough DPR to threaten a tanky defender; may hit round cap without kill.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 470.0000 | [50.0000, 6000.0000] | 0.859 | ok |
| rounds_simulated | 25.0000 | [10.0000, 25.0000] | 1.000 | ok |
| defender_hull_remaining | 4906.0000 | [1000.0000, 5000.0000] | 0.953 | ok |
| defender_shield_remaining | 1624.0000 | [0.0000, 2000.0000] | 0.624 | ok |
| attacker_hull_remaining | 5000.0000 | [4000.0000, 5000.0000] | 1.000 | ok |

**fixture_ok:** yes

### `drift_survey_soak`

Low pierce survey-style attacker vs high-mitigation defender with shields; long horizon soak.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 1140.0000 | [400.0000, 3200.0000] | 0.471 | ok |
| rounds_simulated | 20.0000 | [1.0000, 20.0000] | 1.000 | ok |
| defender_hull_remaining | 60.0000 | [0.0000, 800.0000] | 0.850 | ok |
| defender_shield_remaining | 0.0000 | [0.0000, 400.0000] | 1.000 | ok |
| attacker_hull_remaining | 3000.0000 | [2500.0000, 3000.0000] | 1.000 | ok |

**fixture_ok:** yes

### `drift_weapon_type_kinetic_gate`

Weapon-type dimension: a synthetic Captain ShotsBonus (+100% shots, 1 round, chance 1.0) at CombatBegin scoped KineticOnly (Kuron ModuleKinetic recharge shape). The attacker carries one kinetic and one energy weapon, 1000 damage each, zero mitigation/pierce/crit. Round 1: the kinetic weapon fires round_half_even(1 x (1+1)) = 2 shots, the energy weapon keeps 1 shot. Each hit deals equal base damage (multi-shot equal-damage fix), so the kinetic weapon deals 2000 and the energy weapon 1000, total_damage = 3000. Without the weapon-type gate both weapons would double (4000); with the gate wrongly excluding everything it would be 2000 - the band pins the kinetic-only value exactly.

| metric | actual | band | σ | status |
| --- | ---: | --- | ---: | --- |
| total_damage | 3000.0000 | [3000.0000, 3000.0000] | 0.000 | ok |
| rounds_simulated | 1.0000 | [1.0000, 1.0000] | 0.000 | ok |

**attacker_won:** ok

**fixture_ok:** yes


## Recorded layer (snapshot-bound)

_No recorded fights in manifest yet — populate after snapshot freeze._
