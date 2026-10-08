#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSummary {
    pub is_plussed: bool,
    pub object_id: u32,
    pub account_id: u64,
    pub name: String,
    pub slot: u16,
}
