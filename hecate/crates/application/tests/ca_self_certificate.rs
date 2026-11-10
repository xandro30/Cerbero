//! T031: Hécate renews its own certificate at two thirds of its life, with a new key, only while
//! unsealed; one that expired while sealed is renewed once unsealed (FR-012).

mod common;

use common::*;
use hecate_application::ports::{EventStore, IdentityStore, PkiProvider};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::events::{CertificateEvent, CertificateOrigin};
use hecate_domain::identity::names::Serial;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_infrastructure::pki::RcgenPki;
use time::Duration;

/// 72 hours and the minute before the issuance.
const LIFETIME: Duration = Duration::minutes(72 * 60 + 1);

fn current_serial(rig: &CaRig) -> Serial {
    rig.store
        .load_own_tls_key()
        .unwrap()
        .unwrap()
        .serial
        .clone()
}

fn issued(rig: &CaRig, serial: &Serial) -> CertificateEvent {
    let records = rig
        .store
        .load(&format!("certificate/hecate/{serial}"))
        .unwrap();
    CertificateEvent::decode(&records[0]).unwrap()
}

#[test]
fn hecate_renews_its_certificate_at_two_thirds_of_its_life_with_a_new_key() {
    let (rig, signed) = created_ca();
    let old = current_serial(&rig);
    let old_key = rig.store.load_own_tls_key().unwrap().unwrap();
    // Issued a minute before the creation, so two thirds of its life end that much earlier.
    rig.clock
        .advance(LIFETIME * 2 / 3 - Duration::minutes(1) - Duration::seconds(1));
    assert_eq!(rig.service.renew_own_certificate_if_due().unwrap(), None);
    assert_eq!(current_serial(&rig), old);
    rig.clock.advance(Duration::seconds(1));
    let new = rig.service.renew_own_certificate_if_due().unwrap().unwrap();
    assert_ne!(new, old);
    assert_eq!(current_serial(&rig), new);

    let pki = RcgenPki::new();
    let new_key = rig.store.load_own_tls_key().unwrap().unwrap();
    assert_ne!(
        pki.public_key(&new_key.private_key).unwrap(),
        pki.public_key(&old_key.private_key).unwrap()
    );
    let CertificateEvent::CertificateIssued(renewal) = issued(&rig, &new) else {
        panic!("not an issuance");
    };
    assert_eq!(renewal.origin, CertificateOrigin::OwnCertificate);
    assert_eq!(renewal.renewed_from.as_ref(), Some(&old));
    assert!(
        pki.is_signed_by(&renewal.certificate, &signed.intermediate_der)
            .unwrap()
    );
    let old_stream = rig
        .store
        .load(&format!("certificate/hecate/{old}"))
        .unwrap();
    assert_eq!(old_stream.len(), 2);
    assert_eq!(
        CertificateEvent::decode(&old_stream[1]).unwrap(),
        CertificateEvent::CertificateRenewed {
            at: renewal.at,
            new_serial: new.clone(),
        }
    );

    let rows: Vec<_> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CertificateRenew)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].method, Method::Internal);
    assert_eq!(rows[0].result, AccessResult::Success);
    assert_eq!(rows[0].subject.as_deref(), Some("hecate"));
    assert_eq!(rows[0].identity.as_deref(), Some("hecate"));
    assert_eq!(rows[0].certificate.as_deref(), Some(new.as_str()));
    assert_eq!(rig.service.renew_own_certificate_if_due().unwrap(), None);
}

#[test]
fn sealed_hecate_does_not_renew_its_certificate() {
    let (rig, _) = created_ca();
    let old = current_serial(&rig);
    rig.sealing.seal().unwrap();
    rig.clock.advance(LIFETIME * 2 / 3);
    assert_eq!(rig.service.renew_own_certificate_if_due().unwrap(), None);
    assert_eq!(current_serial(&rig), old);
    assert!(rig.service.tls_material().unwrap().is_some());
}

#[test]
fn a_certificate_that_expired_while_sealed_is_renewed_once_unsealed() {
    let (rig, _) = created_ca();
    let old = current_serial(&rig);
    rig.sealing.seal().unwrap();
    rig.clock.advance(LIFETIME);
    assert_eq!(rig.service.renew_own_certificate_if_due().unwrap(), None);
    assert!(rig.service.tls_material().unwrap().is_none());
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let new = rig.service.renew_own_certificate_if_due().unwrap().unwrap();
    assert_ne!(new, old);
    let material = rig.service.tls_material().unwrap().unwrap();
    assert_eq!(material.not_after, rig_now(&rig) + Duration::hours(72));
}

fn rig_now(rig: &CaRig) -> time::OffsetDateTime {
    use hecate_application::ports::Clock;
    rig.clock.now()
}

#[test]
fn the_tls_material_is_the_chain_up_to_the_root_and_the_current_key() {
    let (rig, signed) = created_ca();
    let material = rig.service.tls_material().unwrap().unwrap();
    let serial = current_serial(&rig);
    let CertificateEvent::CertificateIssued(own) = issued(&rig, &serial) else {
        panic!("not an issuance");
    };
    assert_eq!(
        material.chain,
        vec![
            own.certificate.clone(),
            signed.intermediate_der.clone(),
            signed.root_der.clone()
        ]
    );
    assert_eq!(material.root, signed.root_der);
    assert_eq!(material.not_after, own.not_after);
    let pki = RcgenPki::new();
    assert_eq!(
        pki.public_key(&material.key).unwrap(),
        pki.read_certificate(&own.certificate).unwrap().public_key
    );
    assert!(ca_rig().service.tls_material().unwrap().is_none());
}

// T122: a power cut in the renewal keeps the current certificate, and the next run renews it.
#[test]
fn a_power_cut_in_the_renewal_keeps_the_current_certificate() {
    let (rig, _) = created_ca();
    let old = current_serial(&rig);
    let old_key = rig.store.load_own_tls_key().unwrap().unwrap();
    rig.clock.advance(Duration::hours(49));
    rig.store.fail_commits(true);
    assert!(rig.service.renew_own_certificate_if_due().is_err());
    rig.store.fail_commits(false);
    assert_eq!(current_serial(&rig), old);
    let kept = rig.store.load_own_tls_key().unwrap().unwrap();
    assert_eq!(kept.private_key.expose(), old_key.private_key.expose());
    assert_eq!(rig.store.streams("certificate/hecate/").unwrap().len(), 1);
    let renewed = rig.service.renew_own_certificate_if_due().unwrap().unwrap();
    assert_ne!(renewed, old);
    assert_eq!(current_serial(&rig), renewed);
}
