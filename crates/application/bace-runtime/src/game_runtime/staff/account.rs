//! One retained account operation, with exact journal retry after uncertainty.
use super::*;
use crate::staff_commands::{
    PendingStaffAccount, PreparedStaffAccount, StaffAccountService, StaffCommandIdentity,
};
pub(in crate::game_runtime) struct AccountPending {
    kind: &'static str,
    phase: AccountPhase,
}
enum AccountPhase {
    Preparing(Job<Result<PreparedStaffAccount, crate::staff_commands::StaffCommandError>>),
    Write(Option<PendingStaffAccount>),
    Writing(
        Job<(
            PendingStaffAccount,
            Result<bace_auth::AccountAdminReceipt, crate::staff_commands::StaffCommandError>,
        )>,
    ),
    Response(String),
}
impl GameRuntime {
    pub(in crate::game_runtime::staff) fn poll_staff_account(&mut self) -> Result<(), String> {
        let Some(p) = self.staff.pending.as_mut() else {
            return Ok(());
        };
        if let Phase::Other {
            line,
            command,
            principal,
        } = &p.phase
        {
            if !matches!(
                bace_admin::prepare_staff_operation(command)
                    .map_err(|e| format!("staff account parameters: {e:?}"))?,
                Some(bace_admin::StaffOperation::Account(_))
            ) {
                return Ok(());
            }
            let kind = command.spec.name;
            if kind == "passwd" {
                if self.staff.last_password.get(&p.key).is_some_and(|last| {
                    self.last_elapsed.saturating_sub(*last) < Duration::from_secs(5)
                }) {
                    p.phase = Phase::Account(Box::new(AccountPending {
                        kind,
                        phase: AccountPhase::Response(
                            "This command may only be run once every 5 seconds.".into(),
                        ),
                    }));
                    return Ok(());
                }
                self.staff.last_password.insert(p.key, self.last_elapsed);
            }
            let session = self
                .sessions
                .get(&p.key)
                .ok_or("staff account session missing")?;
            let identity = StaffCommandIdentity::Game {
                account: p.context.account,
                name: session.account.name.clone(),
                principal: *principal,
            };
            let service = StaffAccountService::new(
                self.bootstrap.store.clone(),
                crate::authentication::PasswordExecutor::new(1).map_err(|e| e.to_string())?,
                1,
                bace_auth::AccessLevel::Player,
            )
            .map_err(|e| e.to_string())?;
            let line = line.clone();
            let mut id = [0; 16];
            id[..8].copy_from_slice(&self.bootstrap.world_owner.epoch().to_le_bytes());
            id[8..].copy_from_slice(&p.token.to_le_bytes());
            p.phase = Phase::Account(Box::new(AccountPending {
                kind,
                phase: AccountPhase::Preparing(Box::pin(async move {
                    service.prepare(&identity, &line, id).await
                })),
            }));
        }
        let Phase::Account(account) = &mut p.phase else {
            return Ok(());
        };
        match &mut account.phase {
            AccountPhase::Preparing(job) => {
                if let Some(result) = poll(job) {
                    account.phase = match result {
                        Ok(PreparedStaffAccount::Read(view)) => AccountPhase::Response(format!(
                            "User: {}, ID: {}",
                            view.name.as_str(),
                            view.account.0
                        )),
                        Ok(PreparedStaffAccount::Write(write)) => AccountPhase::Write(Some(write)),
                        Err(error) => {
                            AccountPhase::Response(format!("Account command rejected: {error}"))
                        }
                    };
                }
            }
            AccountPhase::Write(write) => {
                let mut write = write.take().ok_or("staff account operation missing")?;
                account.phase = AccountPhase::Writing(Box::pin(async move {
                    let result = write.commit().await;
                    (write, result)
                }));
            }
            AccountPhase::Writing(job) => {
                if let Some((write, result)) = poll(job) {
                    match result {
                        Ok(receipt) => {
                            account.phase = AccountPhase::Response(success(account.kind, &receipt))
                        }
                        Err(error) if write.uncertain() => {
                            account.phase = AccountPhase::Write(Some(write));
                            self.staff.failure =
                                Some(format!("staff account commit uncertain: {error}"));
                        }
                        Err(error) => {
                            account.phase =
                                AccountPhase::Response(format!("Account command rejected: {error}"))
                        }
                    }
                }
            }
            AccountPhase::Response(text) => {
                let r = self
                    .players
                    .replication(p.context.actor)
                    .ok_or("staff account recipient missing")?;
                let limits = bace_replication::BatchLimits {
                    max_messages: self.limits.messages,
                    max_bytes: self.limits.message_bytes,
                    max_message_bytes: self.limits.message_bytes,
                    max_string_bytes: 4096,
                };
                let batch = bace_replication::project_staff_response(
                    r.binding,
                    text,
                    if account.kind == "accountcreate" {
                        20
                    } else {
                        0
                    },
                    limits,
                )
                .map_err(|e| format!("staff account response: {e:?}"))?;
                self.staff.output.push_back(
                    crate::game_messages::session_batch_command(r.key, batch)
                        .map_err(|e| e.to_string())?,
                );
                self.staff.pending = None;
            }
        }
        Ok(())
    }
}
fn poll<T>(job: &mut Job<T>) -> Option<T> {
    match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}
fn success(kind: &str, r: &bace_auth::AccountAdminReceipt) -> String {
    let article = if matches!(
        r.access,
        bace_auth::AccessLevel::Advocate
            | bace_auth::AccessLevel::Admin
            | bace_auth::AccessLevel::Envoy
    ) {
        "an"
    } else {
        "a"
    };
    match kind {
        "accountcreate" => format!(
            "Account successfully created for {} ({}) with access rights as {article} {:?}.",
            r.name.as_str(),
            r.account.0,
            r.access
        ),
        "set-accountaccess" => format!(
            "Account {} updated with access rights set as {article} {:?}.",
            r.name.as_str(),
            r.access
        ),
        "set-accountpassword" => format!(
            "Account password for {} successfully changed.",
            r.name.as_str()
        ),
        "passwd" => "Account password successfully changed.".into(),
        _ => "Account operation committed.".into(),
    }
}

#[cfg(test)]
mod tests;
