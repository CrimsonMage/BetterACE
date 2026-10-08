//! Pinned SentinelCommands ban family, retained across SQL and Audit admission.
use super::*;
use bace_admin::StaffOperation;
use bace_auth::{
    AccessLevel, AccountAdminRepository, AccountName, StaffPrincipal, VersionedAccount,
};
use bace_db_postgres::StoreError;
use bace_gameplay_api::staff::StaffAction;
use bace_persistence::{
    AccountBanChange, AccountBanListEntry, AccountBanOperation, AccountBanReceipt,
    AccountBanVerdict,
};

pub(in crate::game_runtime) struct BanPending {
    mode: BanMode,
    sudo: bool,
    now: i64,
    state: BanState,
    audit: VecDeque<String>,
    responses: Vec<String>,
    boot: Option<(SessionKey, bace_types::AccountId)>,
}

enum BanMode {
    Ban {
        name: String,
        duration: [String; 3],
        reason: Option<String>,
    },
    Unban {
        name: String,
    },
    List,
}

enum BanState {
    Lookup(Job<Result<Option<(VersionedAccount, AccountBanVerdict)>, String>>),
    Write(AccountBanOperation),
    Writing {
        operation: AccountBanOperation,
        job: Job<Result<AccountBanReceipt, StoreError>>,
    },
    Listing {
        cursor: Option<String>,
        rows: Vec<String>,
        job: Job<Result<Vec<AccountBanListEntry>, StoreError>>,
    },
    AuditSubmit,
    AuditAwait,
    Response,
    Failed,
}

