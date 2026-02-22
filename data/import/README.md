# Spreadsheet / CSV import

Place CSV files here and run the corresponding importer.

## Officer roster (your owned officers)

Import which officers you own so the optimizer only suggests crew you have.

- **File:** A `.txt` file (e.g. `data/import/my_roster.txt`).
- **Command:** `kobayashi import my_roster.txt`
- **Format:** One line per officer, comma-separated: `name,tier,level`. Optional first line header `name,tier,level` is ignored.
  - **Name** is required. Apostrophes in names (e.g. D'Vana) are fine. If a name contains a comma, put it in double quotes (e.g. `"Kirk, James",3,45`).
  - **Tier** and **level** are optional. You can write tier as `T3`, `Tier 2`, or `2`; level as `lvl 20`, `LVL20`, or `20`.
  - **Failsafe:** If you give only the name, the importer assumes max tier and max level. If you give name and tier, it assumes max level for that tier.
- **Example:**

  ```text
  name,tier,level
  Kirk,3,45
  Spock,T2,lvl 20
  McCoy
  "Kirk, James",1,30
  ```

- The app writes `data/officers/roster.imported.json`; you do not need to edit that file. For Spocks.club JSON export, use a `.json` file: `kobayashi import export.json`.

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

## Syndicate reputation

- **CSV file:** `data/import/syndicate_reputation.csv` — or pass an **.xlsx path** (e.g. export of the spreadsheet).
- **Command:** `cargo run --bin import_syndicate_reputation` (uses CSV) or `cargo run --bin import_syndicate_reputation -- "path/to/Syndicate Progression.xlsx"`.
- **Output:** `data/syndicate_reputation.json` (and registry updated).

**CSV columns (header row optional):** level, stat, value, operator

- `level`: Syndicate level (1–80+)
- `stat`: stat key (e.g. weapon_damage, armor, shield_hp)
- `value`: numeric value (e.g. 0.05 for +5%)
- `operator`: "add" or "multiply" (default add)

**XLSX:** The importer reads the **Level By Level Comparison** sheet. Headers use carry-forward for merged cells so each column gets a unique label (Section > Subsection > Bracket, e.g. `Officer_Stats_>_Officer_Attack_>_51-60`). Rows are Syndicate levels; player level brackets are 10-19 … 61-70. The mapping from spreadsheet columns to engine stat keys follows the **"Syndicate bonuses Descriptions"** sheet (one column → one stat; no double-counting). Use `cargo run --bin syndicate_bonuses -- <syndicate_level> <ops_level> --combat-only` to print cumulative combat bonuses.

**Source:** [Syndicate Progression (M80)](https://docs.google.com/spreadsheets/d/1A7HOLqeQUANpQyZUTW__Wrq4MGna4Yl-7S28Gq4SOLw/) by USSSilvis et al. — download as .xlsx and pass the file path, or export the sheet to CSV (one row per level per bonus: level, stat, value, operator).
