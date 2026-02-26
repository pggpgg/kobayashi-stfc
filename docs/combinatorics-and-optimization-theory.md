# Combinatorics and Optimization Theory

## Experiment: Baseline Crew Combination Space

**Objective.** Establish the total number of distinct crew combinations possible under the game's slot rules, using the current canonical officer roster. This serves as the baseline for measuring the effect of later optimizations.

**Date.** 2026-02-25

---

### 1. Constraints (ship crew layout)

A ship has:

| Role          | Slots | Description                    |
|---------------|-------|--------------------------------|
| Captain       | 1     | Single captain                 |
| Bridge        | 2     | Two bridge officers            |
| Below decks   | 7     | Seven below-deck officers       |

**Total crew size:** 1 + 2 + 7 = 10 distinct officers per crew.

**Rules.**

- No officer may appear in more than one slot (all 10 must be distinct).
- Captain: exactly one officer. Bridge: two officers, **order does not matter** (the two slots are interchangeable). Below decks: seven officers, order matters (seven distinct slots).

---

### 2. Officer roster (baseline)

**Source.** Owned roster (officers the player currently has).

**Count.** **N = 231** officers.

For this baseline we assume **no role restrictions**: any officer may be placed in any role (captain, bridge, or below decks). In practice, the code may restrict some officers to bridge-only or below-decks-only; those restrictions would shrink the pools and the total count. This document's baseline is the **unrestricted** count so that optimizations can be compared against the maximum possible space.

---

### 3. Combinatorics (unrestricted)

We count assignments with **all 10 officers distinct**: captain (1); bridge (unordered pair of 2); below decks (ordered 7-tuple).

1. **Captain**  
   Choose 1 from N:  
   **C_captain = N**

2. **Bridge**  
   Choose 2 distinct officers from the remaining (N−1); **order does not matter** (combination, not permutation):  
   **C_bridge = C(N−1, 2) = (N−1)(N−2) / 2**

3. **Below decks**  
   From the remaining (N−3) officers, choose 7 and assign them to the 7 below-deck slots (order matters). That is 7-permutations of (N−3):  
   **C_below = P(N−3, 7) = (N−3)(N−4)(N−5)(N−6)(N−7)(N−8)(N−9)**

**Total combinations:**

```
C_total = N × (N−1)(N−2)/2 × P(N−3, 7)
        = N × (N−1)(N−2)/2 × (N−3)(N−4)(N−5)(N−6)(N−7)(N−8)(N−9)
```

With **N = 231**:

| Factor              | Value        |
|---------------------|--------------|
| C_captain           | 231          |
| C_bridge            | C(230, 2) = 230×229/2 = 26,335 |
| C_below = P(228, 7) | 228×227×226×225×224×223×222 ≈ 2.918×10¹⁶ |

**Baseline total:**

**C_total = 177,542,699,875,593,966,144,000**  
**(≈ 1.775 × 10²³)**

---

### 4. Baseline summary

| Quantity        | Value / Formula |
|-----------------|------------------|
| Officers (N)    | 231 (owned)     |
| Captain choices | N = 231          |
| Bridge (unordered pair) | C(N−1, 2) = 26,335 |
| Below-deck 7-tuples | P(N−3, 7) ≈ 2.918×10¹⁶ |
| **Total crew combinations** | **177,542,699,875,593,966,144,000** |

This is the **baseline** combination space. Any constraint (e.g. role eligibility, "below decks: only 7 highest-power if no below-decks ability") will reduce this number; the reduction can be computed and recorded below.

---

### 5. Time to exhaust all combinations (benchmark-based estimate)

If we **simulate one combat per crew combination** to evaluate it, the total number of simulations equals **C_total**. Using the benchmark throughput from the project's `PERFORMANCE.md` (run after the sim efficiency plan):

| Benchmark | Throughput | Source |
|-----------|------------|--------|
| Single combat (100 rounds) | **829,736** combats/s | `benchmark_simulator --log` |
| Monte Carlo parallel (64 candidates × 1000 it) | **513,081** sims/s | `benchmark_parallel_speedup` |

