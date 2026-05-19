//! Ingestion helpers for stfc.cc–style cheat-sheet columns into [`CombatEffectSpec`].
//! Column names match `data/upstream/cheat-sheet/raw-officers-m88-17rc.csv`. This is **optional**
//! tooling — the engine still uses LCARS + research catalogs at runtime.

use std::collections::HashMap;

use crate::data::combat_effect_spec::{
    AbilityConditionSpec, AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec,
    AbilityTriggerSpec, CombatEffectSpec, EffectCategory, EffectSource, ValueSpec,
};

/// Upstream cheat-sheet tokens with no resolved [`AbilityConditionSpec`] / engine mapping yet;
/// ingested as [`AbilityConditionSpec::StfcCcToken`]. See `docs/CANONICAL_CONDITIONS.md` deferred list.
const STFC_CC_DEFERRED_CONDITION_TOKENS: &[&str] = &[
    "CombatBattleType",
    "HullHealthAbove",
    "HullHealthBelow",
    "HullHealthBelowStartOfCombat",
    "TargetMaxLevel",
];

/// Stable diagnostic strings (prefix `unmapped_*:`) for rows that cannot be fully converted.
pub type StfcCcDiagnostics = Vec<String>;

fn get_csv_field<'a>(
    headers: &'a csv::StringRecord,
    record: &'a csv::StringRecord,
    name: &str,
) -> Option<&'a str> {
    let idx = headers.iter().position(|h| h == name)?;
    record.get(idx).map(str::trim).filter(|s| !s.is_empty())
}

/// Map cheat-sheet `AbilityModifier` cell → canonical modifier.
///
/// Aligns with [`crate::bin::generate_lcars::map_modifier`] where there is a single primary stat.
/// `AllDefenses` maps to [`AbilityModifierSpec::MitigationAdditive`] (the “add” branch); `MultiplySub` rows are
/// still folded to [`AbilityModifierSpec::Add`] via [`map_stfc_cc_operation`], so mitigation-specific
/// sign handling is lossy here. Composite `OfficerStatAll` maps to [`AbilityModifierSpec::TagOnly`]
/// (same bucket as `generate_lcars` tag / non-single-stat modifiers).
pub fn map_stfc_cc_modifier(raw: &str) -> Result<AbilityModifierSpec, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("unmapped_modifier:<empty>".into());
    }
    match s {
        "Accuracy" => Ok(AbilityModifierSpec::Accuracy),
        "WeaponDamage" | "OfficerWeaponDamage" => Ok(AbilityModifierSpec::WeaponDamage),
        "AllDamage" | "OfficerStatAttack" => Ok(AbilityModifierSpec::WeaponDamage),
        "CritChance" => Ok(AbilityModifierSpec::CritChance),
        "CritDamage" => Ok(AbilityModifierSpec::CritDamage),
        "Pierce" | "ArmorPierce" | "ArmorPiercing" | "ShieldPierce" | "ShieldPiercing"
        | "AllPiercing" => Ok(AbilityModifierSpec::Pierce),
        "ShieldMitigation" => Ok(AbilityModifierSpec::ShieldMitigation),
        "Armor" | "ShipArmor" | "OfficerStatDefense" | "AllDefenses" => {
            Ok(AbilityModifierSpec::MitigationAdditive)
        }
        "Dodge" | "ShipDodge" => Ok(AbilityModifierSpec::Dodge),
        "HullHP" | "HullHealth" | "HullHPRepair" | "HullRegen" | "OfficerStatHealth" => {
            Ok(AbilityModifierSpec::HullHp)
        }
        "HullHPMax" => Ok(AbilityModifierSpec::HullHp),
        "ShieldHP" | "ShieldHealth" | "ShieldHPRepair" | "ShieldRegen" => {
            Ok(AbilityModifierSpec::ShieldHp)
        }
        "ShieldHPMax" => Ok(AbilityModifierSpec::ShieldHp),
        "ShotsPerAttack" => Ok(AbilityModifierSpec::ShotsBonus),
        "IsolyticDamage" => Ok(AbilityModifierSpec::IsolyticDamage),
        "IsolyticDefense" => Ok(AbilityModifierSpec::IsolyticDefense),
        "IsolyticCascade" | "IsolyticCascadeDamage" => {
            Ok(AbilityModifierSpec::IsolyticCascadeDamage)
        }
        "ApexShred" => Ok(AbilityModifierSpec::ApexShred),
        "ApexBarrier" => Ok(AbilityModifierSpec::ApexBarrier),
        // State / proc / loot / economy rows: represented in IR for coverage; runtime LCARS uses tags.
        "AddState" | "AddRandomState" => Ok(AbilityModifierSpec::TagOnly),
        "MiningRate"
        | "MiningReward"
        | "CargoCapacity"
        | "CargoProtection"
        | "WarpSpeed"
        | "ImpulseSpeed"
        | "WarpDistance"
        | "JumpAndTowCostEff"
        | "RepairCostsPost"
        | "RepairTime"
        | "HullRepair"
        | "CombatXPReward"
        | "CombatPveRewards"
        | "FactionPointsGain"
        | "AllReloadSpeed"
        | "AllLoadSpeed"
        | "HostileLoot"
        | "ArmadaLoot"
        | "OffAbilityEffect"
        | "CptManeuverEffect"
        | "CombatParsteelReward"
        | "CombatTritaniumReward"
        | "CombatDilithiumReward"
        | "Shields"
        | "SkillCloakingDuration"
        | "SkillCloakingCooldown"
        | "ActianVenomAndNanoprobeLoot"
        | "BrokenShipPartsLoot"
        | "ArtifactTokenLoot"
        | "VoyagerAsaCE"
        | "HirogenRelicAndBiotoxinLoot"
        | "XindiHostileLoot"
        | "SkillCuttingBeamAbilityCost"
        | "GornHostileVolatileLoot"
        | "TrelliumRewards"
        | "PveChestLootMultiplierLimitedResources"
        | "WokAugmentAllLootRewards"
        | "Omega13Cooldown"
        | "SkillCuttingBeamPvPBaseDamagePercentage"
        | "CombatScavenger"
        | "OfficerStatAll" => Ok(AbilityModifierSpec::TagOnly),
        _ => Err(format!("unmapped_modifier:{s}")),
    }
}

