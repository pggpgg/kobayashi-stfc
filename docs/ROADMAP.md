# Roadmap

Planned features and priorities for Kobayashi.

---

## Sync (STFC Community Mod)

- **Persisted today:** officer, research, buildings, ships, and **forbidden tech (`type: "ft"`)** — see [SYNC.md](SYNC.md). FT is written to `rosters/forbidden_tech.imported.json` and merged into the player profile for the optimizer (bonuses from `data/forbidden_chaos_tech.json`).

- **Optional next sync work** — the mod also sends payload types that are accepted (200) but not stored. Candidates for future persistence (as product needs and data shapes are clarified): traits, slots, buffs, resources, missions, battlelogs, inventory, jobs.

See [SYNC.md](SYNC.md) for the current sync protocol and payload reference.
