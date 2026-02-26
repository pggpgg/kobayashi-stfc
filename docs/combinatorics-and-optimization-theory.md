# Combinatorics and Optimization Theory

## Experiment: Baseline Crew Combination Space

**Objective.** Establish the total number of distinct crew combinations possible under the game’s slot rules, using the current canonical officer roster. This serves as the baseline for measuring the effect of later optimizations.

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
- Captain slot is filled by exactly one officer; bridge by two (order matters: slot A vs slot B); below decks by seven (order matters: seven distinct slots).

---

### 2. Officer roster (baseline)

**Source.** `data/officers/officers.canonical.json` (canonical officer list).

**Count.** **N = 277** officers.

For this baseline we assume **no role restrictions**: any officer may be placed in any role (captain, bridge, or below decks). In practice, the code may restrict some officers to bridge-only or below-decks-only; those restrictions would shrink the pools and the total count. This document’s baseline is the **unrestricted** count so that optimizations can be compared against the maximum possible space.

---

### 3. Combinatorics (unrestricted)

We count **ordered** assignments with **all 10 officers distinct**.

1. **Captain**  
   Choose 1 from N:  
   **C_captain = N**

2. **Bridge**  
   Choose 2 distinct officers from the remaining (N−1), and assign them to the two bridge slots (order matters):  
   **C_bridge = (N−1)(N−2)**

3. **Below decks**  
   From the remaining (N−3) officers, choose 7 and assign them to the 7 below-deck slots (order matters). That is 7-permutations of (N−3):  
   **C_below = P(N−3, 7) = (N−3)(N−4)(N−5)(N−6)(N−7)(N−8)(N−9)**

**Total combinations:**

```
C_total = N × (N−1)(N−2) × P(N−3, 7)
        = N × (N−1)(N−2) × (N−3)(N−4)(N−5)(N−6)(N−7)(N−8)(N−9)
```

With **N = 277**:

| Factor              | Value        |
|---------------------|--------------|
| C_captain           | 277          |
| C_bridge            | 276 × 275 = 75,900 |
| C_below = P(274, 7) | 274×273×272×271×270×269×268 ≈ 1.07325×10¹⁷ |

**Baseline total:**

**C_total = 2,256,439,989,832,254,914,688,000**  
**(≈ 2.256 × 10²⁴)**

---

### 4. Baseline summary

| Quantity        | Value / Formula |
|-----------------|------------------|
| Officers (N)    | 277              |
| Captain choices | N = 277          |
| Bridge pairs    | (N−1)(N−2) = 75,900 |
| Below-deck 7-tuples | P(N−3, 7) ≈ 1.073×10¹⁷ |
| **Total crew combinations** | **2,256,439,989,832,254,914,688,000** |

This is the **baseline** combination space. Any constraint (e.g. role eligibility, “below decks: only 7 highest-power if no below-decks ability”) will reduce this number; the reduction can be computed and recorded below.

---

### 5. Optimization log (reductions from baseline)

*Add each optimization as a new subsection. For each, state the rule, the new counts (or formula), and the reduction (absolute and/or factor).*

*(No optimizations applied yet.)*

---

#### 5.1 *[Example placeholder]*

- **Rule:** *(e.g. “Below decks: for officers without a below-decks ability, only allow the 7 highest-power officers.”)*
- **New pool sizes:** *(e.g. captain pool, bridge pool, below-decks pool and how they’re derived.)*
- **New total:** *(formula and numeric value.)*
- **Reduction:** *(e.g. “Reduced by factor of X” or “Reduced by Y combinations.”)*

---

*Document version: baseline established 2026-02-25. Optimizations to be appended in §5.*
