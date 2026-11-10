//! The credentials of a call to the API (research R1, R3): the certificates of its connection,
//! which TLS only checked for possession, and the short call token of `Authorization: Bearer`.
//! The use case validates them (the chain up to the current root CA at the time of the clock,
//! that Hécate issued the certificate, and the token, once), so a failure is `auth` with the row
//! of its operation.

use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use hecate_application::identity::service::Credentials;

use crate::api::server::PeerCertificates;

const BEARER: &str = "Bearer ";

/// What a call brings to authenticate itself; it never refuses by itself.
pub struct CallCredentials(pub Credentials);

impl<S: Send + Sync> FromRequestParts<S> for CallCredentials {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let certificates = parts
            .extensions
            .get::<PeerCertificates>()
            .map(|peer| {
                peer.0
                    .iter()
                    .map(|certificate| certificate.to_vec())
                    .collect()
            })
            .unwrap_or_default();
        let bearer = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix(BEARER))
            .map(str::to_string);
        Ok(Self(Credentials {
            certificates,
            bearer,
        }))
    }
}
