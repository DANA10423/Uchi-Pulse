use std::collections::HashMap;
use std::fmt;

use uchi_pulse_common::{ActionId, StateType, StateValue, TargetType};

use crate::db::{ActionRecord, ActionStateChangeRecord, Database, DatabaseError};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ActionStateScope {
    Common,
    Family(u32),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActionStateKey {
    pub scope: ActionStateScope,
    pub state_type: StateType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateApplyError {
    DuplicateStateType(StateType),
}

impl fmt::Display for StateApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateStateType(state_type) => {
                write!(formatter, "duplicate state change for {state_type:?}")
            }
        }
    }
}

impl std::error::Error for StateApplyError {}

/// In-memory business-state store. Communication status is deliberately kept
/// in CommunicationStateManager and is not represented here.
#[derive(Clone, Debug, Default)]
pub struct ActionStateManager {
    values: HashMap<ActionStateKey, StateValue>,
}

impl ActionStateManager {
    pub fn get(&self, scope: ActionStateScope, state_type: StateType) -> Option<StateValue> {
        self.values
            .get(&ActionStateKey { scope, state_type })
            .copied()
    }

    pub fn values(&self) -> impl Iterator<Item = (&ActionStateKey, &StateValue)> {
        self.values.iter()
    }

    /// Validates and stages every change before replacing the live map. This
    /// keeps a multi-change Action from becoming partially applied.
    pub fn apply_batch(
        &mut self,
        scope: ActionStateScope,
        changes: &[ActionStateChangeRecord],
    ) -> Result<(), StateApplyError> {
        let mut staged = self.values.clone();
        let mut seen = Vec::with_capacity(changes.len());
        for change in changes {
            if seen.contains(&change.state_type) {
                return Err(StateApplyError::DuplicateStateType(change.state_type));
            }
            seen.push(change.state_type);
            staged.insert(
                ActionStateKey {
                    scope,
                    state_type: change.state_type,
                },
                change.state_value,
            );
        }
        self.values = staged;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedAction {
    pub action: ActionRecord,
    pub scope: ActionStateScope,
    pub state_changes: Vec<ActionStateChangeRecord>,
}

#[derive(Debug)]
pub enum ActionError {
    NotFound(ActionId),
    Disabled(ActionId),
    FamilyTargetMissing(ActionId),
    CommonTargetHasFamily(ActionId),
    TargetFamilyNotFound { action_id: ActionId, family_id: u32 },
    Database(DatabaseError),
    State(StateApplyError),
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(action_id) => write!(formatter, "Action {action_id} does not exist"),
            Self::Disabled(action_id) => write!(formatter, "Action {action_id} is disabled"),
            Self::FamilyTargetMissing(action_id) => {
                write!(formatter, "FAMILY Action {action_id} has no target family")
            }
            Self::CommonTargetHasFamily(action_id) => {
                write!(formatter, "COMMON Action {action_id} has a target family")
            }
            Self::TargetFamilyNotFound {
                action_id,
                family_id,
            } => write!(
                formatter,
                "Action {action_id} targets missing family {family_id}"
            ),
            Self::Database(error) => write!(formatter, "Action database operation failed: {error}"),
            Self::State(error) => write!(formatter, "Action state application failed: {error}"),
        }
    }
}

impl std::error::Error for ActionError {}

impl From<DatabaseError> for ActionError {
    fn from(error: DatabaseError) -> Self {
        Self::Database(error)
    }
}

pub struct ActionEngine {
    database: Database,
    state: ActionStateManager,
}

