# Officer Ingest Pipeline

This pipeline imports `Copy of STFC Cheat Sheet - M86 (1.4RC).xlsx` into a canonical officer dataset.

## Canonical schema

Each officer record in `data/officers/officers.canonical.json` contains:

- `id`: deterministic canonical ID (`slug(name)` + `-` + 6-char SHA1 disambiguator of `source_officer_id`)
- `name`: canonicalized officer display name
- `faction`: normalized from **Sourcing Guide** tab when available
- `group`: officer synergy group from `RawOfficers.OfficerSynergy`
- `rarity`: rarity normalized to lowercase
- `slot`: badge type/slot from **Sourcing Guide** (command/science/engineering)
- `abilities`: ability list (captain/officer slot, modifier, trigger, target, operation, conditions, attributes, rank scaling)
- `scaling`: schema-level notes for rank/tier scaling fields
- `source`: source metadata for provenance
- `source_officer_id`: stable source-side ID from the spreadsheet (`AT` column)

Top-level payload metadata:

- `data_version`: deterministic version key derived from source workbook SHA256
- `imported_at`: UTC timestamp of import run
- `source_fingerprint`: source file name and SHA256 hash
- `schema_version`: integer schema version

## Alias normalization map

`data/officers/name_aliases.json` stores uppercase source names as keys and canonical display names as values. The importer:

1. uses pre-existing aliases first,
2. then falls back to the `Sourcing Guide` name,
3. then title-cases the raw value,
4. and persists discovered aliases to avoid future casing/spelling churn.

## ID freeze rules

- ID creation is deterministic: `slug(name) + '-' + sha1(source_officer_id)[:6]`.
- `data/officers/id_registry.json` freezes the mapping from source officer IDs to canonical IDs.
- If an officer's display name changes later, the canonical ID remains stable.

## Validation and report

The importer fails if any record has:

- missing required fields,
- duplicate canonical IDs,
- or no ability rows.

`data/officers/import_report.json` contains summary counts (`new`, `updated`, `unchanged`, `invalid`) and invalid-row details.

## Run

```bash
python tools/officer_ingest/import_officers.py
```
