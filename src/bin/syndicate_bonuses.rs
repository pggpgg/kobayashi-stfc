//! Report native Syndicate reward definitions without reading or changing a profile.
//! Usage: syndicate_bonuses <syndicate_level> [ops_level] [--combat-only] [--json]

use kobayashi::data::syndicate_rewards::{native_reward_report, RewardStatus};

const USAGE: &str =
    "Usage: syndicate_bonuses <syndicate_level> [ops_level] [--combat-only] [--json]";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut combat_only = false;
    let mut json = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--combat-only" => combat_only = true,
            "--json" => json = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                println!("Without Ops, values remain unranked. No player profile is read.");
                return Ok(());
            }
            _ if arg.starts_with('-') => {
                return Err(format!("Unknown option {arg}. {USAGE}").into())
            }
            _ => positional.push(arg),
        }
    }
    if positional.is_empty() || positional.len() > 2 {
        return Err(USAGE.into());
    }
    let level = positional[0].parse::<u32>()?;
    let ops = positional.get(1).map(|s| s.parse::<u32>()).transpose()?;
    let report = native_reward_report(level, ops)?;
    if json {
        // JSON always retains every selected definition and its diagnostic status.
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!(
        "Native Syndicate reward definitions: level {}, Ops {}",
        level,
        ops.map_or_else(|| "unknown".to_owned(), |n| n.to_string())
    );
    let coverage = &report.complete_source_coverage;
    println!(
        "Reviewed source: {} tiers, {} reward references, {} resolved BuffIDs; cache {}",
        coverage.tier_count,
        coverage.reward_reference_count,
        coverage.resolved_buff_count,
        report.static_cache_sha256
    );
    println!("\nUnconditional combat definitions at this Ops snapshot:");
    if report.unconditional_combat_bonuses.is_empty() {
        println!("  No ranked unconditional totals available.");
    }
    for (stat, value) in &report.unconditional_combat_bonuses {
        println!("  {stat}: {:.2}%", value * 100.0);
    }
    println!("\nSelected reward provenance and diagnostics:");
    for row in report
        .rows
        .iter()
        .filter(|r| !combat_only || r.status != RewardStatus::Noncombat)
    {
        let source = row
            .source_group
            .map_or_else(|| "unresolved".to_owned(), |g| g.to_string());
        let value = row
            .raw_value
            .map_or_else(|| "unranked".to_owned(), |v| format!("raw={v}"));
        println!(
            "  level {} BID {} source {} {:?} {}",
            row.unlock_level, row.buff_id, source, row.status, value
        );
        if !matches!(
            row.status,
            RewardStatus::UnconditionalCombat | RewardStatus::Noncombat
        ) {
            println!("    {} Conditions: {:?}", row.reason, row.condition_codes);
        }
    }
    println!(
        "\n{} selected references, {} distinct BuffIDs. {}",
        report.selected_reward_references, report.unique_selected_buff_ids, report.limitation
    );
    Ok(())
}
