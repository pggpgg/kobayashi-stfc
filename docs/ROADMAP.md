# Roadmap

Planned features and priorities for Kobayashi.

---

## Sync (STFC Community Mod)

- **Reception of additional sync types**  
  Extend sync ingress to accept and persist more payload types from the [STFC Community Mod](https://github.com/netniV/stfc-mod). Currently only officer, research, buildings, and ships are persisted; the mod also sends others that are accepted (200) but not stored.

  - **Tech (forbidden tech)** — main priority. Persist `type: "ft"` (mod payload) into a roster or data file so player forbidden-tech state can drive the optimizer or bonus layer.
  - Other candidates (lower priority): traits, slots, buffs, resources, missions, battlelogs, inventory, jobs — as product needs and data shapes are clarified.

See [SYNC.md](SYNC.md) for the current sync protocol and payload reference.
