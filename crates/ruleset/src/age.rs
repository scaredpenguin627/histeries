//! Player age progression in ruleset declaration order.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::{CivicDefinition, RulesetError, TechnologyDefinition, validate_id};

/// One age, with prerequisites and hooks applied on entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgeDefinition {
    id: String,
    name: String,
    #[serde(default)]
    required_technologies: Vec<String>,
    #[serde(default)]
    required_civics: Vec<String>,
    #[serde(default)]
    unlocks: Vec<String>,
    #[serde(default)]
    reset_science: bool,
    #[serde(default)]
    reset_culture: bool,
}

impl AgeDefinition {
    /// Stable age identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Technologies a seat must have completed to enter this age.
    pub fn required_technologies(&self) -> &[String] {
        &self.required_technologies
    }

    /// Civics a seat must have completed to enter this age.
    pub fn required_civics(&self) -> &[String] {
        &self.required_civics
    }

    /// Capability identifiers granted on entry and retained in later ages.
    pub fn unlocks(&self) -> &[String] {
        &self.unlocks
    }

    /// Whether entering this age clears unspent science.
    pub const fn reset_science(&self) -> bool {
        self.reset_science
    }

    /// Whether entering this age clears unspent culture.
    pub const fn reset_culture(&self) -> bool {
        self.reset_culture
    }
}

pub(super) fn validate<'a>(
    ages: &'a [AgeDefinition],
    technologies: &BTreeMap<String, TechnologyDefinition>,
    civics: &BTreeMap<String, CivicDefinition>,
) -> Result<BTreeSet<&'a str>, RulesetError> {
    let mut ids = BTreeSet::new();
    let mut unlocks = BTreeSet::new();
    for (index, age) in ages.iter().enumerate() {
        validate_id("age", &age.id)?;
        if !ids.insert(age.id()) {
            return Err(RulesetError::DuplicateId {
                kind: "age",
                id: age.id.clone(),
            });
        }
        if age.name.trim().is_empty() {
            return Err(RulesetError::MissingField("age.name"));
        }
        if index == 0 && (!age.required_technologies.is_empty() || !age.required_civics.is_empty())
        {
            return Err(RulesetError::InvalidValue {
                field: "initial age prerequisites",
                value: age.id.clone(),
            });
        }
        validate_requirements(
            "age.required_technologies",
            &age.required_technologies,
            technologies,
        )?;
        validate_requirements("age.required_civics", &age.required_civics, civics)?;
        for unlock in &age.unlocks {
            validate_id("age unlock", unlock)?;
            if !unlocks.insert(unlock) {
                return Err(RulesetError::DuplicateId {
                    kind: "age unlock",
                    id: unlock.clone(),
                });
            }
        }
    }
    Ok(ids)
}

fn validate_requirements<T>(
    field: &'static str,
    requirements: &[String],
    known: &BTreeMap<String, T>,
) -> Result<(), RulesetError> {
    let mut seen = BTreeSet::new();
    for id in requirements {
        if !known.contains_key(id) || !seen.insert(id) {
            return Err(RulesetError::InvalidValue {
                field,
                value: id.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Ruleset, base_ruleset};

    const HEADER: &str = "schema_version=1\nid='test'\nname='Test'\nprovenance='designed'\n";
    const INITIAL: &str = "[[ages]]\nid='z'\nname='First'\n";
    const NEXT: &str = "[[ages]]\nid='a'\nname='Second'\n";
    const PROGRESSION: &str = "[[technologies]]\nid='t'\ncost=1\n[[civics]]\nid='c'\ncost=1\n";

    #[test]
    fn ages_are_optional_and_policy_gates_must_resolve() {
        let empty = Ruleset::from_toml(HEADER).unwrap();
        assert!(empty.ages().is_empty());
        assert!(!empty.has_reached_age(None, "z"));
        let gate = "[[policies]]\nid='p'\nrequired_age='missing'\n";
        for tables in ["", INITIAL] {
            assert!(Ruleset::from_toml(&format!("{HEADER}{tables}{gate}")).is_err());
        }
    }

    #[test]
    fn declared_order_controls_policy_access_and_transition_hooks_are_independent() {
        let tables = format!(
            "{HEADER}{PROGRESSION}{INITIAL}unlocks=['first_capability']\n{NEXT}\
             required_technologies=['t']\nrequired_civics=['c']\nreset_culture=true\n\
             [[ages]]\nid='third'\nname='Third'\nreset_science=true\n\
             [[policies]]\nid='policy'\nrequired_age='a'\nrequired_civic='c'"
        );
        let rules = Ruleset::from_toml(&tables).unwrap();
        let [first, second, third] = rules.ages() else {
            panic!("expected three declared ages");
        };
        assert_eq!(first.name(), "First");
        assert_eq!(first.unlocks(), &["first_capability"]);
        assert!(!first.reset_science() && !first.reset_culture());
        assert_eq!(second.required_technologies(), &["t"]);
        assert_eq!(second.required_civics(), &["c"]);
        assert!(!second.reset_science() && second.reset_culture());
        assert!(third.reset_science() && !third.reset_culture());
        let policy = rules.policy("policy").unwrap();
        assert_eq!(policy.required_age(), Some("a"));
        assert_eq!(policy.required_civic(), Some("c"));
        for current in [None, Some("missing"), Some("z")] {
            assert!(!rules.has_reached_age(current, "a"));
        }
        assert!(rules.has_reached_age(Some("a"), "a"));
        assert!(rules.has_reached_age(Some("third"), "a"));
        assert!(!rules.has_reached_age(Some("third"), "missing"));
    }

    #[test]
    fn rejects_invalid_age_definitions() {
        for (case, tables) in [
            ("duplicate age", format!("{INITIAL}{INITIAL}")),
            ("invalid slug", INITIAL.replace("'z'", "'X'")),
            ("blank name", INITIAL.replace("'First'", "'  '")),
            (
                "initial tech",
                format!("{INITIAL}required_technologies=['t']"),
            ),
            ("initial civic", format!("{INITIAL}required_civics=['c']")),
            (
                "unknown tech",
                format!("{INITIAL}{NEXT}required_technologies=['c']"),
            ),
            (
                "unknown civic",
                format!("{INITIAL}{NEXT}required_civics=['t']"),
            ),
            (
                "repeated prerequisite",
                format!("{INITIAL}{NEXT}required_civics=['c','c']"),
            ),
            ("invalid capability", format!("{INITIAL}unlocks=['bad id']")),
            (
                "duplicate capability",
                format!("{INITIAL}unlocks=['x']\n{NEXT}unlocks=['x']"),
            ),
            ("misspelled hook", format!("{INITIAL}clear_culture=true")),
        ] {
            assert!(
                Ruleset::from_toml(&format!("{HEADER}{PROGRESSION}{tables}")).is_err(),
                "{case}"
            );
        }
    }

    #[test]
    fn base_age_progression_grants_orbital_access_and_research() {
        let rules = base_ruleset();
        let [first, second, third] = rules.ages() else {
            panic!("expected three base ages");
        };
        assert_eq!(first.id(), "founding");
        assert_eq!(second.required_technologies(), &["tech_12"]);
        assert_eq!(third.required_technologies(), &["tech_20"]);
        assert_eq!(third.unlocks(), &["orbital_access"]);
        assert_eq!(
            rules
                .policy("policy_orbital_research")
                .unwrap()
                .required_age(),
            Some(third.id())
        );
        assert_eq!(
            rules
                .policies()
                .filter(|p| p.required_age().is_some())
                .count(),
            1
        );
    }
}
