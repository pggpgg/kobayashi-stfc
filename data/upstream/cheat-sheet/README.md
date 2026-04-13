# STFC officers cheat sheet (upstream CSV)

These files are exports of a **community-curated “cheat sheet”** spreadsheet for *Star Trek Fleet Command*. The sheet uses a **standardized vocabulary** to describe officer abilities and **which hostile / enemy types** each effect applies to.

When used carefully as a reference (not as automatic ground truth for in-game behavior), they can **speed up officer ability modeling** and improve **simulator accuracy** by surfacing intended wording, conditions, and scope before those mechanics are encoded in LCARS or code.

## Files in this directory


| File                            | Role                                                                                                                                                                                          |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `raw-officers-m88-17rc.csv`     | **Source / machine-oriented export** — complete data in the form most suitable for scripts, diffing, and automated ingestion. Treat this as the primary tabular source when building tooling. |
| `master-officers-m88-17rc.csv`  | **Complete, human-oriented export** — same coverage as raw, laid out for reading and manual review.                                                                                           |
| `compact-officers-m88-17rc.csv` | **Truncated summary** — very short; useful for quick scans, not for full modeling.                                                                                                            |


Version suffixes in the filenames (e.g. `m88-17rc`) refer to the spreadsheet **milestone / release** the export was taken from; replace or add files when the community sheet updates.

## Relationship to Kobayashi

Officer combat definitions in this repo live in LCARS (`data/officers/officers.lcars.yaml`, generated from the canonical catalog). These CSVs are **upstream reference material**; they do not drive the simulator until values are validated and translated into LCARS (or documented assumptions).