# Borg Sphere Omicron evidence and limitations

The level-10 Omicron Particle Charge text displays 2,665,000% base damage
against Conqueror Borg Suppressors and Obliterators. Its upstream ability
509252162 stores `26650`, a fractional bonus, not percentage points. The
ship-ability catalog scopes it to the `conqueror_borg` hostile tag and selects
the ability value by ship level.

The native client static cache extracted on 2026-10-23 (SHA-256
`b472e4460e3869cc8abb44e456a0873e3c5702ed6f3a9701b18d27938022f3cf`)
labels this effect `ModAllDamage`, operation `MultiplyAdd`, at combat start.
The client component damage getters apply `base + additions`, followed by
`1 + MULT`. This is evidence against multiplying an already fully buffed attack
by 26651. The native server's full ordering of all damage pools is unavailable.

In two captured fights, level-55 and level-57 Suppressors, the deployed Sphere
had two tier-3 kinetic and two tier-2 energy weapon components despite the
ship itself being tier 2. Their native min/max values are respectively
16862–20545 and 4894–5944. The first two level-55 noncritical shots reported
528,777,712 and 485,153,904 pre-reduction standard damage. Dividing by 26651
gives approximately 19841 and 18205, both in the kinetic range. The same
check places all five level-55 and ten independently decoded level-57
noncritical shots in their respective component ranges. For the latter,
standard pre-reduction damage is reconstructed from hull, shield,
mitigation and apex columns after removing the isolytic portion. These bounds
corroborate the large component-base term; they do not separately identify
smaller profile or officer contributions or establish a damage-roll law.

Kobayashi retains the selected component's unbuffed mean before profile and
officer scaling, and adds `component_base * Omicron_bonus` to its existing
legacy attack calculation before critical and isolytic damage. This preserves
the demonstrated component-base magnitude while avoiding double application
of existing profile and officer multipliers. The detailed interactions with
those legacy pools remain approximate; the tool warns for the affected
scenarios. When component base is unavailable the term is omitted. The native
min/max data remain in the upstream ship JSON, but a random distribution or
RNG sequence has not been established, so the simulator uses the mean.