/// Map cheat-sheet `AbilityTrigger` cell.
pub fn map_stfc_cc_trigger(raw: &str) -> Result<AbilityTriggerSpec, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("unmapped_trigger:<empty>".into());
    }
    match s {
        "CombatStart" | "OnCombatStart" => Ok(AbilityTriggerSpec::CombatBegin),
        "ShipLaunched" | "ShipLaunch" => Ok(AbilityTriggerSpec::ShipLaunched),
        "RoundStart" | "OnRoundStart" => Ok(AbilityTriggerSpec::RoundStart),
        "RoundEnd" | "OnRoundEnd" => Ok(AbilityTriggerSpec::RoundEnd),
        "OnAttack" | "AttackPhase" | "CriticalShotFired" | "EnemyTakesHit" => {
            Ok(AbilityTriggerSpec::AttackPhase)
        }
        "AfterShot" | "OnAfterShot" | "AfterWeapon" => Ok(AbilityTriggerSpec::AfterSubround),
        "OnDefense" | "HitTaken" => Ok(AbilityTriggerSpec::DefensePhase),
        "OnKill" | "BattleWon" => Ok(AbilityTriggerSpec::Kill),
        "OnHullBreach" | "HullDamageTaken" => Ok(AbilityTriggerSpec::HullBreach),
        "OnReceiveDamage" | "ShieldDamageTaken" => Ok(AbilityTriggerSpec::ReceiveDamage),
        "OnCombatEnd" => Ok(AbilityTriggerSpec::CombatEnd),
        "OnShieldBreak" | "OnEnemyShieldBreak" | "ShieldsDepleted" => {
            Ok(AbilityTriggerSpec::ShieldBreak)
        }
        "OnOwnShieldBreak" => Ok(AbilityTriggerSpec::SelfShieldBreak),
        "CriticalShotTaken" => Ok(AbilityTriggerSpec::DefensePhase),
        "ShipRecalled" => Ok(AbilityTriggerSpec::CombatEnd),
        "TargetShieldsDepleted" => Ok(AbilityTriggerSpec::ShieldBreak),
        _ => Err(format!("unmapped_trigger:{s}")),
    }
}

