# Track D — in-game verification checklist (HiggsBozo)

Use after Track D2 code lands. Record findings in `officer_modeling_fidelity.yaml` or this file, then tune catalog `duration_rounds` / values if needed.

| id | Ship | What to verify | Observed (fill in) | Catalog OK? |
| --- | --- | --- | --- | --- |
| `2441576367` | B'Rel | Debuff stat: pierce vs accuracy vs all three; round-1 only | | |
| `1379978713` | Sanctus | Shield drain % of max per round; 5-round cap at tier | | |
| `701705952` | Quv'Sompek | 5-round pierce/accuracy debuff magnitude vs tooltip tier | | |
| `1463338054` | U.S.S. Intrepid | Dodge/deflection/armor stack with officer buffs | | |
| `509252162` | (reclassified) | Attack multiplier still applies in optimize | | |
| `2425475474` | (reclassified) | Conqueror borg beam suppression vs Conqueror Borg | | |

**Simulator proxies (known):** Quv'Sompek / B'Rel use counter-pierce scaling only, not full `component_mitigation` accuracy leg. Intrepid applies one fraction to both mitigation and dodge.
