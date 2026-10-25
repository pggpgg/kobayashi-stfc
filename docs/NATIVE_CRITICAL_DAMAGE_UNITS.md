# Native critical damage units

The game's native `Attack.CritModifier` / `CriticalDamage` is the bonus over a
noncritical hit. A native value of `1.5` therefore produces a final critical-hit
factor of `2.5`. KOBAYASHI's public `Combatant.crit_multiplier` continues to mean
the final factor; manual and synthetic records retain that convention.

The retained client ThreatConfig `dpr` equation is
`base + base * CriticalChance * CriticalDamage`, where
`base = AvgDMG * Shots / ReloadingTime`. See
[`client_threat_formulas_2026-10-20.json`](../data/client_threat_formulas_2026-10-20.json).
The native `StaticSyncDataContainer.GetDPR` at `0x1818f90e0` evaluates this
calculator; `GetAverageDamage` at `0x1818f8870` uses the minimum/maximum midpoint.
The equation establishes the units independently of matching a combat result.

Ship, extended-ship, and hostile records have an optional
`crit_damage_is_bonus` source flag. `true` converts `1 + native_value` once at
record-to-combat projection, for both the scalar fallback and every explicit
weapon row. The raw record remains unchanged, so the native display value is
still available. `false` explicitly preserves legacy final-factor semantics.
Component replacement preserves the source flag and the same conversion rule.
New native normalizer output writes `true`; the legacy normalizer writes `false`.

Existing normalized assets predate that flag. A compact
[`native_critical_damage_sources.json`](../data/native_critical_damage_sources.json)
binds 116 ships and 5,519 hostiles whose scalar and every weapon critical value
match their cached upstream native source. The loader uses membership only when
the record has no explicit flag. Unmatched records and direct deserialization
retain legacy semantics. A custom record reusing a known native ID must set
`crit_damage_is_bonus: false` when its critical values already mean final factors.
Regenerate this manifest with
`node scripts/build_native_critical_damage_manifest.mjs` after refreshing assets.
The manifest is included in optimizer reuse fingerprints.

For the controlled Realta, the native base `1.5` plus native additive profile
bonuses `2.14` gives the displayed critical bonus `3.64` and the combat factor
`4.64`. This conversion does not establish the timing or stacking of additional
effects activated during combat. It also does not establish a server random-roll
distribution from the displayed weapon range.