/// Map cheat-sheet `AbilityTarget` cell.
pub fn map_stfc_cc_target(raw: &str) -> Result<AbilityTargetSpec, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("unmapped_target:<empty>".into());
    }
    match s {
        "SelfShip" | "SelfAll" => Ok(AbilityTargetSpec::SelfShip),
        // SelfBridge: officers on the attacking ship’s bridge — map to attacker self.
        "SelfBridge" => Ok(AbilityTargetSpec::AttackerSelf),
        "EnemyShip" | "TargetShip" => Ok(AbilityTargetSpec::EnemyShip),
        "AttackerSelf" => Ok(AbilityTargetSpec::AttackerSelf),
        "DefenderOpponent" => Ok(AbilityTargetSpec::DefenderOpponent),
        // AoE / seat shorthand from cheat-sheet columns.
        "EnemyAllShips" | "EnemyAll" => Ok(AbilityTargetSpec::DefenderTeam),
        "SelfCaptain" => Ok(AbilityTargetSpec::AttackerSelf),
        "EnemyBridge" => Ok(AbilityTargetSpec::EnemyBridgeOfficers),
        "EnemyCaptain" => Ok(AbilityTargetSpec::EnemyShip),
        "SelfAllShips" => Ok(AbilityTargetSpec::AttackerTeam),
        "SelfOfficer" => Ok(AbilityTargetSpec::AttackerSelf),
        _ => Err(format!("unmapped_target:{s}")),
    }
}

/// Map cheat-sheet `AbilityOperation` cell.
pub fn map_stfc_cc_operation(raw: &str) -> Result<AbilityOperationSpec, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("unmapped_operation:<empty>".into());
    }
    match s {
        "Add" | "MultiplyAdd" | "MultiplyBaseAdd" => Ok(AbilityOperationSpec::Add),
        // MultiplySub / Sub: cheat-sheet naming; fold to Add for canonical IR (values still raw).
        "MultiplySub" | "MultiplyBaseSub" | "Sub" => Ok(AbilityOperationSpec::Add),
        "Multiply" | "Mul" => Ok(AbilityOperationSpec::Multiply),
        "Set" => Ok(AbilityOperationSpec::Set),
        "Min" => Ok(AbilityOperationSpec::Min),
        "Max" => Ok(AbilityOperationSpec::Max),
        _ => Err(format!("unmapped_operation:{s}")),
    }
}

/// Parse `faction_id=…` from cheat-sheet `AbilityAttributes` (comma-separated `key=value` segments).
fn faction_id_from_ability_attributes(attrs: &str) -> Option<i64> {
    for segment in attrs.split(',') {
        let s = segment.trim();
        if let Some(rest) = s.strip_prefix("faction_id=") {
            return rest.trim().parse::<i64>().ok();
        }
    }
    None
}

/// Merge parsed `AbilityAttributes` key/value pairs into [`CombatEffectSpec::attributes`] under `stfc_cc_ability_attributes`.
fn merge_stfc_cc_ability_attributes(
    raw: &str,
    out: &mut serde_json::Map<String, serde_json::Value>,
) {
    let raw = raw.trim();
    if raw.is_empty() {
        return;
    }
    let mut obj = serde_json::Map::new();
    for segment in raw.split(',') {
        let s = segment.trim();
        if s.is_empty() {
            continue;
        }
        let Some((k, v)) = s.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        let v = v.trim();
        let val = if v.starts_with('[') || v.starts_with('{') {
            serde_json::from_str::<serde_json::Value>(v)
                .unwrap_or_else(|_| serde_json::Value::String(v.to_string()))
        } else if let Ok(n) = v.parse::<i64>() {
            serde_json::json!(n)
        } else if let Ok(f) = v.parse::<f64>() {
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::Value::String(v.to_string()))
        } else {
            serde_json::Value::String(v.to_string())
        };
        obj.insert(k.to_string(), val);
    }
    if !obj.is_empty() {
        out.insert(
            "stfc_cc_ability_attributes".to_string(),
            serde_json::Value::Object(obj),
        );
    }
}

