//! Parse STFC game fight export CSV/TSV for comparison with the simulator.
//!
//! Format: tab-separated sections — summary (player/enemy rows), rewards, fleet stats, round events.
//! See docs/combat_log_format.md for column mapping.

use std::collections::HashMap;

use crate::combat::{
    mitigation, pierce_damage_through_bonus, AttackerStats, Combatant, DefenderStats, ShipType,
};

/// Parsed game fight export (multi-section TSV).
#[derive(Debug, Clone)]
pub struct FightExport {
    /// Player won (outcome from summary; player is attacker).
    pub attacker_won: bool,
    /// Max round index from events.
    pub rounds: u32,
    /// Defender (enemy) hull remaining at end (from summary).
    pub defender_hull_remaining: f64,
    /// Defender (enemy) shield remaining at end (from summary).
    pub defender_shield_remaining: f64,
    /// Total damage dealt to defender (sum of Total Damage from attacker→defender events).
    pub total_damage: f64,
    /// Fleet stats for player (attacker). Key = column name, value = cell string.
    pub player_fleet: HashMap<String, String>,
    /// Fleet stats for enemy (defender).
    pub enemy_fleet: HashMap<String, String>,
    /// Per-event records (round, type, hull_damage, shield_damage, total_damage, etc.).
    pub events: Vec<FightExportEvent>,
}

/// Single event row from the events section.
#[derive(Debug, Clone)]
pub struct FightExportEvent {
    pub round: u32,
    pub event_type: String,
    pub hull_damage: f64,
    pub shield_damage: f64,
    pub total_damage: f64,
    pub critical_hit: bool,
}

fn parse_tsv_row(line: &str) -> Vec<String> {
    line.split('\t').map(|s| s.trim().to_string()).collect()
}

