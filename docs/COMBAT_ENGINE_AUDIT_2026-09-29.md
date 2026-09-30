# Combat engine audit — 2026-09-29

This audit used the local `fight samples/` exports as observations of game event order and
damage application. Their player profiles may have changed since recording, so absolute
damage and survival comparisons remain calibration targets rather than exact regressions.

## Corrected mechanics

| Mechanic | Evidence | Engine change |
| --- | --- | --- |
| Normal shield depletion | The player confirmed that damage assigned beyond remaining shield HP spills into hull on the same hit. The Realta export's displayed shield and hull damage columns cannot be treated as direct post-hit HP deltas: interpreting its event 2 figures that way conflicts with the hostile surviving to event 5. | A shield-breaking hit sends its shield-assigned excess into hull. Later hits land fully on hull. Breen Energy-Dampening Field uses the same overflow helper with its special shield-routing rule. |
| PvE volley order | Every supplied export containing attack rows starts with a hostile volley. The Realta export alternates hostile, player, hostile, player. The Enterprise-D and V'ger exports likewise group shots by weapon volley. | The defender volley fires before the player volley for each weapon index. A shield-break reaction from the player volley can affect later defender volleys in the round. |
| Fire after destruction | The recorded fights end their attack sequence when a ship is destroyed. | Weapon and shot loops stop once the target's hull is exhausted, including the SIMD batch application path. A lethal hostile volley prevents the following player volley. |

The deterministic golden fixture was regenerated for these intentional changes. Combat,
calibration, property, Breen, and Xindi checks pass with the current data after their
expectations were updated to follow the observed order.

## Remaining fidelity gaps

- The Gorn Eviscerator export is a round-one critical win. With the bundled demo profile,
  Monte Carlo produced one round-one kill in 2,000 trials. The historical profile is
  unavailable, so that frequency cannot be compared directly to the recorded fight.
  The fixture keeps damage, win-rate, and a minimal nonzero round-one check.
- The supplied exports are PvE. Defender-first ordering in player-versus-player combat has not
  been checked against an in-game PvP export.
- The Realta export reports shield damage above the hostile's stated maximum shield HP while
  the hostile survives. This discrepancy needs event-level remaining-HP data before its display
  columns can be used to infer the engine's damage split. The overflow rule follows the player's
  correction rather than that ambiguous export interpretation.
- Buffs triggered by an incoming shield break enter the round accumulator for later weapon
  sub-rounds. Their effect on the player's immediately following volley still needs a
  corresponding event-level log comparison.

These observations support the listed fixes but do not establish a complete, current STFC
formula. Future calibration should capture the player's profile alongside each fight export.
