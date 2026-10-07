use crate::SkillAdvancement;

/// Named fields prevent confusing the creation payload's Coordination/Quickness
/// order with the different PropertyAttribute numeric ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationAttributes {
    pub strength: u32,
    pub endurance: u32,
    pub coordination: u32,
    pub quickness: u32,
    pub focus: u32,
    pub self_attribute: u32,
}

impl CreationAttributes {
    pub fn wire_order(self) -> [u32; 6] {
        [
            self.strength,
            self.endurance,
            self.coordination,
            self.quickness,
            self.focus,
            self.self_attribute,
        ]
    }
}

/// Allocation subset of CharacterCreateInfo. Appearance/name/heritage selection
/// and durable character creation are separate validations; this is not a create
/// request whose successful validation authorizes world entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationAllocation {
    pub attributes: CreationAttributes,
    /// Pinned ACE expects exactly 55 entries, indexed by the wire skill ID.
    pub skills: [SkillAdvancement; 55],
}