impl ActionEngine {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            state: ActionStateManager::default(),
        }
    }

    pub fn state(&self) -> &ActionStateManager {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut ActionStateManager {
        &mut self.state
    }

    /// Performs all pre-acceptance checks. The UDP processor calls this before
    /// writing the EVENT history, so an unavailable Action is never accepted.
    pub fn validate_event(&self, action_id: ActionId) -> Result<ValidatedAction, ActionError> {
        let action = self
            .database
            .get_action(action_id)?
            .ok_or(ActionError::NotFound(action_id))?;
        if !action.enabled {
            return Err(ActionError::Disabled(action_id));
        }

        let scope = self.validate_target(&action)?;
        let state_changes = self.database.list_action_state_changes(action_id)?;
        Ok(ValidatedAction {
            action,
            scope,
            state_changes,
        })
    }

    fn validate_target(&self, action: &ActionRecord) -> Result<ActionStateScope, ActionError> {
        match (action.target_type, action.target_family_id) {
            (TargetType::Family, Some(family_id)) => {
                if self.database.get_family(family_id)?.is_none() {
                    return Err(ActionError::TargetFamilyNotFound {
                        action_id: action.action_id,
                        family_id,
                    });
                }
                Ok(ActionStateScope::Family(family_id))
            }
            (TargetType::Family, None) => Err(ActionError::FamilyTargetMissing(action.action_id)),
            (TargetType::Common, Some(_)) => {
                Err(ActionError::CommonTargetHasFamily(action.action_id))
            }
            (TargetType::Common, None) => Ok(ActionStateScope::Common),
        }
    }

    /// Applies the already validated Action atomically. The Action is loaded
    /// again so runtime database changes become an execution failure rather
    /// than an implicit partial application.
    pub fn execute(&mut self, action_id: ActionId) -> Result<ValidatedAction, ActionError> {
        let plan = self.validate_event(action_id)?;
        self.state
            .apply_batch(plan.scope, &plan.state_changes)
            .map_err(ActionError::State)?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::FamilyRecord;

    fn database() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn family_database() -> Database {
        let database = database();
        database
            .insert_family(&FamilyRecord {
                family_id: 1,
                display_name: "家族".into(),
                enabled: true,
            })
            .unwrap();
        database
    }

    fn action(
        action_id: ActionId,
        target_type: TargetType,
        target_family_id: Option<u32>,
    ) -> ActionRecord {
        ActionRecord {
            action_id,
            action_name: format!("action-{action_id}"),
            target_type,
            target_family_id,
            web_message: None,
            enabled: true,
        }
    }

    #[test]
    fn validates_enabled_actions_and_targets() {
        let database = family_database();
        database
            .insert_action(&action(1, TargetType::Family, Some(1)))
            .unwrap();
        database
            .insert_action(&action(2, TargetType::Common, None))
            .unwrap();
        let engine = ActionEngine::new(database);

        assert_eq!(
            engine.validate_event(1).unwrap().scope,
            ActionStateScope::Family(1)
        );
        assert_eq!(
            engine.validate_event(2).unwrap().scope,
            ActionStateScope::Common
        );
        assert!(matches!(
            engine.validate_event(3),
            Err(ActionError::NotFound(3))
        ));
    }

    #[test]
    fn state_changes_are_applied_as_an_atomic_batch() {
        let mut state = ActionStateManager::default();
        let changes = [
            ActionStateChangeRecord {
                action_id: 1,
                state_type: StateType::MealNotice,
                state_value: StateValue::On,
            },
            ActionStateChangeRecord {
                action_id: 1,
                state_type: StateType::MealNotice,
                state_value: StateValue::Off,
            },
        ];
        assert!(matches!(
            state.apply_batch(ActionStateScope::Family(1), &changes),
            Err(StateApplyError::DuplicateStateType(StateType::MealNotice))
        ));
        assert_eq!(
            state.get(ActionStateScope::Family(1), StateType::MealNotice),
            None
        );
    }

    #[test]
    fn family_and_common_target_invariants_are_rejected_even_before_db_constraints() {
        let database = family_database();
        let engine = ActionEngine::new(database);
        assert!(matches!(
            engine.validate_target(&action(10, TargetType::Family, None)),
            Err(ActionError::FamilyTargetMissing(10))
        ));
        assert!(matches!(
            engine.validate_target(&action(11, TargetType::Common, Some(1))),
            Err(ActionError::CommonTargetHasFamily(11))
        ));
    }
}
