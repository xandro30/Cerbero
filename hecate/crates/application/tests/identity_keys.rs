//! T081: the published call keys (FR-016, FR-017, FR-024, research R5): the list of the keys
//! valid for new calls, valid 300 s, and any registered key by its `kid`.

mod common;

use common::*;
use hecate_application::identity::service::Credentials;
use hecate_application::ports::{Clock, IdentityStore};
use hecate_application::testing::CaRig;
use hecate_domain::events::{CertificateEvent, IdentityEvent, RevocationReason, RevokedBy};
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::errors::HecateError;
use time::Duration;

fn kid_of(rig: &CaRig, component: &Component) -> String {
    current_key(rig, component).kid()
}

fn current_key(rig: &CaRig, component: &Component) -> CallKey {
    rig.pki
        .read_call_key(&component.inner.call_key().unwrap())
        .unwrap()
}

fn listed(rig: &CaRig, caller: &Component) -> Vec<(String, String)> {
    let mut keys: Vec<(String, String)> = rig
        .identities
        .keys(&caller.credentials(None))
        .unwrap()
        .keys
        .iter()
        .map(|key| (key.identity.identity().to_string(), key.key.kid()))
        .collect();
    keys.sort();
    keys
}

fn is_auth(error: &hecate_application::errors::ServiceError) -> bool {
    matches!(
        error,
        hecate_application::errors::ServiceError::Hecate(HecateError::AuthenticationFailed(_))
    )
}

fn revoke_urgently(rig: &CaRig, identity: &str, component: &Component) {
    let stream = format!("certificate/{identity}/{}", component.serial());
    let event = CertificateEvent::CertificateRevoked {
        at: rig.clock.now(),
        urgent: true,
        by: RevokedBy::Admin,
        reason: RevocationReason::Admin,
    };
    rig.store
        .transaction(&mut |writes| {
            let version = writes.load(&stream)?.len() as u64;
            writes.append(&stream, &[event.encode()], version)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn the_list_has_only_the_keys_of_valid_certificates_of_enabled_identities() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let hades = Component::enrolled(&rig, "hades");
    let published = rig.identities.keys(&hermes.credentials(None)).unwrap();
    assert_eq!(published.keys.len(), 2);
    assert!(published.keys.iter().all(|key| key.active));
    let hermes_key = published
        .keys
        .iter()
        .find(|key| key.identity == spiffe("hermes"))
        .unwrap();
    assert_eq!(hermes_key.key, current_key(&rig, &hermes));
    assert_eq!(hermes_key.not_after, rig.clock.now() + Duration::hours(72));
    assert_eq!(
        listed(&rig, &hermes),
        vec![
            ("hades".to_string(), kid_of(&rig, &hades)),
            ("hermes".to_string(), kid_of(&rig, &hermes)),
        ]
    );
}

#[test]
fn after_renewing_the_new_key_and_the_previous_one_are_valid_until_its_certificate_expires() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let old_kid = kid_of(&rig, &hermes);
    rig.clock.advance(Duration::hours(48));
    hermes.renewed(&rig);
    let new_kid = kid_of(&rig, &hermes);
    let mut both = vec![
        ("hermes".to_string(), old_kid.clone()),
        ("hermes".to_string(), new_kid.clone()),
    ];
    both.sort();
    assert_eq!(listed(&rig, &hermes), both);
    rig.clock.advance(Duration::hours(24));
    assert_eq!(listed(&rig, &hermes), vec![("hermes".to_string(), new_kid)]);
}

#[test]
fn disabling_or_revoking_urgently_removes_a_key_at_once() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let _hades = Component::enrolled(&rig, "hades");
    let caller = Component::enrolled(&rig, "caller");
    append_identity_events(
        &rig,
        "hades",
        &[IdentityEvent::IdentityDisabled {
            at: rig.clock.now(),
            urgent: false,
        }],
    );
    revoke_urgently(&rig, "hermes", &hermes);
    assert_eq!(
        listed(&rig, &caller),
        vec![("caller".to_string(), kid_of(&rig, &caller))]
    );
}

#[test]
fn the_list_is_valid_for_300_seconds_from_the_query() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    rig.clock.advance(Duration::minutes(7));
    let published = rig.identities.keys(&hermes.credentials(None)).unwrap();
    assert_eq!(
        published.expires_at,
        rig.clock.now() + Duration::seconds(300)
    );
}

#[test]
fn by_kid_any_registered_key_comes_with_its_status() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let old_kid = kid_of(&rig, &hermes);
    rig.clock.advance(Duration::hours(48));
    hermes.renewed(&rig);
    let new_kid = kid_of(&rig, &hermes);
    let active = rig
        .identities
        .key(&hermes.credentials(None), &old_kid)
        .unwrap();
    assert!(active.active, "valid until its certificate expires");
    rig.clock.advance(Duration::hours(24));
    let old = rig
        .identities
        .key(&hermes.credentials(None), &old_kid)
        .unwrap();
    assert!(!old.active);
    assert_eq!(old.identity, spiffe("hermes"));
    let current = rig
        .identities
        .key(&hermes.credentials(None), &new_kid)
        .unwrap();
    assert!(current.active);
    let unknown = CallKey::from_bytes(&[3; 32]).unwrap().kid();
    let error = rig
        .identities
        .key(&hermes.credentials(None), &unknown)
        .err()
        .unwrap();
    assert!(is_not_found(&error), "{error:?}");
    let error = rig
        .identities
        .key(&hermes.credentials(None), "not a kid")
        .err()
        .unwrap();
    assert!(is_invalid(&error), "{error:?}");
}

#[test]
fn served_queries_leave_no_row_and_refusals_do() {
    let (rig, _) = created_ca();
    let hermes = Component::enrolled(&rig, "hermes");
    let kid = kid_of(&rig, &hermes);
    rig.identities.keys(&hermes.credentials(None)).unwrap();
    rig.identities.key(&hermes.credentials(None), &kid).unwrap();
    rig.identities
        .capabilities(&hermes.credentials(None))
        .unwrap();
    let rows = |operation: Operation| {
        rig.access_log
            .entries()
            .into_iter()
            .filter(|entry| entry.operation == operation)
            .collect::<Vec<_>>()
    };
    assert!(rows(Operation::KeyList).is_empty());
    assert!(rows(Operation::KeyGet).is_empty());
    assert!(rows(Operation::CapabilitiesGet).is_empty());
    let no_token = Credentials {
        certificates: hermes.inner.certificate_chain().unwrap(),
        bearer: None,
    };
    assert!(is_auth(&rig.identities.keys(&no_token).err().unwrap()));
    assert!(is_auth(&rig.identities.key(&no_token, &kid).err().unwrap()));
    assert!(is_auth(
        &rig.identities.capabilities(&no_token).err().unwrap()
    ));
    for operation in [
        Operation::KeyList,
        Operation::KeyGet,
        Operation::CapabilitiesGet,
    ] {
        let refused = rows(operation);
        assert_eq!(refused.len(), 1, "{operation:?}");
        assert_eq!(refused[0].method, Method::Identity);
        assert_eq!(refused[0].result, AccessResult::Failure);
        assert_eq!(refused[0].identity.as_deref(), Some("hermes"));
    }
}
