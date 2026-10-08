//! ACE ContractManager registry membership. Status/quest timing is resolved from
//! the prepared DAT contract and current QuestRegistry at projection time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractState {
    pub id: u32,
    pub display: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractRegistry {
    entries: Vec<ContractState>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractChange {
    pub before: Vec<ContractState>,
    pub after: Vec<ContractState>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    Invalid,
    Full,
    MissingDefinition,
    Conflict,
}
impl ContractRegistry {
    pub fn restore(entries: Vec<ContractState>) -> Result<Self, ContractError> {
        if entries.len() > 100
            || entries.iter().any(|e| e.id == 0)
            || entries.windows(2).any(|e| e[0].id >= e[1].id)
            || entries.iter().filter(|e| e.display).count() > 1
        {
            return Err(ContractError::Invalid);
        }
        Ok(Self { entries })
    }
    pub fn entries(&self) -> &[ContractState] {
        &self.entries
    }
    pub fn full(&self) -> bool {
        self.entries.len() >= 100
    }
    pub fn propose(
        &self,
        id: u32,
        add: bool,
        definition_present: bool,
    ) -> Result<ContractChange, ContractError> {
        if id == 0 {
            return Err(ContractError::Invalid);
        }
        if add && !definition_present {
            return Err(ContractError::MissingDefinition);
        }
        if add && self.full() {
            return Err(ContractError::Full);
        }
        let mut after = self.entries.clone();
        match (after.binary_search_by_key(&id, |e| e.id), add) {
            (Err(index), true) => after.insert(index, ContractState { id, display: false }),
            (Ok(index), false) => {
                after.remove(index);
            }
            _ => {}
        }
        Ok(ContractChange {
            before: self.entries.clone(),
            after,
        })
    }
    pub fn adopt(&mut self, change: ContractChange) -> Result<(), ContractError> {
        if self.entries != change.before {
            return Err(ContractError::Conflict);
        }
        let next = Self::restore(change.after)?;
        *self = next;
        Ok(())
    }
}
