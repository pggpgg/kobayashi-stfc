//! Source-backed Premium Hostile Combat Hull Repair, independently of player stats.
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, AbilityEffect, Combatant,
    CrewConfiguration, EnemyType, EnemyTypes, OpponentFactionTag, ShipType, SimulationConfig,
    TimingWindow, WeaponStats,
};
use kobayashi::data::import::ResearchEntry;
use kobayashi::data::profile::{research_derived_attack_phase_seats, SupportBuffResearchGateState};
use kobayashi::data::research::{
    load_research_canonical_overrides, load_research_catalog, DEFAULT_RESEARCH_CANONICAL_PATH,
    DEFAULT_RESEARCH_CATALOG_PATH,
};

fn repair_crew() -> CrewConfiguration {
    let seats = research_derived_attack_phase_seats(
        &[ResearchEntry {
            rid: 545_866_536,
            level: 3,
        }],
        &load_research_catalog(DEFAULT_RESEARCH_CATALOG_PATH).unwrap(),
        &SupportBuffResearchGateState::default(),
        &load_research_canonical_overrides(DEFAULT_RESEARCH_CANONICAL_PATH),
    );
    assert_eq!(seats.len(), 1);
    assert_eq!(seats[0].ability.timing, TimingWindow::RoundStart);
    assert_eq!(
        seats[0].ability.effect,
        AbilityEffect::HullRegenPrevRoundFraction(0.08)
    );
    CrewConfiguration { seats }
}

fn fight(
    crew: &CrewConfiguration,
    hull: f64,
    defender_type: ShipType,
    pvp: bool,
    tags: EnemyTypes,
) -> f64 {
    let attacker = Combatant {
        attack: 1.0,
        hull_health: 10_000.0,
        ..Default::default()
    };
    let defender = Combatant {
        hull_health: hull,
        weapons: [123.0, 118.0, 117.0]
            .into_iter()
            .map(|attack| WeaponStats {
                attack,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 2,
            engagement_enemy_types: tags,
            ..Default::default()
        },
        crew,
        OpponentFactionTag::Unknown,
        defender_type,
        ShipType::Explorer,
        !pvp,
        pvp,
        &CrewConfiguration::default(),
    )
    .attacker_hull_remaining
}

#[test]
fn repairs_eight_percent_of_all_three_previous_round_hull_hits() {
    let crew = repair_crew();
    let normal = EnemyTypes::default();
    let baseline = fight(
        &CrewConfiguration::default(),
        1_000_000.0,
        ShipType::Battleship,
        false,
        normal.clone(),
    );
    let repaired = fight(&crew, 1_000_000.0, ShipType::Battleship, false, normal);
    assert!((baseline - (10_000.0 - 2.0 * 358.0)).abs() < 1e-9);
    assert!((repaired - baseline - 28.64).abs() < 1e-9);
}

#[test]
fn a_lethal_first_round_does_not_receive_a_later_repair() {
    let crew = repair_crew();
    let baseline = fight(
        &CrewConfiguration::default(),
        1.0,
        ShipType::Battleship,
        false,
        EnemyTypes::default(),
    );
    let repaired = fight(
        &crew,
        1.0,
        ShipType::Battleship,
        false,
        EnemyTypes::default(),
    );
    assert_eq!(repaired, baseline);
}

#[test]
fn repair_rejects_pvp_and_armada_hulls_or_engagement_tags() {
    let crew = repair_crew();
    let cases = [
        (ShipType::Battleship, true, EnemyType::PvpSpace.into()),
        (ShipType::Armada, false, EnemyTypes::default()),
        (ShipType::Battleship, false, EnemyType::GroupArmadas.into()),
        (ShipType::Battleship, false, EnemyType::SoloArmadas.into()),
        (
            ShipType::Battleship,
            false,
            EnemyType::OutpostArmadas.into(),
        ),
    ];
    for (ship_type, pvp, tags) in cases {
        let baseline = fight(
            &CrewConfiguration::default(),
            1_000_000.0,
            ship_type,
            pvp,
            tags.clone(),
        );
        let repaired = fight(&crew, 1_000_000.0, ship_type, pvp, tags);
        assert_eq!(repaired, baseline);
    }
}
