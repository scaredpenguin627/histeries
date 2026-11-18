//! Deterministic technology, civic, policy, and victory state.

use std::collections::{BTreeMap, BTreeSet};

/// The two Phase 1 victory predicates.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum VictoryKind {
    /// One player owns the only remaining cities.
    Domination = 0,
    /// One player completed the full technology table.
    Science = 1,
}

impl VictoryKind {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Domination,
            1 => Self::Science,
            _ => return None,
        })
    }
}

/// Per-seat research, civic, and policy state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerProgress {
    pub(crate) technology_progress: i64,
    pub(crate) civic_progress: i64,
    pub(crate) current_technology: Option<String>,
    pub(crate) current_civic: Option<String>,
    pub(crate) technologies: BTreeSet<String>,
    pub(crate) civics: BTreeSet<String>,
    pub(crate) policy_slots: u8,
    pub(crate) policies: BTreeMap<u8, String>,
}

impl Default for PlayerProgress {
    fn default() -> Self {
        Self {
            technology_progress: 0,
            civic_progress: 0,
            current_technology: None,
            current_civic: None,
            technologies: BTreeSet::new(),
            civics: BTreeSet::new(),
            policy_slots: 2,
            policies: BTreeMap::new(),
        }
    }
}

impl PlayerProgress {
    /// Policy slots unlocked by the given number of completed civics.
    pub const fn slots_for_civics(count: usize) -> u8 {
        if count >= 15 {
            5
        } else {
            2 + (count / 5) as u8
        }
    }

    /// Returns whether a technology is complete.
    pub fn has_technology(&self, id: &str) -> bool {
        self.technologies.contains(id)
    }

    /// Returns whether a civic is complete.
    pub fn has_civic(&self, id: &str) -> bool {
        self.civics.contains(id)
    }

    /// Returns active policies in slot order.
    pub fn policies(&self) -> impl Iterator<Item = (u8, &str)> {
        self.policies
            .iter()
            .map(|(slot, policy)| (*slot, policy.as_str()))
    }
}
