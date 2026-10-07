use crate::HostConsole;
use axum::http::HeaderMap;
use rand_core::{OsRng, RngCore};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

pub(crate) struct Session {
    pub csrf: String,
    created: Instant,
    seen: Instant,
}
#[derive(Default)]
pub(crate) struct AuthState {
    sessions: HashMap<String, Session>,
    attempts: VecDeque<Instant>,
}
fn secret() -> Option<String> {
    let mut bytes = [0u8; 32];
    OsRng.try_fill_bytes(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
impl HostConsole {
    pub(crate) fn authority_valid(&self, headers: &HeaderMap) -> bool {
        headers.get("host").and_then(|value| value.to_str().ok()) == Some(&self.inner.authority)
    }
    pub(crate) fn origin_valid(&self, headers: &HeaderMap) -> bool {
        headers.get("origin").and_then(|value| value.to_str().ok()) == Some(&self.inner.origin)
    }
    pub(crate) fn admit_login(&self) -> bool {
        let mut auth = self.inner.auth.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        while auth
            .attempts
            .front()
            .is_some_and(|time| now.duration_since(*time) >= Duration::from_secs(60))
        {
            auth.attempts.pop_front();
        }
        if auth.attempts.len() >= 5 {
            return false;
        }
        auth.attempts.push_back(now);
        true
    }
    pub(crate) fn new_session(&self) -> Option<(String, String)> {
        let mut auth = self.inner.auth.lock().unwrap_or_else(|e| e.into_inner());
        self.expire(&mut auth);
        if auth.sessions.len() >= self.inner.config.max_sessions {
            return None;
        }
        let token = secret()?;
        let csrf = secret()?;
        auth.sessions.insert(
            token.clone(),
            Session {
                csrf: csrf.clone(),
                created: Instant::now(),
                seen: Instant::now(),
            },
        );
        Some((token, csrf))
    }
    pub(crate) fn session(&self, headers: &HeaderMap, mutation: bool) -> Option<String> {
        if !self.authority_valid(headers) || (mutation && !self.origin_valid(headers)) {
            return None;
        }
        let token = cookie(headers)?;
        let mut auth = self.inner.auth.lock().unwrap_or_else(|e| e.into_inner());
        self.expire(&mut auth);
        let session = auth.sessions.get_mut(token)?;
        if mutation
            && headers
                .get("x-csrf-token")
                .and_then(|value| value.to_str().ok())
                != Some(&session.csrf)
        {
            return None;
        }
        session.seen = Instant::now();
        Some(session.csrf.clone())
    }
    pub(crate) fn logout(&self, headers: &HeaderMap) {
        if let Some(token) = cookie(headers) {
            self.inner
                .auth
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .sessions
                .remove(token);
        }
    }
    fn expire(&self, auth: &mut AuthState) {
        let now = Instant::now();
        auth.sessions.retain(|_, session| {
            now.duration_since(session.seen).as_secs() < self.inner.config.session_idle_seconds
                && now.duration_since(session.created).as_secs()
                    < self.inner.config.session_lifetime_seconds
        });
    }
}
fn cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|item| item.trim().strip_prefix("bace_host="))
}