fn map_condition_token(
    token: &str,
    ability_attributes: Option<&str>,
) -> Result<AbilityConditionSpec, String> {
    let t = token.trim();
    if t.is_empty() {
        return Err("unmapped_condition:<empty>".into());
    }
    match t {
        "SelfHasMorale" | "AttackerMorale" | "Morale" => Ok(AbilityConditionSpec::MoraleActive),
        "TargetBurning" | "DefenderBurning" | "Burning" | "TargetHasBurning" => {
            Ok(AbilityConditionSpec::DefenderBurning)
        }
        "TargetHullBreach" | "DefenderHullBreach" | "TargetHasHullBreach" => {
            Ok(AbilityConditionSpec::DefenderHullBreach)
        }
        "SelfBurning" | "AttackerBurning" | "SelfHasBurning" => {
            Ok(AbilityConditionSpec::AttackerBurning)
        }
        "SelfHullBreach" | "AttackerHullBreach" | "SelfHasHullBreach" => {
            Ok(AbilityConditionSpec::AttackerHullBreach)
        }
        "TargetAssimilated" | "DefenderAssimilated" | "TargetHasAssimilated" => {
            Ok(AbilityConditionSpec::DefenderAssimilated)
        }
        "SelfExplorer" => Ok(AbilityConditionSpec::AttackerShipTypeIs {
            ship_type: "explorer".into(),
        }),
        "SelfBattleship" => Ok(AbilityConditionSpec::AttackerShipTypeIs {
            ship_type: "battleship".into(),
        }),
        "SelfInterceptor" => Ok(AbilityConditionSpec::AttackerShipTypeIs {
            ship_type: "interceptor".into(),
        }),
        "SelfSurveyor" => Ok(AbilityConditionSpec::AttackerShipTypeIs {
            ship_type: "survey".into(),
        }),
        "EnemyExplorer" => Ok(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: "explorer".into(),
        }),
        "EnemyBattleship" => Ok(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: "battleship".into(),
        }),
        "EnemyInterceptor" => Ok(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: "interceptor".into(),
        }),
        "EnemySurvey" => Ok(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: "survey".into(),
        }),
        "TargetIsArmada" => Ok(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: "armada".into(),
        }),
        "DefenderNpcHostile" | "EnemyHostile" => Ok(AbilityConditionSpec::DefenderIsNpcHostile),
        "DefenderPlayerShip" | "EnemyPlayer" => Ok(AbilityConditionSpec::DefenderIsPlayerShip),
        "SelfOfficerTalNotOnBridge" => Ok(AbilityConditionSpec::AttackerOfficerTalNotOnBridge),
        "SelfHullVoyager" => Ok(AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "uss_voyager".into(),
        }),
        "SelfHullDiscovery" => Ok(AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "uss_discovery".into(),
        }),
        "SelfHullBorgCube" => Ok(AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "borg_cube".into(),
        }),
        "SelfHullNseaProtector" => Ok(AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "nsea_protector".into(),
        }),
        "SelfHullAmalgam" => Ok(AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "amalgam".into(),
        }),
        "SelfHullJunker" => Ok(AbilityConditionSpec::AttackerShipIdIs {
            ship_id: "gs_31".into(),
        }),
        "SelfHullFranklins" => Ok(AbilityConditionSpec::Or {
            any: vec![
                AbilityConditionSpec::AttackerShipIdIs {
                    ship_id: "uss_franklin".into(),
                },
                AbilityConditionSpec::AttackerShipIdIs {
                    ship_id: "uss_franklin_a".into(),
                },
            ],
        }),
        "TargetNotArmada" => Ok(AbilityConditionSpec::Not {
            inner: Box::new(AbilityConditionSpec::DefenderShipTypeIs {
                ship_type: "armada".into(),
            }),
        }),
        "TargetNotASB" | "SelfAttacking" | "TargetNotPlayerStation" => {
            Ok(AbilityConditionSpec::LiteralBool { value: true })
        }
        "SelfDefending" => Ok(AbilityConditionSpec::LiteralBool { value: false }),
        "SelfAtSoloArmada" => Ok(AbilityConditionSpec::EngagementIncludes {
            enemy_type: "solo_armadas".into(),
        }),
        "TargetNotSoloArmada" => Ok(AbilityConditionSpec::EngagementIncludes {
            enemy_type: "group_armadas".into(),
        }),
        "EnemySentinel"
        | "CombatGameContext"
        | "SelfAtStation"
        | "SelfAtWaveDefenseChallenge"
        | "SelfAtAssault2"
        | "TargetIsInvadingEntity" => Ok(AbilityConditionSpec::LiteralBool { value: false }),
        "TargetIsArmadaOrInvadingEntity" => Ok(AbilityConditionSpec::DefenderShipTypeIs {
            ship_type: "armada".into(),
        }),
        "TargetNotInvadingEntity" => Ok(AbilityConditionSpec::LiteralBool { value: true }),
        "ModuleKinetic" | "ModuleEnergy" => Ok(AbilityConditionSpec::LiteralBool { value: true }),
        "TargetStateAny" => Ok(AbilityConditionSpec::Or {
            any: vec![
                AbilityConditionSpec::DefenderBurning,
                AbilityConditionSpec::DefenderHullBreach,
                AbilityConditionSpec::DefenderAssimilated,
            ],
        }),
        "SelfStateNone" => Ok(AbilityConditionSpec::Not {
            inner: Box::new(AbilityConditionSpec::Or {
                any: vec![
                    AbilityConditionSpec::AttackerBurning,
                    AbilityConditionSpec::AttackerHullBreach,
                ],
            }),
        }),
        "SelfCloaked" | "SelfMining" => Ok(AbilityConditionSpec::LiteralBool { value: false }),
        "CargoEmpty" | "EnemyNotToaTrialHostile" => {
            Ok(AbilityConditionSpec::LiteralBool { value: true })
        }
        "CargoFull" | "EnemyStronger" | "HitEnemyWithEnergy" | "HitEnemyWithKinetic" => {
            Ok(AbilityConditionSpec::LiteralBool { value: false })
        }
        "EnemyHullFaction" => {
            let Some(attrs) = ability_attributes.map(str::trim).filter(|s| !s.is_empty()) else {
                return Err("unmapped_condition:EnemyHullFaction".into());
            };
            let id = faction_id_from_ability_attributes(attrs)
                .ok_or_else(|| "unmapped_condition:EnemyHullFaction".to_string())?;
            Ok(AbilityConditionSpec::DefenderHullFactionIdIs { faction_id: id })
        }
        _ => {
            if STFC_CC_DEFERRED_CONDITION_TOKENS.contains(&t) {
                Ok(AbilityConditionSpec::StfcCcToken {
                    token: t.to_string(),
                })
            } else {
                Err(format!("unmapped_condition:{t}"))
            }
        }
    }
}

