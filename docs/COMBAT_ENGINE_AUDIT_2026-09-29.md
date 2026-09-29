# Combat engine audit — 2026-09-29

This audit used the local `fight samples/` exports as observations of game event order and
damage application. Their player profiles may have changed since recording, so absolute
damage and survival comparisons remain calibration targets rather than exact regressions.

## Corrected mechanics

| Mechanic | Evidence | Engine change |
| --- | --- | --- |
| Normal shield depletion | In `realta vs takret militia 10.csv`, round 1 event 2 reports 1,387 shield damage and 369 hull damage against 360 remaining shield HP and 470 hull HP. The hostile survives to event 5. Sending the excess 1,027 shield damage to hull would have killed it at event 2. | A shield-breaking hit applies only its direct hull portion; shield-assigned excess is discarded. Later hits land fully on hull. The Breen Energy-Dampening Field's special routing retains its separate overflow behavior pending a Breen fight export. |
| PvE volley order | Every supplied export containing attack rows starts with a hostile volley. The Realta export alternates hostile, player, hostile, player. The Enterprise-D and V'ger exports likewise group shots by weapon volley. | The defender volley fires before the player volley for each weapon index. A shield-break reaction from the player volley can affect later defender volleys in the round. |
| Fire after destruction | The recorded fights end their attack sequence when a ship is destroyed. | Weapon and shot loops stop once the target's hull is exhausted, including the SIMD batch application path. A lethal hostile volley prevents the following player volley. |

The deterministic golden fixture was regenerated for these intentional changes. Combat,
calibration, property, Breen, and Xindi checks pass with the current data after their
expectations were updated to follow the observed order.

## Remaining fidelity gaps

- The Gorn Eviscerator export is a round-one critical win, but the bundled demo-profile
  Monte Carlo produced zero round-one kills in 2,000 trials after these corrections. The
  historical profile is unavailable, and the recorded hit's critical spike is not reproduced.
  The fixture keeps its damage and win-rate checks and records the round-one gap explicitly.
- The supplied exports are PvE. Defender-first ordering in player-versus-player combat has not
  been checked against an in-game PvP export.
- `total_isolytic_damage` measures the isolytic leg before shield HP caps it, whereas
  `total_damage` sums applied hull and shield HP damage. Isolytic damage can therefore exceed
  applied total damage on a shield-breaking hit. The client log also reports attempted shield
  damage above remaining shield HP.
- Buffs triggered by an incoming shield break enter the round accumulator for later weapon
  sub-rounds. Their effect on the player's immediately following volley still needs a
  corresponding event-level log comparison.

These observations support the listed fixes but do not establish a complete, current STFC
formula. Future calibration should capture the player's profile alongside each fight export.
