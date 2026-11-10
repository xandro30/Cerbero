//! The short call token, verified with jsonwebtoken and EdDSA only (research R3). The library
//! checks the algorithm and the signature with the registered key; the times, the issuer, the
//! audience and the `jti` are checked here, against the time of the `Clock` port, so the tests
//! move it with `FakeClock`.

use hecate_application::ports::{
    CALL_TOKEN_MARGIN as MARGIN, CALL_TOKEN_MAX_LIFETIME as MAX_LIFETIME, CallTokenClaims,
    CallTokenVerifier,
};
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::names::SpiffeId;
use hecate_domain::sealing::errors::HecateError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};

const MAX_JTI_LEN: usize = 128;
const NOT_VALID: &str = "the call token is not valid";

/// The claims that Hécate reads; others are ignored, as RFC 7519 asks.
#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    iat: i64,
    exp: i64,
    jti: String,
}

fn refused() -> HecateError {
    HecateError::auth(NOT_VALID)
}

fn instant(seconds: i64) -> Result<OffsetDateTime, HecateError> {
    OffsetDateTime::from_unix_timestamp(seconds).map_err(|_| refused())
}

#[derive(Debug, Default)]
pub struct JwtCallTokenVerifier;

impl JwtCallTokenVerifier {
    pub fn new() -> Self {
        Self
    }
}

impl CallTokenVerifier for JwtCallTokenVerifier {
    fn verify(
        &self,
        token: &str,
        key: &CallKey,
        issuer: &SpiffeId,
        audience: &SpiffeId,
        now: OffsetDateTime,
    ) -> Result<CallTokenClaims, HecateError> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.required_spec_claims.clear();
        validation.validate_exp = false;
        validation.validate_nbf = false;
        validation.validate_aud = false;
        let decoding = DecodingKey::from_ed_components(&key.x()).map_err(|_| refused())?;
        let claims = jsonwebtoken::decode::<Claims>(token, &decoding, &validation)
            .map_err(|_| refused())?
            .claims;
        if claims.iss != issuer.to_string() || claims.aud != audience.to_string() {
            return Err(refused());
        }
        let (issued_at, expires_at) = (instant(claims.iat)?, instant(claims.exp)?);
        let lifetime = expires_at - issued_at;
        if lifetime < Duration::ZERO || lifetime > MAX_LIFETIME {
            return Err(refused());
        }
        if now + MARGIN < issued_at || now >= expires_at + MARGIN {
            return Err(refused());
        }
        let jti_valid = (1..=MAX_JTI_LEN).contains(&claims.jti.len())
            && claims.jti.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
        if !jti_valid {
            return Err(refused());
        }
        Ok(CallTokenClaims {
            issuer: issuer.clone(),
            audience: audience.clone(),
            issued_at,
            expires_at,
            jti: claims.jti,
        })
    }
}
