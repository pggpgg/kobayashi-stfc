# Spreadsheet / CSV import

Place CSV files here and run the corresponding importer.

## Forbidden / Chaos tech

- **File:** `forbidden_chaos_tech.csv`
- **Command:** `cargo run --bin import_forbidden_chaos`
- **Output:** `data/forbidden_chaos_tech.json`

**Columns (header row optional):** name, tech_type, tier, stat, value, operator

- `name`: tech name (e.g. "Ablative Armor")
- `tech_type`: "forbidden" or "chaos"
- `tier`: numeric tier (optional)
- `stat`: stat key (e.g. weapon_damage, armor, shield_hp)
- `value`: numeric value (e.g. 0.15 for +15%)
- `operator`: "add" or "multiply" (default add)

Multiple rows with the same name are merged into one record with multiple bonuses.