impl GameRuntime {
    pub(in crate::game_runtime::staff) fn poll_staff_ban(&mut self) -> Result<(), String> {
        let Some(mut pending) = self.staff.pending.take() else {
            return Ok(());
        };
        if let Phase::Other {
            command, principal, ..
        } = &pending.phase
        {
            if !matches!(command.spec.name, "ban" | "unban" | "banlist") {
                self.staff.pending = Some(pending);
                return Ok(());
            }
            let operation = match bace_admin::prepare_staff_operation(command) {
                Ok(operation) => operation,
                Err(error) => {
                    return self.retain_ban_preparation_error(
                        pending,
                        format!("staff ban parameters: {error:?}"),
                    );
                }
            };
            let mode = match operation {
                Some(StaffOperation::Ban {
                    name,
                    duration,
                    reason,
                }) => BanMode::Ban {
                    name,
                    duration,
                    reason,
                },
                Some(StaffOperation::Unban { name }) => BanMode::Unban { name },
                Some(StaffOperation::BanList) => BanMode::List,
                _ => {
                    self.staff.pending = Some(pending);
                    return Ok(());
                }
            };
            if let Err(error) = self.validate_ban_issuer(&pending, *principal, command.sudo) {
                return self.retain_ban_preparation_error(pending, error);
            }
            let now = match self.ban_now() {
                Ok(now) => now,
                Err(error) => return self.retain_ban_preparation_error(pending, error),
            };
            let store = self.bootstrap.store.clone();
            let state = match &mode {
                BanMode::Ban { name, .. } | BanMode::Unban { name } => {
                    let lookup = AccountName::parse(name).ok();
                    BanState::Lookup(Box::pin(async move {
                        let Some(name) = lookup else {
                            return Ok(None);
                        };
                        let Some(account) = store
                            .account_for_admin(&name)
                            .await
                            .map_err(|error| error.to_string())?
                        else {
                            return Ok(None);
                        };
                        let verdict = store
                            .account_ban_verdict(account.account.id.0, now)
                            .await
                            .map_err(|error| error.to_string())?;
                        Ok(Some((account, verdict)))
                    }))
                }
                BanMode::List => BanState::Listing {
                    cursor: None,
                    rows: Vec::new(),
                    job: list_job(store, now, None),
                },
            };
            pending.phase = Phase::Ban(Box::new(BanPending {
                mode,
                sudo: command.sudo,
                now,
                state,
                audit: VecDeque::new(),
                responses: Vec::new(),
                boot: None,
            }));
        }
        let Phase::Ban(mut ban) = std::mem::replace(&mut pending.phase, Phase::Awaiting) else {
            self.staff.pending = Some(pending);
            return Ok(());
        };
        let now = ban.now;
        let state = std::mem::replace(&mut ban.state, BanState::Response);
        ban.state = match state {
            BanState::Lookup(mut job) => match poll(&mut job) {
                None => BanState::Lookup(job),
                Some(Err(error)) => {
                    ban.responses
                        .push(format!("Account ban lookup failed: {error}"));
                    BanState::Response
                }
                Some(Ok(account)) => {
                    let name = match &ban.mode {
                        BanMode::Ban { name, .. } | BanMode::Unban { name } => name,
                        BanMode::List => unreachable!(),
                    };
                    match account {
                        None => {
                            ban.responses.push(match ban.mode {
                                BanMode::Ban { .. } => format!("Cannot ban \"{name}\" because that account cannot be found in database. Check syntax/spelling and try again."),
                                _ => format!("Cannot unban \"{name}\" because that account cannot be found in database. Check spelling and try again."),
                            });
                            BanState::Response
                        }
                        Some((account, verdict)) => match &ban.mode {
                            BanMode::Ban {
                                duration, reason, ..
                            } => match parsed_duration(duration, now) {
                                Ok(expiry) if reason.as_ref().is_none_or(|r| r.len() <= 2048) => {
                                    BanState::Write(self.ban_operation(
                                        &pending,
                                        &account,
                                        AccountBanChange::Ban {
                                            started_unix_millis: now,
                                            expires_unix_millis: expiry,
                                            reason: reason.clone(),
                                        },
                                    ))
                                }
                                Ok(_) => {
                                    ban.responses
                                        .push("Ban reason exceeds the supported length.".into());
                                    BanState::Response
                                }
                                Err(message) => {
                                    ban.responses.push(message);
                                    BanState::Response
                                }
                            },
                            BanMode::Unban { .. } => {
                                if account.account.disabled
                                    || matches!(
                                        verdict,
                                        AccountBanVerdict::Banned(_)
                                            | AccountBanVerdict::Expired(_)
                                    )
                                {
                                    BanState::Write(self.ban_operation(
                                        &pending,
                                        &account,
                                        AccountBanChange::Unban,
                                    ))
                                } else {
                                    ban.responses.push(format!(
                                        "Cannot unban\"{name}\" because that account is not banned."
                                    ));
                                    BanState::Response
                                }
                            }
                            BanMode::List => unreachable!(),
                        },
                    }
                }
            },
            BanState::Write(operation) => {
                let store = self.bootstrap.store.clone();
                let request = operation.clone();
                BanState::Writing {
                    operation,
                    job: Box::pin(async move { store.apply_account_ban(&request).await }),
                }
            }
            BanState::Writing { operation, mut job } => match poll(&mut job) {
                None => BanState::Writing { operation, job },
                Some(Err(StoreError::CommitUncertain(error))) => {
                    ban.state = BanState::Write(operation);
                    return self.retain_ban_error(
                        pending,
                        ban,
                        format!("staff ban commit uncertain: {error}"),
                    );
                }
                Some(Err(error)) => {
                    ban.responses
                        .push(format!("Account ban command rejected: {error}"));
                    BanState::Response
                }
                Some(Ok(receipt)) => {
                    if receipt.operation_id != operation.operation_id
                        || receipt.account_id != operation.account_id
                        || receipt.account_revision <= operation.expected_revision
                    {
                        ban.state = BanState::Failed;
                        return self.retain_ban_error(
                            pending,
                            ban,
                            "staff ban receipt identity".into(),
                        );
                    }
                    self.prepare_committed_ban(&mut ban);
                    BanState::AuditSubmit
                }
            },
            BanState::Listing {
                cursor,
                mut rows,
                mut job,
            } => match poll(&mut job) {
                None => BanState::Listing { cursor, rows, job },
                Some(Err(error)) => {
                    ban.responses
                        .push(format!("Account ban list unavailable: {error}"));
                    BanState::Response
                }
                Some(Ok(page)) => {
                    let count = page.len();
                    if rows.len().saturating_add(count) > 4096 {
                        ban.state = BanState::Failed;
                        return self.retain_ban_error(
                            pending,
                            ban,
                            "staff banlist row capacity".into(),
                        );
                    }
                    let next = page.last().map(|entry| entry.canonical_name.clone());
                    let formatted = page
                        .iter()
                        .map(|entry| banlist_row(entry, self.assets.local_offset_seconds))
                        .collect::<Result<Vec<_>, _>>();
                    let formatted = match formatted {
                        Ok(formatted) => formatted,
                        Err(error) => {
                            ban.state = BanState::Failed;
                            return self.retain_ban_error(pending, ban, error);
                        }
                    };
                    rows.extend(formatted);
                    if count == 100 {
                        let cursor = next.expect("nonempty full banlist page");
                        BanState::Listing {
                            job: list_job(self.bootstrap.store.clone(), now, Some(cursor.clone())),
                            cursor: Some(cursor),
                            rows,
                        }
                    } else {
                        ban.responses = banlist_messages(rows);
                        BanState::Response
                    }
                }
            },
            BanState::AuditSubmit => {
                if ban.audit.is_empty() {
                    ban.state = BanState::Failed;
                    return self.retain_ban_error(
                        pending,
                        ban,
                        "staff ban audit text missing".into(),
                    );
                };
                let command = StaffCommand {
                    token: pending.token,
                    action: StaffAction::Audit {
                        context: pending.context,
                        texts: ban.audit.iter().cloned().collect(),
                        sudo: ban.sudo,
                    },
                };
                match self
                    .simulation
                    .input()
                    .try_submit(bace_simulation::Command::Staff(command))
                {
                    Ok(()) => BanState::AuditAwait,
                    Err(std::sync::mpsc::TrySendError::Full(_)) => BanState::AuditSubmit,
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                        ban.state = BanState::AuditSubmit;
                        return self.retain_ban_error(
                            pending,
                            ban,
                            "staff Audit owner closed".into(),
                        );
                    }
                }
            }
            BanState::AuditAwait => BanState::AuditAwait,
            BanState::Response => {
                if let Err(error) = self.publish_ban_response(&pending, &ban) {
                    ban.state = BanState::Response;
                    return self.retain_ban_error(pending, ban, error);
                }
                return Ok(());
            }
            BanState::Failed => BanState::Failed,
        };
        pending.phase = Phase::Ban(ban);
        self.staff.pending = Some(pending);
        Ok(())
    }

    fn retain_ban_error(
        &mut self,
        mut pending: Pending,
        ban: Box<BanPending>,
        error: String,
    ) -> Result<(), String> {
        pending.phase = Phase::Ban(ban);
        self.staff.pending = Some(pending);
        self.staff.failure = Some(error.clone());
        Err(error)
    }

    fn retain_ban_preparation_error(
        &mut self,
        pending: Pending,
        error: String,
    ) -> Result<(), String> {
        self.staff.pending = Some(pending);
        self.staff.failure = Some(error.clone());
        Err(error)
    }

    fn validate_ban_issuer(
        &mut self,
        pending: &Pending,
        principal: StaffPrincipal,
        sudo: bool,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get(&pending.key)
            .ok_or("staff ban issuer missing")?;
        let loading = session
            .loading
            .as_ref()
            .ok_or("staff ban issuer not entered")?;
        if session.terminated
            || session.disconnected
            || loading.loaded.binding.actor != pending.context.actor
            || loading.loaded.binding.account != pending.context.account
            || loading.loaded.binding.session != pending.context.session
            || session.account.access_level != principal.account_access
            || !principal.allows(AccessLevel::Sentinel, sudo)
            || self
                .players
                .replication(pending.context.actor)
                .is_none_or(|r| r.key != pending.key || r.binding != loading.loaded.binding)
        {
            return Err("staff ban issuer authorization changed".into());
        }
        Ok(())
    }

    fn ban_now(&self) -> Result<i64, String> {
        let elapsed =
            u64::try_from(self.last_elapsed.as_millis()).map_err(|_| "staff ban clock overflow")?;
        let now = self
            .clock
            .unix_millis
            .checked_add(elapsed)
            .ok_or("staff ban clock overflow")?;
        i64::try_from(now).map_err(|_| "staff ban clock overflow".into())
    }

    fn ban_operation(
        &self,
        pending: &Pending,
        account: &VersionedAccount,
        change: AccountBanChange,
    ) -> AccountBanOperation {
        let mut operation_id = [0; 16];
        operation_id[..8].copy_from_slice(&self.bootstrap.world_owner.epoch().to_le_bytes());
        operation_id[8..].copy_from_slice(&pending.token.to_le_bytes());
        AccountBanOperation {
            operation_id,
            account_id: account.account.id.0,
            expected_revision: account.revision,
            issuer_account_id: Some(pending.context.account.0),
            change,
        }
    }

    fn prepare_committed_ban(&self, ban: &mut BanPending) {
        match &ban.mode {
            BanMode::Ban {
                name,
                duration,
                reason,
            } => {
                let target = self.sessions.iter().find_map(|(key, session)| {
                    (session.connected
                        && !session.disconnected
                        && !session.terminated
                        && session.account.name.as_str().eq_ignore_ascii_case(name))
                    .then_some((*key, session.account.id))
                });
                if target.is_some() {
                    ban.responses.push(format!(
                        "Booting account {name}.{}",
                        reason
                            .as_ref()
                            .map_or(String::new(), |r| format!(" Reason: {r}"))
                    ));
                    ban.audit
                        .push_back(ban.responses.last().expect("boot response").clone());
                    ban.boot = target;
                }
                let text = format!(
                    "Banned account {name} for {} days, {} hours and {} minutes.{}",
                    duration[0],
                    duration[1],
                    duration[2],
                    reason
                        .as_ref()
                        .map_or(String::new(), |r| format!(" Reason: {r}"))
                );
                ban.responses.push(text.clone());
                ban.audit.push_back(text);
            }
            BanMode::Unban { name } => {
                let text = format!("UnBanned account {name}.");
                ban.responses.push(text.clone());
                ban.audit.push_back(text);
            }
            BanMode::List => unreachable!("banlist has no committed mutation"),
        }
    }

    pub(in crate::game_runtime::staff) fn accept_staff_ban_audit_outcome(
        &mut self,
        event: &StaffEvent,
    ) -> Result<bool, String> {
        let StaffEvent::Outcome {
            token,
            actor,
            result,
        } = event
        else {
            return Ok(false);
        };
        let Some(pending) = self.staff.pending.as_mut() else {
            return Ok(false);
        };
        let Phase::Ban(ban) = &mut pending.phase else {
            return Ok(false);
        };
        if !matches!(ban.state, BanState::AuditAwait)
            || *token != pending.token
            || *actor != Some(pending.context.actor)
        {
            return Err("staff ban Audit outcome correlation".into());
        }
        match result {
            Ok(()) => {
                if ban.audit.is_empty() {
                    return Err("staff ban Audit receipt without record".into());
                }
                ban.audit.clear();
                ban.state = BanState::Response;
            }
            Err(error) => {
                ban.state = BanState::AuditSubmit;
                self.staff.failure = Some(format!("staff ban Audit rejected: {error:?}"));
            }
        }
        Ok(true)
    }

    fn publish_ban_response(&mut self, pending: &Pending, ban: &BanPending) -> Result<(), String> {
        let mut commands = Vec::new();
        let issuer_disconnected = self
            .sessions
            .get(&pending.key)
            .is_none_or(|session| session.disconnected || session.terminated || !session.connected);
        if !issuer_disconnected {
            let replica = self
                .players
                .replication(pending.context.actor)
                .ok_or("staff ban recipient missing")?;
            if replica.key != pending.key
                || replica.binding.account != pending.context.account
                || replica.binding.session != pending.context.session
            {
                return Err("staff ban recipient changed".into());
            }
            for text in &ban.responses {
                let batch = bace_replication::project_staff_response(
                    replica.binding,
                    text,
                    0,
                    bace_replication::BatchLimits {
                        max_messages: 1,
                        max_bytes: self.limits.message_bytes,
                        max_message_bytes: self.limits.message_bytes,
                        max_string_bytes: 4096,
                    },
                )
                .map_err(|error| format!("staff ban response: {error:?}"))?;
                commands.push(
                    crate::game_messages::session_batch_command(replica.key, batch)
                        .map_err(|error| error.to_string())?,
                );
            }
        }
        let boot = if let Some((target, _account)) = ban.boot.filter(|(key, account)| {
            self.sessions.get(key).is_some_and(|session| {
                session.connected
                    && !session.disconnected
                    && !session.terminated
                    && session.account.id == *account
            })
        }) {
            let reason = match &ban.mode {
                BanMode::Ban { reason, .. } => reason.as_deref(),
                _ => None,
            };
            let reason = reason.map_or(String::new(), |r| format!(" - {r}"));
            Some((
                target,
                bace_wire::AccountControl::Boot {
                    reason: Some(&reason),
                }
                .encode()
                .map_err(|error| format!("staff ban boot packet: {error:?}"))?,
            ))
        } else {
            None
        };
        if self
            .staff
            .output
            .len()
            .saturating_add(commands.len())
            .saturating_add(usize::from(boot.is_some()))
            > self.limits.messages
        {
            return Err("staff ban output capacity".into());
        }
        if let Some((target, bytes)) = boot {
            // Source HandleBoot private result precedes its target termination;
            // final ban result follows. Audit events were already retained.
            if !commands.is_empty() {
                let first = commands.remove(0);
                self.staff.output.push_back(first);
            }
            self.staff
                .output
                .push_back(NetworkCommand::TerminateAfterFlush {
                    key: target,
                    queue: 9,
                    bytes,
                });
            if let Some(session) = self.sessions.get_mut(&target) {
                session.terminated = true;
                session.closing = true;
            }
        }
        self.staff.output.extend(commands);
        Ok(())
    }
}

