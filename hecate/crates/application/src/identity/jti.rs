//! The short call tokens already seen (research R3): each pair of `iss` and `jti` is accepted
//! once and kept while its token could still be accepted. A token issued before the start of the
//! process, minus the margin, is refused, so a restart only lets one of its last 30 s again.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use time::OffsetDateTime;

use crate::ports::CALL_TOKEN_MARGIN as MARGIN;

pub struct SeenTokens {
    started: OffsetDateTime,
    seen: Mutex<HashMap<(String, String), OffsetDateTime>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SeenTokens {
    pub fn new(started: OffsetDateTime) -> Self {
        Self {
            started,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Whether a token is accepted now: never twice with the same issuer and `jti` while it
    /// could still be accepted, and never one issued before the start minus the margin.
    pub fn accept(
        &self,
        issuer: &str,
        jti: &str,
        issued_at: OffsetDateTime,
        expires_at: OffsetDateTime,
        now: OffsetDateTime,
    ) -> bool {
        if issued_at < self.started - MARGIN {
            return false;
        }
        let mut seen = lock(&self.seen);
        let key = (issuer.to_string(), jti.to_string());
        if seen.get(&key).is_some_and(|until| *until > now) {
            return false;
        }
        seen.insert(key, expires_at + MARGIN);
        true
    }

    /// Drops the pairs whose token can no longer be accepted; the task of every minute calls it.
    pub fn purge(&self, now: OffsetDateTime) {
        lock(&self.seen).retain(|_, until| *until > now);
    }

    pub fn len(&self) -> usize {
        lock(&self.seen).len()
    }

    pub fn is_empty(&self) -> bool {
        lock(&self.seen).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;
    use time::macros::datetime;

    use super::*;

    const START: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);

    #[test]
    fn the_same_issuer_and_jti_are_accepted_once_until_the_token_expires() {
        let seen = SeenTokens::new(START);
        let (iat, exp) = (START, START + Duration::seconds(300));
        assert!(seen.accept("spiffe://cerbero.test/hermes", "a", iat, exp, START));
        assert!(!seen.accept("spiffe://cerbero.test/hermes", "a", iat, exp, START));
        assert!(seen.accept("spiffe://cerbero.test/hades", "a", iat, exp, START));
        assert!(seen.accept("spiffe://cerbero.test/hermes", "b", iat, exp, START));
        assert_eq!(seen.len(), 3);
        seen.purge(exp + MARGIN - Duration::seconds(1));
        assert_eq!(
            seen.len(),
            3,
            "kept while the token could still be accepted"
        );
        seen.purge(exp + MARGIN);
        assert_eq!(seen.len(), 0);
    }

    #[test]
    fn a_token_issued_before_the_start_minus_the_margin_is_refused() {
        let seen = SeenTokens::new(START);
        let exp = START + Duration::seconds(60);
        assert!(!seen.accept("i", "a", START - MARGIN - Duration::seconds(1), exp, START));
        assert!(seen.accept("i", "b", START - MARGIN, exp, START));
    }
}
