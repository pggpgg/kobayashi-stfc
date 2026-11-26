//! End-to-end tests for the Augment Exile [WOK] loop, U.S.S. Reliant ship abilities
//! (`Always Reliant`, `Reliant Barrier`, `M/ARA Insulation`), and Hijacked D7 / DY-200
//! `Not So Wounded` + `Dilithium Destabilization`.

use kobayashi::combat::hostile_tags::{
    hostile_tag_mask_for_slug, mask_from_native_faction, HOSTILE_TAG_MASK_AUGMENT_EXILE,
};
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect, Combatant,
    CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig, WeaponStats, WeaponType,
};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::ship_ability_resolve::ship_abilities_to_crew_seat_contexts;

fn simple_combatant(id: &str, attack: f64, hull: f64, shield: f64) -> Combatant {
    Combatant {
        id: id.to_string(),
        attack,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: hull,
        shield_health: shield,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack,
            shots: Some(1),
            pierce: Some(0.0),
            crit_chance: Some(0.0),
            crit_multiplier: Some(1.0),
            proc_chance: Some(0.0),
            proc_multiplier: Some(1.0),
            weapon_type: WeaponType::Energy,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

#[test]
fn all_96_augment_exile_hostiles_carry_augment_exile_tag_and_d7_dy200_have_not_so_wounded() {
    assert_eq!(
        hostile_tag_mask_for_slug("augment_exile"),
        Some(HOSTILE_TAG_MASK_AUGMENT_EXILE)
    );
    assert_eq!(
        mask_from_native_faction(1_306_860_549),
        HOSTILE_TAG_MASK_AUGMENT_EXILE
    );

    let registry = DataRegistry::load().expect("DataRegistry::load");
    let catalog = hostile_ability_catalog_for_default_path().expect("hostile ability catalog");
    let index = registry.hostile_index().expect("hostile index");

    let mut exile_count = 0usize;
    let mut d7_count = 0usize;
    let mut dy200_count = 0usize;

    for entry in &index.hostiles {
        let rec = registry.resolve_hostile(&entry.id).expect("hostile record");
        let is_exile_faction = rec.faction.as_ref().is_some_and(|f| f.id == 1_306_860_549);
        if is_exile_faction {
            exile_count += 1;
            assert_ne!(
                rec.hostile_tag_mask() & HOSTILE_TAG_MASK_AUGMENT_EXILE,
                0,
                "Exile hostile {} ({}) must carry HOSTILE_TAG_MASK_AUGMENT_EXILE",
                rec.id,
                rec.hostile_name
            );
            assert!(
                rec.effective_hostile_tags()
                    .iter()
                    .any(|t| t == "augment_exile"),
                "Exile hostile {} must include augment_exile in effective_hostile_tags",
                rec.id
            );

            if rec.hostile_name == "Hijacked D7 Class" || rec.hostile_name == "DY-200-class" {
                if rec.hostile_name == "Hijacked D7 Class" {
                    d7_count += 1;
                } else {
                    dy200_count += 1;
                }
                let def_crew =
                    hostile_abilities_to_defender_crew(&rec.ability, Some(catalog), rec.level);
                assert!(
                    def_crew.seats.iter().any(|s| matches!(
                        s.ability.effect,
                        AbilityEffect::IsolyticDamageBonus(v) if (v - 0.15).abs() < 1e-6
                    )),
                    "{} ({}) must resolve Augmented Force (+15% isolytic)",
                    rec.hostile_name,
                    rec.id
                );
                assert!(
                    def_crew.seats.iter().any(|s| matches!(
                        s.ability.effect,
                        AbilityEffect::ApexBarrierBonus(v) if (v - 5000.0).abs() < 1e-6
                    )),
                    "{} ({}) must resolve Not So Wounded (+5000 Apex Barrier from 492210137)",
                    rec.hostile_name,
                    rec.id
                );
                assert!(
                    def_crew.seats.iter().any(|s| matches!(
                        s.ability.effect,
                        AbilityEffect::HostileLethalCombatBegin { chance } if chance > 0.0
                    )),
                    "{} ({}) must resolve Dilithium Destabilization",
                    rec.hostile_name,
                    rec.id
                );
            }
        } else {
            assert_eq!(
                rec.hostile_tag_mask() & HOSTILE_TAG_MASK_AUGMENT_EXILE,
                0,
                "Non-Exile hostile {} ({}) must not carry augment_exile tag",
                rec.id,
                rec.hostile_name
            );
        }
    }

    assert_eq!(exile_count, 96, "expected 96 Augment Exile hostiles");
    assert_eq!(d7_count, 23, "expected 23 Hijacked D7 Class hostiles");
    assert_eq!(dy200_count, 4, "expected 4 DY-200-class hostiles");
}

#[test]
fn uss_reliant_abilities_gate_on_augment_exile_and_block_dilithium_destabilization() {
    let registry = DataRegistry::load().expect("DataRegistry::load");
    let catalog = hostile_ability_catalog_for_default_path().expect("hostile ability catalog");

    let reliant = registry
        .resolve_ship_with_tier_level("uss_reliant", Some(5), Some(25))
        .expect("uss_reliant record");
    let abilities = reliant.abilities.as_deref().expect("uss_reliant abilities");
    let reliant_seats = ship_abilities_to_crew_seat_contexts(abilities);

    assert!(
        reliant_seats.iter().any(
            |s| matches!(s.ability.effect, AbilityEffect::IsolyticDamageBonus(v) if v >= 11.0)
        ),
        "U.S.S. Reliant must resolve Always Reliant isolytic damage, seats={reliant_seats:?}"
    );
    assert!(
        reliant_seats
            .iter()
            .any(|s| matches!(s.ability.effect, AbilityEffect::ApexBarrierBonus(v) if (v - 50_000.0).abs() < 1e-6)),
        "U.S.S. Reliant must resolve Reliant Barrier (+50,000 Apex Barrier)"
    );
    assert!(
        reliant_seats.iter().any(|s| matches!(
            s.ability.effect,
            AbilityEffect::DilithiumDestabilizationImmunity
        )),
        "U.S.S. Reliant must resolve M/ARA Insulation (DilithiumDestabilizationImmunity)"
    );

    let reliant_crew = CrewConfiguration {
        seats: reliant_seats,
    };

    // Load a real Hijacked D7 Class (90% Dilithium Destabilization).
    let d7 = registry
        .resolve_hostile("565037258")
        .expect("Hijacked D7 Class L40");
    assert_eq!(d7.hostile_name, "Hijacked D7 Class");
    let d7_defender_crew = hostile_abilities_to_defender_crew(&d7.ability, Some(catalog), d7.level);

    let attacker = simple_combatant("uss_reliant", 10_000.0, 10_000_000.0, 0.0);
    let defender = simple_combatant("hijacked_d7", 100_000.0, 100_000_000.0, 0.0);

    // Against Augment Exile with Dilithium Destabilization (90% chance):
    // - Without M/ARA Insulation, most seeds die instantaneously on round 0.
    // - With U.S.S. Reliant's M/ARA Insulation, 0 of 30 seeds die on round 0, AND Reliant gets
    //   Always Reliant isolytic damage + Reliant Barrier (+50,000 Apex Barrier).
    let mut non_reliant_instant_losses = 0u32;
    let mut reliant_instant_losses = 0u32;
    for seed in 1..=30u64 {
        let cfg = SimulationConfig {
            rounds: 2,
            seed,
            defender_hostile_tag_mask: d7.hostile_tag_mask(),
            defender_hull_faction_id: d7.faction.as_ref().map_or(0, |f| f.id),
            defender_level: Some(d7.level),
            ..Default::default()
        };
        let plain = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &CrewConfiguration::default(),
            OpponentFactionTag::Unknown,
            ShipType::Interceptor,
            ShipType::Explorer,
            true,
            false,
            &d7_defender_crew,
        );
        if plain.rounds_simulated == 0 && !plain.attacker_won {
            non_reliant_instant_losses += 1;
        }
        let with_reliant = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &reliant_crew,
            OpponentFactionTag::Unknown,
            ShipType::Interceptor,
            ShipType::Explorer,
            true,
            false,
            &d7_defender_crew,
        );
        if with_reliant.rounds_simulated == 0 && !with_reliant.attacker_won {
            reliant_instant_losses += 1;
        }
        assert!(
            with_reliant.total_isolytic_damage > 100_000.0,
            "Always Reliant must deal heavy isolytic damage vs Augment Exile (seed={seed}, iso={})",
            with_reliant.total_isolytic_damage
        );
        // Reliant Barrier (+50,000) reduces incoming 100k damage by 10k/(10k+50k) = 1/6th per round.
        let taken = attacker.hull_health - with_reliant.attacker_hull_remaining;
        assert!(
            taken < 60_000.0,
            "Reliant Barrier (+50k Apex Barrier) must sharply reduce Exile counter damage (taken={taken})"
        );
    }
    assert!(
        non_reliant_instant_losses >= 20,
        "Non-Reliant ship vs 90% Dilithium Destabilization should instant-die most runs, got {non_reliant_instant_losses}/30"
    );
    assert_eq!(
        reliant_instant_losses, 0,
        "U.S.S. Reliant M/ARA Insulation must prevent all Dilithium Destabilization instant losses"
    );

    // Gate check: against a non-Exile NPC hostile (tag mask = 0) AND against a player ship (PvP),
    // Always Reliant, Reliant Barrier, and M/ARA Insulation must all remain strictly inert!
    for (is_npc, is_player, label) in [
        (true, false, "non-Exile NPC hostile"),
        (false, true, "PvP player ship"),
    ] {
        let cfg = SimulationConfig {
            rounds: 2,
            seed: 7,
            defender_hostile_tag_mask: 0,
            ..Default::default()
        };
        let plain = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &CrewConfiguration::default(),
            OpponentFactionTag::Federation,
            ShipType::Explorer,
            ShipType::Explorer,
            is_npc,
            is_player,
            &CrewConfiguration::default(),
        );
        let with_reliant = simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &reliant_crew,
            OpponentFactionTag::Federation,
            ShipType::Explorer,
            ShipType::Explorer,
            is_npc,
            is_player,
            &CrewConfiguration::default(),
        );
        assert!(
            (with_reliant.total_damage - plain.total_damage).abs() < 1e-6
                && (with_reliant.total_isolytic_damage - plain.total_isolytic_damage).abs() < 1e-6
                && (with_reliant.attacker_hull_remaining - plain.attacker_hull_remaining).abs()
                    < 1e-6,
            "Reliant abilities must stay inert vs {label}: plain=({},{},{}), reliant=({},{},{})",
            plain.total_damage,
            plain.total_isolytic_damage,
            plain.attacker_hull_remaining,
            with_reliant.total_damage,
            with_reliant.total_isolytic_damage,
            with_reliant.attacker_hull_remaining
        );
    }
}
