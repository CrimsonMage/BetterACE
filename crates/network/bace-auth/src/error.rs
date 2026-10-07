#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    InvalidAccountName,
    InvalidPasswordLength,
    InvalidPasswordHash,
    HashCostLimit,
    Busy,
    InvalidConcurrencyLimit,
    RandomUnavailable,
    HashFailure,
    InvalidAccessLevel,
}
impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authentication error: {self:?}")
    }
}
impl std::error::Error for AuthError {}