/// Parse comma-separated `AbilityConditions` cells into condition specs. Empty string → `Ok(vec![])`.
///
/// When the cell includes `EnemyHullFaction`, pass the `AbilityAttributes` column so
/// `faction_id=…` can be resolved to `DefenderHullFactionIdIs`.
pub fn parse_stfc_cc_conditions(
    raw: &str,
    ability_attributes: Option<&str>,
) -> Result<Vec<AbilityConditionSpec>, StfcCcDiagnostics> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut errs = StfcCcDiagnostics::new();
    for part in raw.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        match map_condition_token(p, ability_attributes) {
            Ok(c) => out.push(c),
            Err(e) => errs.push(e),
        }
    }
    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

fn parse_value_1(record: &csv::StringRecord, headers: &csv::StringRecord) -> Option<f64> {
    let v = get_csv_field(headers, record, "AbilityValue_1")?;
    v.parse::<f64>().ok()
}

/// Convert one CSV row into a [`CombatEffectSpec`] when every mapped field resolves.
pub fn try_stfc_cc_string_record_to_spec(
    record: &csv::StringRecord,
    headers: &csv::StringRecord,
) -> Result<CombatEffectSpec, StfcCcDiagnostics> {
    let mut diags = StfcCcDiagnostics::new();

    let officer = get_csv_field(headers, record, "OfficerName").unwrap_or("?");
    let ability_type = get_csv_field(headers, record, "AbilityType").unwrap_or("?");
    let ability_id = get_csv_field(headers, record, "AbilityID").unwrap_or("unknown");

    let modifier_s = get_csv_field(headers, record, "AbilityModifier").unwrap_or("");
    let trigger_s = get_csv_field(headers, record, "AbilityTrigger").unwrap_or("");
    let target_s = get_csv_field(headers, record, "AbilityTarget").unwrap_or("");
    let operation_s = get_csv_field(headers, record, "AbilityOperation").unwrap_or("");
    let conditions_s = get_csv_field(headers, record, "AbilityConditions").unwrap_or("");

    let modifier_r = map_stfc_cc_modifier(modifier_s);
    let trigger_r = map_stfc_cc_trigger(trigger_s);
    let target_r = map_stfc_cc_target(target_s);
    let operation_r = map_stfc_cc_operation(operation_s);

    let (modifier, trigger, target, operation) =
        match (modifier_r, trigger_r, target_r, operation_r) {
            (Ok(m), Ok(t), Ok(ta), Ok(o)) => (m, t, ta, o),
            (a, b, c, d) => {
                if let Err(e) = a {
                    diags.push(e);
                }
                if let Err(e) = b {
                    diags.push(e);
                }
                if let Err(e) = c {
                    diags.push(e);
                }
                if let Err(e) = d {
                    diags.push(e);
                }
                return Err(diags);
            }
        };

    let attrs_for_conditions = get_csv_field(headers, record, "AbilityAttributes");
    let conditions = match parse_stfc_cc_conditions(conditions_s, attrs_for_conditions) {
        Ok(c) => c,
        Err(mut e) => {
            diags.append(&mut e);
            return Err(diags);
        }
    };

    let value = parse_value_1(record, headers).map(|scalar| ValueSpec {
        scalar: Some(scalar),
        by_rank: None,
        unit: None,
        officer_stat_scaling: None,
    });

    let mut attributes = serde_json::Map::new();
    if let Some(attrs) = attrs_for_conditions {
        merge_stfc_cc_ability_attributes(attrs, &mut attributes);
    }

    Ok(CombatEffectSpec {
        id: format!("stfc_cc:{officer}:{ability_type}:{ability_id}"),
        source: EffectSource::StfcCcCheatSheet,
        source_ref: None,
        text: None,
        trigger,
        target,
        modifier,
        operation,
        value,
        chance: None,
        duration: None,
        decay: None,
        accumulate: None,
        conditions,
        attributes,
        stacking: None,
        category: Some(EffectCategory::Combat),
        confidence: None,
    })
}