fn row_to_map(header: &[String], values: &[String]) -> HashMap<String, String> {
    header
        .iter()
        .zip(values.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn get_f64(map: &HashMap<String, String>, key: &str) -> f64 {
    map.get(key)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Parse a full fight export string (tab-separated, multi-section).
pub fn parse_fight_export(input: &str) -> Result<FightExport, String> {
    let lines: Vec<&str> = input.lines().map(str::trim).filter(|s| !s.is_empty()).collect();
    if lines.is_empty() {
        return Err("empty input".to_string());
    }

    let mut attacker_won = false;
    let mut rounds = 0u32;
    let mut defender_hull_remaining = 0.0;
    let mut defender_shield_remaining = 0.0;
    let mut player_fleet = HashMap::new();
    let mut enemy_fleet = HashMap::new();
    let mut events = Vec::new();
    let mut total_damage = 0.0;

    let mut i = 0;
    while i < lines.len() {
        let row = parse_tsv_row(lines[i]);
        if row.is_empty() {
            i += 1;
            continue;
        }
        let first = row.first().map(String::as_str).unwrap_or("");

        // Summary section: header "Player Name", then player row, then enemy row
        if first == "Player Name" && row.len() > 12 {
            let header = row.clone();
            i += 1;
            if i + 2 > lines.len() {
                return Err("summary section: expected 2 data rows".to_string());
            }
            let player_row = parse_tsv_row(lines[i]);
            let enemy_row = parse_tsv_row(lines[i + 1]);
            let player_map = row_to_map(&header, &player_row);
            let enemy_map = row_to_map(&header, &enemy_row);
            attacker_won = player_map.get("Outcome").map(|s| s.as_str()) == Some("VICTORY");
            defender_hull_remaining =
                enemy_map.get("Hull Health Remaining").and_then(|s| s.parse().ok()).unwrap_or(0.0);
            defender_shield_remaining = enemy_map
                .get("Shield Health Remaining")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            i += 2;
            continue;
        }

        // Fleet section: header "Fleet Type", then "Player Fleet 1", then "Enemy Fleet 1"
        if first == "Fleet Type" && row.len() > 20 {
            let header = row.clone();
            i += 1;
            if i + 2 > lines.len() {
                return Err("fleet section: expected 2 data rows".to_string());
            }
            let row1 = parse_tsv_row(lines[i]);
            let row2 = parse_tsv_row(lines[i + 1]);
            let map1 = row_to_map(&header, &row1);
            let map2 = row_to_map(&header, &row2);
            if map1.get("Fleet Type").map(String::as_str) == Some("Player Fleet 1") {
                player_fleet = map1;
                enemy_fleet = map2;
            } else {
                player_fleet = map2;
                enemy_fleet = map1;
            }
            i += 2;
            continue;
        }

        // Events section: header "Round", then event rows (numeric first column)
        if first == "Round" && row.len() > 18 {
            i += 1;
            while i < lines.len() {
                let event_row = parse_tsv_row(lines[i]);
                if event_row.is_empty() {
                    i += 1;
                    continue;
                }
                let round_str = event_row.get(0).map(String::as_str).unwrap_or("");
                let round: u32 = round_str.parse().unwrap_or(0);
                if round == 0 && round_str != "0" {
                    break; // not a numeric round, next section or end
                }
                rounds = rounds.max(round);
                let event_type = event_row.get(2).cloned().unwrap_or_default();
                let hull_damage = event_row.get(14).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let shield_damage = event_row.get(15).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let total = event_row.get(19).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let critical = event_row.get(13).map(|s| s.eq_ignore_ascii_case("YES")).unwrap_or(false);
                events.push(FightExportEvent {
                    round,
                    event_type,
                    hull_damage,
                    shield_damage,
                    total_damage: total,
                    critical_hit: critical,
                });
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    // Total damage to defender = initial HP - remaining (from summary).
    if !enemy_fleet.is_empty() {
        let def_hull = get_f64(&enemy_fleet, "Hull Health");
        let def_shield = get_f64(&enemy_fleet, "Shield Health");
        total_damage =
            (def_hull - defender_hull_remaining) + (def_shield - defender_shield_remaining);
    }

    Ok(FightExport {
        attacker_won,
        rounds,
        defender_hull_remaining,
        defender_shield_remaining,
        total_damage,
        player_fleet,
        enemy_fleet,
        events,
    })
}

/// Infer ship type from fleet/ship name string.
pub fn ship_type_from_name(name: &str) -> ShipType {
    let n = name.to_uppercase();
    if n.contains("BATTLESHIP") {
        ShipType::Battleship
    } else if n.contains("EXPLORER") {
        ShipType::Explorer
    } else if n.contains("INTERCEPTOR") {
        ShipType::Interceptor
    } else if n.contains("SURVEY") {
        ShipType::Survey
    } else if n.contains("ARMADA") {
        ShipType::Armada
    } else {
        ShipType::Battleship
    }
}

/// Build Combatant for the attacker (player) from export fleet row and defender stats for pierce/mitigation.
pub fn export_to_attacker(
    player_fleet: &HashMap<String, String>,
    defender_stats: DefenderStats,
    defender_ship_type: ShipType,
    id: String,
) -> Combatant {
    let attack = get_f64(player_fleet, "Damage Per Round");
    let armor_piercing = get_f64(player_fleet, "Armour Pierce");
    let shield_piercing = get_f64(player_fleet, "Shield Pierce");
    let accuracy = get_f64(player_fleet, "Accuracy");
    let attacker_stats = AttackerStats {
        armor_piercing,
        shield_piercing,
        accuracy,
    };
    let pierce = pierce_damage_through_bonus(defender_stats, attacker_stats, defender_ship_type);
    Combatant {
        id,
        attack,
        mitigation: 0.0,
        pierce,
        crit_chance: get_f64(player_fleet, "Critical Chance"),
        crit_multiplier: get_f64(player_fleet, "Critical Damage"),
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: get_f64(player_fleet, "Hull Health"),
        shield_health: get_f64(player_fleet, "Shield Health"),
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
    }
}

/// Build Combatant for the defender (enemy) from export fleet row and attacker stats for mitigation.
pub fn export_to_defender(
    enemy_fleet: &HashMap<String, String>,
    attacker_stats: AttackerStats,
    ship_type: ShipType,
    id: String,
) -> Combatant {
    let defender_stats = DefenderStats {
        armor: get_f64(enemy_fleet, "Armour"),
        shield_deflection: get_f64(enemy_fleet, "Shield Deflection"),
        dodge: get_f64(enemy_fleet, "Dodge"),
    };
    let mitigation_val = mitigation(defender_stats, attacker_stats, ship_type);
    Combatant {
        id,
        attack: 0.0,
        mitigation: mitigation_val,
        pierce: 0.0,
        crit_chance: get_f64(enemy_fleet, "Critical Chance"),
        crit_multiplier: get_f64(enemy_fleet, "Critical Damage"),
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: get_f64(enemy_fleet, "Hull Health"),
        shield_health: get_f64(enemy_fleet, "Shield Health"),
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
    }
}

/// Build attacker and defender Combatants from a parsed FightExport.
/// Player = attacker, enemy = defender. Uses defender's ship type for mitigation/pierce.
pub fn export_to_combatants(export: &FightExport) -> (Combatant, Combatant) {
    let attacker_stats = AttackerStats {
        armor_piercing: get_f64(&export.player_fleet, "Armour Pierce"),
        shield_piercing: get_f64(&export.player_fleet, "Shield Pierce"),
        accuracy: get_f64(&export.player_fleet, "Accuracy"),
    };
    let defender_stats = DefenderStats {
        armor: get_f64(&export.enemy_fleet, "Armour"),
        shield_deflection: get_f64(&export.enemy_fleet, "Shield Deflection"),
        dodge: get_f64(&export.enemy_fleet, "Dodge"),
    };
    let defender_ship_type = ShipType::Battleship; // from "HOSTILE BATTLESHIP" in sample
    let attacker = export_to_attacker(
        &export.player_fleet,
        defender_stats,
        defender_ship_type,
        "player".to_string(),
    );
    let defender = export_to_defender(
        &export.enemy_fleet,
        attacker_stats,
        defender_ship_type,
        "hostile".to_string(),
    );
    (attacker, defender)
}
