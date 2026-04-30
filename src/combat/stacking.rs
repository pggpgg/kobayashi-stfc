use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackCategory {
    /// Base contribution (`A`)
    Base,
    /// Multiplicative modifier contribution (`B`)
    Modifier,
    /// Flat additive contribution (`C`)
    Flat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackContribution<K> {
    pub key: K,
    pub category: StackCategory,
    pub value: f64,
}

impl<K> StackContribution<K> {
    pub fn base(key: K, value: f64) -> Self {
        Self {
            key,
            category: StackCategory::Base,
            value,
        }
    }

    pub fn modifier(key: K, value: f64) -> Self {
        Self {
            key,
            category: StackCategory::Modifier,
            value,
        }
    }

    pub fn flat(key: K, value: f64) -> Self {
        Self {
            key,
            category: StackCategory::Flat,
            value,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CategoryTotals {
    pub base: f64,
    pub modifier: f64,
    pub flat: f64,
}

impl CategoryTotals {
    pub fn apply(&mut self, category: StackCategory, value: f64) {
        match category {
            StackCategory::Base => self.base += value,
            StackCategory::Modifier => self.modifier += value,
            StackCategory::Flat => self.flat += value,
        }
    }

    /// Adds another CategoryTotals into self (for merging two accumulators).
    pub fn add_from(&mut self, other: &CategoryTotals) {
        self.base += other.base;
        self.modifier += other.modifier;
        self.flat += other.flat;
    }

    pub fn compose(self) -> f64 {
        self.base * (1.0 + self.modifier) + self.flat
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatStacking<K: Ord> {
    totals: BTreeMap<K, CategoryTotals>,
}

impl<K: Ord> StatStacking<K> {
    pub fn new() -> Self {
        Self {
            totals: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, contribution: StackContribution<K>) {
        self.totals
            .entry(contribution.key)
            .or_default()
            .apply(contribution.category, contribution.value);
    }

    pub fn add_many<I>(&mut self, contributions: I)
    where
        I: IntoIterator<Item = StackContribution<K>>,
    {
        for contribution in contributions {
            self.add(contribution);
        }
    }

    pub fn totals_for(&self, key: &K) -> Option<CategoryTotals> {
        self.totals.get(key).copied()
    }

    pub fn composed_for(&self, key: &K) -> Option<f64> {
        self.totals_for(key).map(CategoryTotals::compose)
    }

    pub fn composed_values(&self) -> BTreeMap<&K, f64> {
        self.totals
            .iter()
            .map(|(key, totals)| (key, totals.compose()))
            .collect()
    }

    pub fn clear(&mut self) {
        self.totals.clear();
    }

    /// Drop accumulated totals for `key` (e.g. after applying round-start regen so it is not double-counted at round end).
    pub(crate) fn remove_totals_for(&mut self, key: &K) {
        self.totals.remove(key);
    }

    /// Iterate accumulated category totals per key (for trace / diagnostics).
    pub(crate) fn iter_totals(&self) -> impl Iterator<Item = (&K, &CategoryTotals)> {
        self.totals.iter()
    }

    /// Merges state from `other` into self (adds totals per key). Used to restore round base without cloning.
    pub fn merge_from(&mut self, other: &StatStacking<K>)
    where
        K: Clone,
    {
        for (key, totals) in &other.totals {
            self.totals.entry(key.clone()).or_default().add_from(totals);
        }
    }
}

pub fn aggregate_contributions<K, I>(contributions: I) -> BTreeMap<K, CategoryTotals>
where
    K: Ord,
    I: IntoIterator<Item = StackContribution<K>>,
{
    let mut totals: BTreeMap<K, CategoryTotals> = BTreeMap::new();
    for contribution in contributions {
        totals
            .entry(contribution.key)
            .or_default()
            .apply(contribution.category, contribution.value);
    }
    totals
}

pub fn compose_totals(totals: CategoryTotals) -> f64 {
    totals.compose()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CategoryTotals ──

    #[test]
    fn compose_all_zeros_yields_zero() {
        let t = CategoryTotals::default();
        assert!((t.compose() - 0.0).abs() < 1e-12);
    }

    #[test]
    fn compose_base_only_yields_base() {
        let t = CategoryTotals {
            base: 100.0,
            ..Default::default()
        };
        assert!((t.compose() - 100.0).abs() < 1e-12);
    }

    #[test]
    fn compose_modifier_only_yields_zero() {
        // base=0, so (1+modifier) is multiplied by 0 → result is just flat (0)
        let t = CategoryTotals {
            modifier: 0.5,
            ..Default::default()
        };
        assert!((t.compose() - 0.0).abs() < 1e-12);
    }

    #[test]
    fn compose_flat_only_yields_flat() {
        let t = CategoryTotals {
            flat: 42.0,
            ..Default::default()
        };
        assert!((t.compose() - 42.0).abs() < 1e-12);
    }

    #[test]
    fn compose_all_three_categories() {
        let t = CategoryTotals {
            base: 100.0,
            modifier: 0.2,
            flat: 10.0,
        };
        // 100 * (1 + 0.2) + 10 = 130
        assert!((t.compose() - 130.0).abs() < 1e-12);
    }

    #[test]
    fn apply_base_category_adds_to_base() {
        let mut t = CategoryTotals::default();
        t.apply(StackCategory::Base, 50.0);
        t.apply(StackCategory::Base, 30.0);
        assert!((t.base - 80.0).abs() < 1e-12);
    }

    #[test]
    fn apply_modifier_category_adds_to_modifier() {
        let mut t = CategoryTotals::default();
        t.apply(StackCategory::Modifier, 0.1);
        t.apply(StackCategory::Modifier, 0.2);
        assert!((t.modifier - 0.3).abs() < 1e-12);
    }

    #[test]
    fn apply_flat_category_adds_to_flat() {
        let mut t = CategoryTotals::default();
        t.apply(StackCategory::Flat, 5.0);
        t.apply(StackCategory::Flat, 7.0);
        assert!((t.flat - 12.0).abs() < 1e-12);
    }

    #[test]
    fn add_from_merges_two_category_totals() {
        let mut a = CategoryTotals {
            base: 10.0,
            modifier: 0.1,
            flat: 5.0,
        };
        let b = CategoryTotals {
            base: 20.0,
            modifier: 0.2,
            flat: 3.0,
        };
        a.add_from(&b);
        assert!((a.base - 30.0).abs() < 1e-12);
        assert!((a.modifier - 0.3).abs() < 1e-12);
        assert!((a.flat - 8.0).abs() < 1e-12);
    }

    // ── StatStacking ──

    #[test]
    fn stat_stacking_add_single_contribution() {
        let mut s = StatStacking::<&str>::new();
        s.add(StackContribution::base("atk", 100.0));
        let c = s.composed_for(&"atk").unwrap();
        assert!((c - 100.0).abs() < 1e-12);
    }

    #[test]
    fn stat_stacking_add_many_to_same_key() {
        let mut s = StatStacking::<&str>::new();
        s.add_many([
            StackContribution::base("atk", 100.0),
            StackContribution::modifier("atk", 0.2),
            StackContribution::flat("atk", 10.0),
        ]);
        let c = s.composed_for(&"atk").unwrap();
        // 100 * (1 + 0.2) + 10 = 130
        assert!((c - 130.0).abs() < 1e-12);
    }

    #[test]
    fn stat_stacking_composed_for_missing_key_returns_none() {
        let s = StatStacking::<&str>::new();
        assert!(s.composed_for(&"nonexistent").is_none());
    }

    #[test]
    fn stat_stacking_composed_values_returns_all_keys() {
        let mut s = StatStacking::<&str>::new();
        s.add(StackContribution::base("x", 1.0));
        s.add(StackContribution::flat("y", 2.0));
        let vals = s.composed_values();
        assert_eq!(vals.len(), 2);
        assert!((*vals.get(&"x").unwrap() - 1.0).abs() < 1e-12);
        assert!((*vals.get(&"y").unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn stat_stacking_remove_totals_for_drops_key() {
        let mut s = StatStacking::<&str>::new();
        s.add(StackContribution::base("atk", 100.0));
        assert!(s.composed_for(&"atk").is_some());
        s.remove_totals_for(&"atk");
        assert!(s.composed_for(&"atk").is_none());
    }

    #[test]
    fn stat_stacking_clear_empties_all_keys() {
        let mut s = StatStacking::<&str>::new();
        s.add(StackContribution::base("a", 1.0));
        s.add(StackContribution::base("b", 2.0));
        s.clear();
        assert!(s.composed_values().is_empty());
    }

    #[test]
    fn stat_stacking_merge_from_combines_two_stacks() {
        let mut a = StatStacking::<i32>::new();
        a.add(StackContribution::base(1, 10.0));

        let mut b = StatStacking::<i32>::new();
        b.add(StackContribution::modifier(1, 0.5));
        b.add(StackContribution::base(2, 20.0));

        a.merge_from(&b);
        // key 1: base=10, modifier=0.5 → 10 * 1.5 = 15
        assert!((a.composed_for(&1).unwrap() - 15.0).abs() < 1e-12);
        // key 2: base=20 → 20
        assert!((a.composed_for(&2).unwrap() - 20.0).abs() < 1e-12);
    }

    #[test]
    fn stacking_is_order_independent() {
        let contributions = [
            StackContribution::base("k", 100.0),
            StackContribution::modifier("k", 0.3),
            StackContribution::flat("k", 10.0),
            StackContribution::modifier("k", 0.1),
        ];

        let mut s1 = StatStacking::<&str>::new();
        s1.add_many(contributions.clone());

        let mut s2 = StatStacking::<&str>::new();
        // Reverse order
        for i in (0..contributions.len()).rev() {
            s2.add(contributions[i].clone());
        }

        let v1 = s1.composed_for(&"k").unwrap();
        let v2 = s2.composed_for(&"k").unwrap();
        // 100 * (1 + 0.3 + 0.1) + 10 = 150
        assert!((v1 - 150.0).abs() < 1e-12);
        assert!((v1 - v2).abs() < 1e-12);
    }

    #[test]
    fn stacking_order_independence_across_keys() {
        let c1 = StackContribution::base("a", 10.0);
        let c2 = StackContribution::base("b", 20.0);

        let mut s1 = StatStacking::<&str>::new();
        s1.add_many([c1.clone(), c2.clone()]);

        let mut s2 = StatStacking::<&str>::new();
        s2.add_many([c2, c1]);

        assert!((s1.composed_for(&"a").unwrap() - 10.0).abs() < 1e-12);
        assert!((s2.composed_for(&"a").unwrap() - 10.0).abs() < 1e-12);
        assert!((s1.composed_for(&"b").unwrap() - 20.0).abs() < 1e-12);
        assert!((s2.composed_for(&"b").unwrap() - 20.0).abs() < 1e-12);
    }

    // ── aggregate_contributions / compose_totals ──

    #[test]
    fn aggregate_contributions_bundles_by_key() {
        let map = aggregate_contributions([
            StackContribution::base("x", 50.0),
            StackContribution::modifier("x", 0.1),
            StackContribution::flat("y", 7.0),
        ]);
        assert_eq!(map.len(), 2);
        assert!((map[&"x"].base - 50.0).abs() < 1e-12);
        assert!((map[&"x"].modifier - 0.1).abs() < 1e-12);
        assert!((map[&"y"].flat - 7.0).abs() < 1e-12);
    }

    #[test]
    fn compose_totals_wraps_category_totals_compose() {
        let t = CategoryTotals {
            base: 10.0,
            modifier: 1.0,
            flat: 5.0,
        };
        // 10 * (1 + 1) + 5 = 25
        assert!((compose_totals(t) - 25.0).abs() < 1e-12);
    }

    // ── StackContribution constructors ──

    #[test]
    fn contribution_base_sets_category() {
        let c = StackContribution::<&str>::base("k", 3.0);
        assert_eq!(c.key, "k");
        assert_eq!(c.category, StackCategory::Base);
        assert!((c.value - 3.0).abs() < 1e-12);
    }

    #[test]
    fn contribution_modifier_sets_category() {
        let c = StackContribution::<&str>::modifier("k", 0.5);
        assert_eq!(c.category, StackCategory::Modifier);
    }

    #[test]
    fn contribution_flat_sets_category() {
        let c = StackContribution::<&str>::flat("k", 9.0);
        assert_eq!(c.category, StackCategory::Flat);
    }
}
