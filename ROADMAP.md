# Roadmap

Planned features and priorities for Kobayashi.

---

## Ship Abilities

- **Ship ability implementation**  
  Implement ship abilities from the data.stfc.space `ability` array (e.g. "when hit, increase armor piercing / shield piercing / accuracy"). These are distinct from officer abilities and affect combat when the ship takes damage or performs actions. Requires extending the combat engine to evaluate ship-specific effects per round.

---

## Sync (STFC Community Mod)

- **Reception of additional sync types**  
  Extend sync ingress to accept and persist more payload types from the [STFC Community Mod](https://github.com/netniV/stfc-mod). Currently only officer, research, buildings, and ships are persisted; the mod also sends others that are accepted (200) but not stored.

  - **Tech (forbidden tech)** — main priority. Persist `type: "ft"` (mod payload) into a roster or data file so player forbidden-tech state can drive the optimizer or bonus layer.
  - Other candidates (lower priority): traits, slots, buffs, resources, missions, battlelogs, inventory, jobs — as product needs and data shapes are clarified.

See [SYNC.md](SYNC.md) for the current sync protocol and payload reference.
