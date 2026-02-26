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

    /// Merges state from `other` into self (adds totals per key). Used to restore round base without cloning.
    pub fn merge_from(&mut self, other: &StatStacking<K>)
    where
        K: Clone,
    {
        for (key, totals) in &other.totals {
            self.totals
                .entry(key.clone())
                .or_default()
                .add_from(totals);
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