fn poll<T>(job: &mut Job<T>) -> Option<T> {
    match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

fn list_job(
    store: bace_db_postgres::PgStore,
    now: i64,
    cursor: Option<String>,
) -> Job<Result<Vec<AccountBanListEntry>, StoreError>> {
    Box::pin(async move {
        store
            .list_active_account_bans(now, cursor.as_deref(), 100)
            .await
    })
}

fn parsed_duration(duration: &[String; 3], now: i64) -> Result<i64, String> {
    let mut milliseconds = 0.0;
    for (index, factor) in [86_400_000.0, 3_600_000.0, 60_000.0]
        .into_iter()
        .enumerate()
    {
        let value: f64 = duration[index].parse().map_err(|_| {
            format!(
                "{} must not be less than 0.",
                ["Days", "Hours", "Minutes"][index]
            )
        })?;
        if !value.is_finite() || value < 0.0 {
            return Err(format!(
                "{} must not be less than 0.",
                ["Days", "Hours", "Minutes"][index]
            ));
        }
        milliseconds += value * factor;
    }
    if !milliseconds.is_finite() || milliseconds > (i64::MAX - now) as f64 {
        return Err("Ban duration exceeds the supported date range.".into());
    }
    now.checked_add(milliseconds.round() as i64)
        .ok_or("Ban duration exceeds the supported date range.".into())
}

fn banlist_messages(rows: Vec<String>) -> Vec<String> {
    if rows.is_empty() {
        return vec!["There are no accounts currently banned.".into()];
    }
    let mut messages = Vec::new();
    let mut current = "The following accounts are banned:\n-------------------\n".to_owned();
    for row in rows {
        if current.len() + row.len() + 1 > 4096 {
            messages.push(current);
            current = String::new();
        }
        current.push_str(&row);
        current.push('\n');
    }
    if !current.is_empty() {
        messages.push(current);
    }
    messages
}

fn banlist_row(entry: &AccountBanListEntry, offset_seconds: i32) -> Result<String, String> {
    let issuer = match (&entry.issuer_canonical_name, entry.ban.issuer_account_id) {
        (Some(name), Some(_)) => format!("account {name}"),
        (None, None) => "CONSOLE".into(),
        (None, Some(_)) => "account <removed>".into(),
        (Some(_), None) => return Err("staff banlist issuer mismatch".into()),
    };
    let local = entry
        .ban
        .expires_unix_millis
        .checked_div(1000)
        .and_then(|seconds| seconds.checked_add(i64::from(offset_seconds)))
        .ok_or("staff banlist date overflow")?;
    let stamp = source_ban_date(local)?;
    let reason = entry
        .ban
        .reason
        .as_deref()
        .map_or(String::new(), |reason| format!(" -- Reason: {reason}"));
    Ok(format!(
        "{} -- banned by {issuer} until server time {stamp}{reason}",
        entry.canonical_name
    ))
}

fn source_ban_date(seconds: i64) -> Result<String, String> {
    let days = seconds.div_euclid(86_400);
    let second = seconds.rem_euclid(86_400);
    let z = days
        .checked_add(719_468)
        .ok_or("staff banlist date overflow")?;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    if !(1..=9999).contains(&year) {
        return Err("staff banlist date range".into());
    }
    let hour = second / 3600;
    let minute = second / 60 % 60;
    let twelve = (hour + 11) % 12 + 1;
    let ampm = if hour < 12 { "AM" } else { "PM" };
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][usize::try_from(month - 1).map_err(|_| "staff banlist month")?];
    Ok(format!(
        "{month} {day:02} {year:04}  {twelve}:{minute:02}{ampm}"
    ))
}

#[cfg(test)]
mod tests;
