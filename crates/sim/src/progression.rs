//! Deterministic technology, civic, policy, and victory state.

use std::collections::{BTreeMap, BTreeSet};

use histeries_ruleset::{AgeDefinition, PolicyDefinition, Ruleset};

/// A seat's current age and all capabilities unlocked through that age.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct AgeObservation {
    /// Stable ruleset identifier of the current age.
    pub id: String,
    /// Ruleset display name.
    pub name: String,
    /// Cumulative capability identifiers in lexical order.
    pub unlocks: Vec<String>,
}

impl AgeObservation {
    pub(crate) fn from_reached(ages: &[AgeDefinition]) -> Option<Self> {
        let current = ages.last()?;
        let mut unlocks: Vec<_> = ages.iter().flat_map(|age| age.unlocks()).cloned().collect();
        unlocks.sort_unstable();
        Some(Self {
            id: current.id().to_owned(),
            name: current.name().to_owned(),
            unlocks,
        })
    }
}

/// The two Phase 1 victory predicates.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
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
    /// Returns whether this seat has unlocked a capability in any age reached so far.
    pub fn has_age_unlock(&self, ruleset: &Ruleset, capability: &str) -> bool {
        self.reached_ages(ruleset)
            .iter()
            .any(|age| age.unlocks().iter().any(|id| id == capability))
    }

    pub(crate) fn reached_ages<'a>(&self, ruleset: &'a Ruleset) -> &'a [AgeDefinition] {
        let ages = ruleset.ages();
        let reached = ages
            .iter()
            .take_while(|age| {
                age.required_technologies()
                    .iter()
                    .all(|id| self.has_technology(id))
                    && age.required_civics().iter().all(|id| self.has_civic(id))
            })
            .count();
        &ages[..reached]
    }

    pub(crate) fn permits_policy(&self, policy: &PolicyDefinition, ages: &[AgeDefinition]) -> bool {
        policy.required_civic().is_none_or(|id| self.has_civic(id))
            && policy
                .required_age()
                .is_none_or(|id| ages.iter().any(|age| age.id() == id))
    }

    /// Settles both selected projects before applying entry hooks for newly reached ages.
    pub(crate) fn resolve_research(&mut self, ruleset: &Ruleset) -> bool {
        let technology = self
            .current_technology
            .as_deref()
            .and_then(|id| ruleset.technology(id))
            .filter(|tech| self.technology_progress >= i64::from(tech.cost()));
        let civic = self
            .current_civic
            .as_deref()
            .and_then(|id| ruleset.civic(id))
            .filter(|civic| self.civic_progress >= i64::from(civic.cost()));
        if technology.is_none() && civic.is_none() {
            return false;
        }
        let previous = self.reached_ages(ruleset).len();
        if let Some(tech) = technology {
            self.technology_progress -= i64::from(tech.cost());
            self.technologies.insert(tech.id().to_owned());
            self.current_technology = None;
        }
        if let Some(civic) = civic {
            self.civic_progress -= i64::from(civic.cost());
            self.civics.insert(civic.id().to_owned());
            self.current_civic = None;
        }
        for age in &self.reached_ages(ruleset)[previous..] {
            if age.reset_science() {
                self.technology_progress = 0;
            }
            if age.reset_culture() {
                self.civic_progress = 0;
            }
        }
        true
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, PlayerId, Session, city::base_ruleset};

    const RESEARCH: &str = r#"
        schema_version=1
        id='test'
        name='Test'
        provenance='designed'
        [[technologies]]
        id='tools'
        cost=2
        [[technologies]]
        id='ships'
        cost=3
        [[civics]]
        id='council'
        cost=5
    "#;

    #[test]
    fn technology_and_civic_settle_before_entry_hooks_and_each_hook_runs_once() {
        for (reset_science, reset_culture) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let rules = Ruleset::from_toml(&format!(
                r#"{RESEARCH}
                [[ages]]
                id='first'
                name='First'
                unlocks=['z_ability']
                [[ages]]
                id='second'
                name='Second'
                required_technologies=['tools']
                reset_culture={reset_culture}
                [[ages]]
                id='third'
                name='Third'
                required_civics=['council']
                reset_science={reset_science}
                unlocks=['a_ability']
            "#
            ))
            .unwrap();
            let mut progress = PlayerProgress {
                technology_progress: 12,
                civic_progress: 19,
                current_technology: Some("tools".into()),
                current_civic: Some("council".into()),
                ..Default::default()
            };
            assert!(progress.resolve_research(&rules));
            let age = AgeObservation::from_reached(progress.reached_ages(&rules)).unwrap();
            assert_eq!(age.id, "third");
            assert_eq!(age.unlocks, ["a_ability", "z_ability"]);
            assert!(progress.has_age_unlock(&rules, "z_ability"));
            assert!(progress.has_age_unlock(&rules, "a_ability"));
            assert_eq!(
                progress.technology_progress,
                if reset_science { 0 } else { 10 }
            );
            assert_eq!(progress.civic_progress, if reset_culture { 0 } else { 14 });
            progress.technology_progress = 13;
            progress.civic_progress = 11;
            progress.current_technology = Some("ships".into());
            assert!(progress.resolve_research(&rules));
            assert_eq!(
                (progress.technology_progress, progress.civic_progress),
                (10, 11)
            );
        }
    }

    #[test]
    fn civic_can_finish_an_age_and_reset_bank_under_an_unfinished_technology() {
        let rules = Ruleset::from_toml(&format!(
            r#"{RESEARCH}
            [[ages]]
            id='first'
            name='First'
            [[ages]]
            id='next'
            name='Next'
            required_technologies=['tools']
            required_civics=['council']
            reset_science=true
        "#
        ))
        .unwrap();
        let mut progress = PlayerProgress {
            technology_progress: 4,
            current_technology: Some("tools".into()),
            ..Default::default()
        };
        assert!(progress.resolve_research(&rules));
        assert_eq!(progress.technology_progress, 2);
        assert_eq!(progress.reached_ages(&rules).len(), 1);
        progress.current_technology = Some("ships".into());
        progress.current_civic = Some("council".into());
        progress.civic_progress = 7;
        assert!(progress.resolve_research(&rules));
        assert_eq!(progress.reached_ages(&rules).len(), 2);
        assert_eq!(progress.technology_progress, 0);
        assert_eq!(progress.civic_progress, 2);
        assert_eq!(progress.current_technology.as_deref(), Some("ships"));
        assert!(progress.has_technology("tools") && progress.has_civic("council"));
    }

    #[test]
    fn ruleset_without_ages_retains_ordinary_research_overflow() {
        let rules = Ruleset::from_toml(RESEARCH).unwrap();
        let mut progress = PlayerProgress {
            technology_progress: 9,
            current_technology: Some("tools".into()),
            ..Default::default()
        };
        assert!(progress.resolve_research(&rules));
        assert_eq!(
            AgeObservation::from_reached(progress.reached_ages(&rules)),
            None
        );
        assert!(!progress.has_age_unlock(&rules, "orbital_access"));
        assert_eq!(progress.technology_progress, 7);
    }

    #[test]
    fn research_paid_from_stored_science_changes_age_on_dispatch() {
        let player = PlayerId::new(1);
        let mut initial = Session::new([player]).state_snapshot();
        let progress = initial.progress.get_mut(&player).unwrap();
        progress
            .technologies
            .extend((1..12).map(|number| format!("tech_{number:02}")));
        progress.technology_progress =
            i64::from(base_ruleset().technology("tech_12").unwrap().cost()) + 1;
        let mut session = Session::from_save_bytes(&initial.encode()).unwrap();
        session
            .dispatch(Command::ResearchTechnology {
                player,
                technology: "tech_12".into(),
            })
            .unwrap();
        let observed = session.observe(player).unwrap();
        assert_eq!(observed.turn(), 0);
        assert_eq!(observed.progress().age.as_ref().unwrap().id, "industrial");
        assert_eq!(observed.progress().technology_progress, 1);
        assert_eq!(
            session.replay().replay().unwrap().state_hash(),
            session.state_hash()
        );
    }

    #[test]
    fn unmet_earlier_age_blocks_later_capabilities() {
        let rules = Ruleset::from_toml(&format!(
            r#"{RESEARCH}
            [[ages]]
            id='early'
            name='Early'
            [[ages]]
            id='middle'
            name='Middle'
            required_civics=['council']
            [[ages]]
            id='late'
            name='Late'
            required_technologies=['tools']
            unlocks=['flight']
        "#
        ))
        .unwrap();
        let mut progress = PlayerProgress::default();
        progress.technologies.insert("tools".into());
        assert_eq!(progress.reached_ages(&rules).last().unwrap().id(), "early");
        assert!(!progress.has_age_unlock(&rules, "flight"));
    }

    #[test]
    fn load_rejects_active_policy_without_required_age() {
        let player = PlayerId::new(1);
        let mut state = Session::new([player]).state_snapshot();
        let progress = state.progress.get_mut(&player).unwrap();
        progress
            .policies
            .insert(0, "policy_orbital_research".into());
        assert!(matches!(
            Session::from_save_bytes(&state.encode()),
            Err(crate::SerializationError::InvalidProgressState)
        ));
    }
}
