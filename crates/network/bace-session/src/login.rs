use bace_wire::{LoginCredential, LoginRequest, NetAuthType};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginRejection {
    WrongClientVersion,
    InvalidAccount,
    UnsupportedAuthentication,
    InvalidPassword,
}
/// Borrowed credentials are not an authenticated identity. The caller MUST
/// resolve the account, enforce disabled/access policy and verify its password
/// before beginning the connection challenge.
pub struct PasswordLogin<'a> {
    pub account: &'a str,
    pub password: &'a str,
}
impl std::fmt::Debug for PasswordLogin<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordLogin")
            .field("account", &self.account)
            .field("password", &"[redacted]")
            .finish()
    }
}
pub fn validate_password_login(
    request: &LoginRequest,
) -> Result<PasswordLogin<'_>, LoginRejection> {
    if request.client_version != "1802" {
        return Err(LoginRejection::WrongClientVersion);
    }
    if request.account.is_empty()
        || request.account.encode_utf16().count() > 50
        || request.account.chars().any(char::is_control)
    {
        return Err(LoginRejection::InvalidAccount);
    }
    let (NetAuthType::AccountPassword, LoginCredential::Password(password)) =
        (&request.net_auth_type, &request.credential)
    else {
        return Err(LoginRejection::UnsupportedAuthentication);
    };
    if password.is_empty() || password.len() > 1024 {
        return Err(LoginRejection::InvalidPassword);
    }
    // auth_flags/account_to_login_as grant no rights. Like official ACE, the
    // account name in the login payload identifies the account to authenticate.
    Ok(PasswordLogin {
        account: &request.account,
        password,
    })
}