**One simulation per combination:**

| Throughput | Time (seconds) | Time (years) |
|------------|-----------------|--------------|
| 829,736 /s | ≈ 2.14 × 10¹⁷ s | **≈ 6.8 billion years** |
| 513,081 /s | ≈ 3.46 × 10¹⁷ s | **≈ 11.0 billion years** |

So exhausting all **177.5 × 10²¹** combinations at current benchmark speeds is on the order of **billions of years** (far beyond the age of the Universe). This motivates reducing the combination space with the optimizations in §6 (e.g. role restrictions, below-decks pruning) and/or not enumerating all combinations (e.g. sampling, search, heuristics).

**If we run *k* iterations per combination** (e.g. Monte Carlo with *k* = 1000 for win-rate estimates), the time above is multiplied by *k*.

---

### 6. Optimization log (reductions from baseline)

*Add each optimization as a new subsection. For each, state the rule, the new counts (or formula), and the reduction (absolute and/or factor).*

---

#### 6.1 Bridge–captain synergy (same group)

- **Rule:** Ignore a bridge officer unless they have **synergy** with the captain. Synergy = same **group** as the captain (e.g. "TOS ENTERPRISE CREW", "SECTION 31"). Bridge slots are still unordered; below decks unchanged.
- **Formula:** For each captain-eligible officer *c*, let *n(c)* = number of bridge-eligible officers in the same group as *c* (excluding *c* if *c* is bridge-eligible). Bridge combinations for that captain = C(*n(c)*, 2). Total: **C_total_synergy = P(N−3, 7) × Σ_c C(*n(c)*, 2)** (sum over captain-eligible *c*).
- **Computed on canonical roster (N = 277):** Σ_c C(*n(c)*, 2) = **4,626** (vs 277 × C(276, 2) = 10,512,150 unrestricted). New total (277) ≈ **4.96 × 10²⁰**. **Reduction factor ≈ 0.00044** (≈ 1∕2,270).
- **For N = 231 (owned):** Exact total depends on which 231 you own. If group distribution is similar, new total ≈ baseline_231 × 0.00044 ≈ **7.8 × 10¹⁹** (vs 1.78 × 10²³). **Reduction by a factor of ~2,270.**
- **Time to exhaust (at 830k/s):** ~7.8×10¹⁹ / 829736 ≈ 3.0×10¹³ s ≈ **950,000 years** (still huge, but ~7,200× less than baseline).

---

#### 6.2 Captain must have a captain ability

- **Rule:** Ignore officers for the captain slot unless they have at least one **captain ability** (i.e. an ability with `slot == "captain"`). Bridge and below decks unchanged.
- **Formula:** Let **K** = number of captain-eligible officers. Then **C_total = K × C(N−1, 2) × P(N−3, 7)** (same as baseline but with K instead of N for the captain factor).
- **Computed on canonical roster (N = 277):** Captain-eligible **K = 203** (74 officers have no captain ability). New total (277) ≈ **8.27 × 10²³**. **Reduction factor = K∕N = 203∕277 ≈ 0.733** (≈ 27% fewer combinations).
- **For N = 231 (owned):** If the same proportion of officers have a captain ability, K ≈ 231 × (203∕277) ≈ **169**. New total ≈ baseline_231 × 0.733 ≈ **1.30 × 10²³**. **Reduction by a factor of ~1.37** (about 27% smaller).
- **Note:** This is a mild reduction (only captains without a captain ability are excluded). It stacks with other optimizations: e.g. apply §6.1 (synergy) on top of this by summing only over captain-eligible *c* and using same-group bridge counts.

---

#### 6.3 *[Next optimization placeholder]*

- **Rule:** *(e.g. "Below decks: for officers without a below-decks ability, only allow the 7 highest-power officers.")*
- **New pool sizes / formula:** *(to be filled.)*
- **Reduction:** *(to be filled.)*

---

*Document version: baseline established 2026-02-25; §6.1 bridge–captain synergy, §6.2 captain-ability filter added. Optimizations to be appended in §6.*
