//! Regenerate docs/OFFICER_MODELING_SCORECARD.md from LCARS + manual fidelity YAML.
//!
//! Run from repo root: `cargo run --bin generate_officer_scorecard`

use std::env;
use std::fs;
use std::path::PathBuf;

use kobayashi::mechanics::coverage::TierCounts;
use kobayashi::mechanics::officer_scorecard::{
    build_officer_scorecard_rows, OfficerScorecardRow, UNMAPPED_TAG_PENALTY_PER_LINE, WEIGHT_BELOW,
    WEIGHT_BRIDGE, WEIGHT_CAPTAIN,
};

fn root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn md_cell(s: &str) -> String {
    s.replace('\n', " ").replace('|', "/")
}

fn fmt_ipc(c: &TierCounts) -> String {
    format!("{}/{}/{}", c.implemented, c.partial, c.ignored)
}

fn opt_i(o: Option<i32>) -> String {
    o.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string())
}

fn rubric() -> String {
    format!(
        r#"# Officer modeling scorecard

This file is **generated**. Do not edit the table by hand. Edit manual notes in `data/officers/officer_modeling_fidelity.yaml`, then run:

```bash
cargo run --bin generate_officer_scorecard
```

## What this measures

- **Auto columns** use the same effect classifier as `/api/mechanics/coverage` ([`lcars_effect_coverage`](../src/lcars/resolver.rs)): Implemented / Partial / Ignored.
- They do **not** detect semantic bugs (wrong target, %-of-SHP modeled as flat HP, missing level caps). Use the **`fidelity`** column for that.

## Combat-intent effects

- Non-`tag` LCARS effects are always combat-intent for this scorecard.
- `type: tag` is combat-intent **unless** the tag string contains `:non_combat` (economy / meta). Tags without that marker (including `:unmapped`) count as **combat gaps**: raw score 0, and they add to **`unmapped_penalty`**.

## Per-effect raw score (0–100)

| Coverage tier | Raw |
|---------------|-----|
| Implemented | 100 |
| Partial | 50 |
| Ignored | 0 |

Combat-intent `tag` lines are always treated as raw **0** for the average (engine skips them in combat).

## Subscores (0–100 integers)

- **`combat_avg`**: arithmetic mean of raw scores over all combat-intent effects. `—` if there are none.
- **`combat_weighted`**: weighted mean — captain ability block **{wc}×**, bridge block **{wb}×**, below decks **{wd}×**. `—` if no combat-intent effects.
- **`unmapped_penalty`**: `min(100, {pen} × unmapped_combat_tags)` where each combat-intent tag (non-`:non_combat`) counts as one line.
- **`combat_auto`**: `clamp(0, 100, combat_weighted - unmapped_penalty)`; `—` if no combat-intent effects.
- **`grade`**: from `combat_auto` — A≥90, B≥80, C≥65, D≥50, F<50.
- **`nc_ack`**: non-combat tag acknowledgment — **100** if there are no tags or all tags are `:non_combat`; **50** if mixed; **0** if any combat-intent tag (no `:non_combat`).
- **`cap_score` / `br_score` / `bd_score`**: mean raw score within that ability block only (`—` if no combat-intent lines in that block).

## Sort order

Rows with at least one combat-intent effect appear first, sorted by **`combat_auto`** ascending (worst first), then **`unmapped_combat_tags`** descending. Officers with **no** combat-intent lines are listed last (sorted by id).

## Column reference

| Column | Meaning |
|--------|---------|
| `cap_I/P/I` | Implemented / Partial / Ignored counts (captain ability block, combat-intent only) |
| `br_I/P/I` | Same for bridge block |
| `bd_I/P/I` | Same for below decks |
| `drop_trig` | Combat-intent effects the LCARS→IR adapter dropped because their `trigger` is unknown |
| `drop_tag` | Same, dropped because the `tag` has no engine-stat mapping (parallels `unmapped_tags`) |
| `drop_stat` | Same, dropped because `stat_modify.stat` has no engine-modifier mapping |
| `drop_cond` | Same, dropped because the `condition` block can't be represented in the canonical IR |

---
"#,
        wc = WEIGHT_CAPTAIN,
        wb = WEIGHT_BRIDGE,
        wd = WEIGHT_BELOW,
        pen = UNMAPPED_TAG_PENALTY_PER_LINE,
    )
}

fn table_header() -> &'static str {
    "| id | name | combat_n | cap_I/P/I | br_I/P/I | bd_I/P/I | unmapped_tags | drop_trig | drop_tag | drop_stat | drop_cond | cap_score | br_score | bd_score | combat_avg | combat_wtd | unmap_pen | combat_auto | grade | nc_ack | nc_label | fidelity |\n|---:|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---|---|\n"
}

fn row_line(r: &OfficerScorecardRow) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
        md_cell(&r.id),
        md_cell(&r.name),
        r.combat_n,
        fmt_ipc(&r.cap_ipc),
        fmt_ipc(&r.br_ipc),
        fmt_ipc(&r.bd_ipc),
        r.unmapped_combat_tags,
        r.dropped_unknown_trigger,
        r.dropped_unmapped_tag,
        r.dropped_unmapped_stat,
        r.dropped_unmapped_condition,
        opt_i(r.cap_score),
        opt_i(r.br_score),
        opt_i(r.bd_score),
        opt_i(r.combat_avg),
        opt_i(r.combat_weighted),
        r.unmapped_penalty,
        opt_i(r.combat_auto),
        md_cell(&r.grade),
        r.nc_ack,
        md_cell(&r.noncombat_label),
        md_cell(&r.fidelity),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let root = root_dir();
    let lcars_dir = root.join("data/officers");
    let fidelity_path = lcars_dir.join("officer_modeling_fidelity.yaml");
    let out_path = root.join("docs/OFFICER_MODELING_SCORECARD.md");

    let rows = build_officer_scorecard_rows(&lcars_dir, &fidelity_path)?;

    let mut body = String::new();
    body.push_str(&rubric());
    body.push_str(table_header());
    for r in &rows {
        body.push_str(&row_line(r));
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, body)?;
    eprintln!("Wrote {} ({} officers)", out_path.display(), rows.len());
    Ok(())
}
