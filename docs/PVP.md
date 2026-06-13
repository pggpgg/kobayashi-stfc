# PvP mode

Kobayashi supports **ship-vs-ship** fights for finding attacker crews that beat a fixed opponent setup.

## Workspace

Open **PvP** in the left navigation (`/pvp`).

- **Attacker**: your ship, tier, level, and crew. Uses the active profile from **ProfileSwitcher** (buildings, research, forbidden tech, roster tiers).
- **Defender**: opponent ship, tier, level, and crew. Requires an **Opponent profile** (saved profile id for defender bonuses/roster). May match your active profile when you only have one profile or want a mirror setup (same account bonuses, different ship/crew).
- **Run sim**: Monte Carlo for the current attacker crew vs the fixed defender.
- **Run optimize**: searches **attacker crews only**; defender setup stays fixed for the request.

## API

Send `defender_ship` + `defender_profile_id` instead of `hostile` on:

- `POST /api/simulate`
- `POST /api/optimize` / `POST /api/optimize/start`
- `POST /api/compare/crews`

Rules:

- `hostile` and `defender_ship` are **mutually exclusive**.
- When `defender_ship` is set, `defender_profile_id` is **required**.
- Server sets `defender_opponent` to `player` automatically.

Optional `defender_crew` uses the same shape as attacker `crew` and is merged with the defender ship’s hull abilities.

## What is modeled

- Defender stats from `ships_extended` + opponent profile bonuses (buildings, research, forbidden tech).
- Outbound mitigation / pierce: player attacker vs player defender (no hostile mystery factor).
- Counter-fire: defender ship weapons vs attacker incoming mitigation from opponent profile research.
- LCARS gates that use `defender_is_player_ship` and defender hull class apply when `defender_opponent` is player.

## What is not modeled (v1)

- Optimizing defender crew (attacker discovery only).
- Armada / multi-ship fights.
- Per-round dynamic officer-stat **Defense/Health** axes ship with Phase 4d (2026-06-12). Dynamic `target: enemy` officer-stat debuffs on the defender side remain deferred (no prod LCARS cases). Fight-setup `target: enemy` / `enemy_bridge` officer-stat debuffs **are** applied in PvP (Phase 4c — e.g. Kras “Know Your Enemy” debuffs defender captain + bridge only when `defender_is_player_ship` passes).

## Optimize cache / warm-start

The SPA includes a defender fingerprint in `optimize_cache_key` so warm-start history does not collide across different opponent setups.