/// Scan an entire cheat-sheet CSV: counts full conversions vs diagnostic keys.
pub fn scan_stfc_cc_cheat_sheet_csv<R: std::io::Read>(
    reader: R,
) -> Result<StfcCcScanSummary, csv::Error> {
    let mut rdr = csv::Reader::from_reader(reader);
    let headers = rdr.headers()?.clone();
    let mut rows_total = 0usize;
    let mut rows_full_convert = 0usize;
    let mut diagnostic_counts: HashMap<String, usize> = HashMap::new();

    for rec in rdr.records() {
        let rec = rec?;
        rows_total += 1;
        match try_stfc_cc_string_record_to_spec(&rec, &headers) {
            Ok(_) => rows_full_convert += 1,
            Err(diags) => {
                for d in diags {
                    *diagnostic_counts.entry(d).or_insert(0) += 1;
                }
            }
        }
    }

    Ok(StfcCcScanSummary {
        rows_total,
        rows_full_convert,
        diagnostic_counts,
    })
}

#[derive(Debug, Clone, Default)]
pub struct StfcCcScanSummary {
    pub rows_total: usize,
    pub rows_full_convert: usize,
    pub diagnostic_counts: HashMap<String, usize>,
}

impl StfcCcScanSummary {
    /// Top N diagnostic keys by frequency (for CLI / reports).
    pub fn top_diagnostics(&self, n: usize) -> Vec<(String, usize)> {
        let mut v: Vec<_> = self
            .diagnostic_counts
            .iter()
            .map(|(k, c)| (k.clone(), *c))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csv::StringRecord;

    fn headers_sample() -> csv::StringRecord {
        csv::StringRecord::from(vec![
            "OfficerName",
            "AbilityType",
            "AbilityModifier",
            "AbilityConditions",
            "AbilityTrigger",
            "AbilityTarget",
            "AbilityOperation",
            "AbilityID",
            "AbilityValue_1",
        ])
    }

    fn headers_with_attributes() -> csv::StringRecord {
        csv::StringRecord::from(vec![
            "OfficerName",
            "AbilityType",
            "AbilityModifier",
            "AbilityConditions",
            "AbilityTrigger",
            "AbilityTarget",
            "AbilityOperation",
            "AbilityID",
            "AbilityValue_1",
            "AbilityAttributes",
        ])
    }

    #[test]
    fn spock_accuracy_row_converts() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "SPOCK",
            "OA",
            "Accuracy",
            "",
            "RoundStart",
            "SelfShip",
            "MultiplyAdd",
            "869555258",
            "0.15",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.modifier, AbilityModifierSpec::Accuracy);
        assert_eq!(spec.trigger, AbilityTriggerSpec::RoundStart);
        assert_eq!(spec.target, AbilityTargetSpec::SelfShip);
        assert_eq!(spec.operation, AbilityOperationSpec::Add);
        assert!((spec.value.as_ref().and_then(|v| v.scalar).unwrap() - 0.15).abs() < 1e-12);
    }

    #[test]
    fn all_damage_maps_to_weapon_damage() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "AllDamage",
            "",
            "ShipLaunched",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.12",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.modifier, AbilityModifierSpec::WeaponDamage);
    }

    #[test]
    fn add_state_maps_to_tag_only() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "AddState",
            "",
            "RoundStart",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.5",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.modifier, AbilityModifierSpec::TagOnly);
    }

    #[test]
    fn kirk_officer_stat_all_maps_to_tag_only() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "KIRK",
            "CM",
            "OfficerStatAll",
            "SelfHasMorale",
            "RoundStart",
            "SelfAll",
            "MultiplyAdd",
            "4102716881",
            "0.4",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.modifier, AbilityModifierSpec::TagOnly);
        assert_eq!(spec.conditions.len(), 1);
        assert!(matches!(
            spec.conditions[0],
            AbilityConditionSpec::MoraleActive
        ));
    }

    #[test]
    fn self_has_morale_condition_parses() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "Accuracy",
            "SelfHasMorale",
            "RoundStart",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.conditions.len(), 1);
        assert!(matches!(
            spec.conditions[0],
            AbilityConditionSpec::MoraleActive
        ));
    }

    #[test]
    fn enemy_hull_faction_maps_with_ability_attributes_faction_id() {
        let h = headers_with_attributes();
        let rec = StringRecord::from(vec![
            "SESHA",
            "OA",
            "WeaponDamage",
            "EnemyHullFaction",
            "AttackPhase",
            "SelfShip",
            "MultiplyAdd",
            "123",
            "0.05",
            "faction_id=1750120904",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.conditions.len(), 1);
        match &spec.conditions[0] {
            AbilityConditionSpec::DefenderHullFactionIdIs { faction_id } => {
                assert_eq!(*faction_id, 1_750_120_904);
            }
            _ => panic!("expected DefenderHullFactionIdIs"),
        }
        let attrs = spec
            .attributes
            .get("stfc_cc_ability_attributes")
            .and_then(|v| v.as_object())
            .expect("merged ability attributes");
        assert_eq!(
            attrs.get("faction_id").and_then(|v| v.as_i64()),
            Some(1_750_120_904)
        );
    }

    #[test]
    fn target_not_asb_maps_to_literal_true_with_merged_attributes() {
        let h = headers_with_attributes();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "AllReloadSpeed",
            "TargetNotArmada, TargetNotASB, TargetHasHullBreach",
            "CriticalShotFired",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.1",
            "num_rounds=1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert!(
            spec.conditions
                .iter()
                .any(|c| { matches!(c, AbilityConditionSpec::LiteralBool { value: true }) }),
            "expected TargetNotASB → literal_bool true, got {:?}",
            spec.conditions
        );
        let n = spec
            .attributes
            .get("stfc_cc_ability_attributes")
            .and_then(|v| v.get("num_rounds"))
            .and_then(|v| v.as_i64());
        assert_eq!(n, Some(1));
    }

    #[test]
    fn enemy_hull_faction_without_attributes_column_fails() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "SESHA",
            "OA",
            "WeaponDamage",
            "EnemyHullFaction",
            "AttackPhase",
            "SelfShip",
            "MultiplyAdd",
            "123",
            "0.05",
        ]);
        let err = try_stfc_cc_string_record_to_spec(&rec, &h).unwrap_err();
        assert!(err.iter().any(|e| e.contains("EnemyHullFaction")));
    }

    #[test]
    fn self_officer_tal_not_on_bridge_condition_parses() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "Accuracy",
            "SelfOfficerTalNotOnBridge",
            "RoundStart",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.conditions.len(), 1);
        assert!(matches!(
            spec.conditions[0],
            AbilityConditionSpec::AttackerOfficerTalNotOnBridge
        ));
    }

    #[test]
    fn self_hull_voyager_condition_parses_to_attacker_ship_id() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "CargoCapacity",
            "SelfHullVoyager",
            "ShipLaunched",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.conditions.len(), 1);
        assert!(matches!(
            spec.conditions[0],
            AbilityConditionSpec::AttackerShipIdIs { ref ship_id } if ship_id == "uss_voyager"
        ));
    }

    #[test]
    fn self_explorer_and_target_has_burning_parse() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "Accuracy",
            "SelfExplorer,TargetHasBurning",
            "RoundStart",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.conditions.len(), 2);
        assert!(matches!(
            &spec.conditions[0],
            AbilityConditionSpec::AttackerShipTypeIs { ship_type } if ship_type == "explorer"
        ));
        assert!(matches!(
            spec.conditions[1],
            AbilityConditionSpec::DefenderBurning
        ));
    }

    #[test]
    fn target_not_armada_condition_parses() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "Accuracy",
            "TargetNotArmada",
            "RoundStart",
            "SelfShip",
            "MultiplyAdd",
            "1",
            "0.1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.conditions.len(), 1);
        match &spec.conditions[0] {
            AbilityConditionSpec::Not { inner } => match inner.as_ref() {
                AbilityConditionSpec::DefenderShipTypeIs { ship_type } => {
                    assert_eq!(ship_type, "armada")
                }
                _ => panic!("expected DefenderShipTypeIs inside Not"),
            },
            _ => panic!("expected Not"),
        }
    }

    #[test]
    fn self_bridge_maps_to_attacker_self() {
        let h = headers_sample();
        let rec = StringRecord::from(vec![
            "X",
            "OA",
            "Accuracy",
            "",
            "RoundStart",
            "SelfBridge",
            "MultiplyAdd",
            "1",
            "0.1",
        ]);
        let spec = try_stfc_cc_string_record_to_spec(&rec, &h).expect("spec");
        assert_eq!(spec.target, AbilityTargetSpec::AttackerSelf);
    }

    #[test]
    fn scan_bundled_cheat_sheet_csv_runs() {
        let f = std::fs::File::open("data/upstream/cheat-sheet/raw-officers-m88-17rc.csv")
            .expect("open bundled cheat-sheet (run tests from crate root)");
        let s = scan_stfc_cc_cheat_sheet_csv(f).expect("scan");
        assert!(s.rows_total > 100);
        assert_eq!(
            s.rows_full_convert,
            s.rows_total,
            "expected every bundled cheat-sheet row to convert; diagnostics: {:?}",
            s.top_diagnostics(15)
        );
    }
}
