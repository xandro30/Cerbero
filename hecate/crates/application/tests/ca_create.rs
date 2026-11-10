//! T030: the creation of the CA (FR-001 to FR-004, SC-007): `begin` checks the state and the CA
//! passphrase and returns the request of the intermediate; `confirm` checks the certificates and
//! stores everything in one transaction; `abort` records its reason.

mod common;

use common::*;
use hecate_application::ports::{
    CertificateProfile, Clock, EventStore, IdentityStore, PkiProvider,
};
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::events::{CaEvent, CertificateEvent, CertificateOrigin};
use hecate_domain::identity::names::{IdentityName, SpiffeId};
use hecate_domain::pem;
use hecate_domain::sealing::access_log::{AccessResult, Method, Operation};
use hecate_domain::sealing::state::SealState;
use hecate_infrastructure::pki::RcgenPki;
use time::Duration;

fn ca_rows(rig: &CaRig) -> Vec<(AccessResult, String)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaCreate)
        .map(|entry| (entry.result, entry.detail))
        .collect()
}

fn nothing_stored(rig: &CaRig) -> bool {
    rig.store.load("ca").unwrap().is_empty()
        && rig.store.streams("certificate/").unwrap().is_empty()
        && rig.store.ca_material_count() == 0
        && rig.store.load_own_tls_key().unwrap().is_none()
}

#[test]
fn begin_needs_hecate_unsealed_without_a_ca_or_another_ceremony() {
    let rig = ca_rig();
    rig.sealing.seal().unwrap();
    let sealed = rig
        .service
        .create_begin(TRUST_DOMAIN, CA_PASSPHRASE)
        .err()
        .unwrap();
    assert_eq!(conflict_state(&sealed), Some(SealState::Sealed));
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let begun = begin(&rig);
    let again = rig
        .service
        .create_begin(TRUST_DOMAIN, CA_PASSPHRASE)
        .err()
        .unwrap();
    assert!(is_conflict(&again), "{again:?}");
    let signed = sign(&rig, &begun);
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    let with_a_ca = rig
        .service
        .create_begin(TRUST_DOMAIN, CA_PASSPHRASE)
        .err()
        .unwrap();
    assert!(is_conflict(&with_a_ca), "{with_a_ca:?}");
}

#[test]
fn begin_validates_the_trust_domain_and_the_ca_passphrase_without_the_shared_counter() {
    let rig = ca_rig();
    for (trust_domain, passphrase) in [
        ("Not A Domain", CA_PASSPHRASE),
        (TRUST_DOMAIN, "elevenchars"),
        (TRUST_DOMAIN, GOOD_PASSPHRASE),
    ] {
        let error = rig
            .service
            .create_begin(trust_domain, passphrase)
            .err()
            .unwrap();
        assert!(is_invalid(&error), "{trust_domain} {passphrase}: {error:?}");
    }
    let status = rig.sealing.status().unwrap();
    assert_eq!((status.consecutive_failures, status.retry_after_s), (0, 0));
    assert!(nothing_stored(&rig));
    assert!(begin(&rig).ceremony_id.len() >= 22);
}

#[test]
fn begin_returns_the_request_of_the_intermediate_for_15_minutes() {
    let rig = ca_rig();
    let begun = begin(&rig);
    assert_eq!(begun.expires_at, rig.clock.now() + Duration::minutes(15));
    let request = RcgenPki::new()
        .read_request(&begun.intermediate_csr)
        .unwrap();
    assert_eq!(
        request.common_name.as_deref(),
        Some("Cerbero Intermediate CA cerbero.test 1.1")
    );
    assert!(nothing_stored(&rig));
}

#[test]
fn confirm_stores_the_ca_its_material_and_hecate_certificate_in_one_transaction() {
    let rig = ca_rig();
    let now = rig.clock.now();
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    let created = rig
        .service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    let pki = RcgenPki::new();
    let fingerprint = pki.fingerprint(&signed.root_der);
    let intermediate = pki.read_certificate(&signed.intermediate_der).unwrap();
    assert_eq!(created.trust_domain, trust_domain());
    assert_eq!(created.root_fingerprint, fingerprint);
    assert_eq!(created.intermediate_not_after, intermediate.not_after);

    let ca: Vec<CaEvent> = rig
        .store
        .load("ca")
        .unwrap()
        .iter()
        .map(|record| CaEvent::decode(record).unwrap())
        .collect();
    assert_eq!(
        ca,
        vec![CaEvent::CaCreated {
            at: now,
            generation: 1,
            trust_domain: trust_domain(),
            root_fingerprint: fingerprint,
            root_not_after: now + Duration::days(3650),
            intermediate_version: 1,
            intermediate_not_after: intermediate.not_after,
        }]
    );

    let material = rig.store.load_ca_material().unwrap();
    assert_eq!(material.len(), 1);
    let material = &material[0];
    assert_eq!((material.generation, material.intermediate_version), (1, 1));
    assert_eq!(material.root_certificate, signed.root_der);
    assert_eq!(material.intermediate_certificate, signed.intermediate_der);
    let aad = format!(
        r#"{{"generation":1,"purpose":"ca_intermediate","root":"{fingerprint}","version":1}}"#
    );
    let key = rig
        .crypto
        .aead_decrypt(
            &rig.sealing.root().unwrap(),
            &material.key_nonce,
            &material.key_ciphertext,
            aad.as_bytes(),
        )
        .unwrap();
    assert_eq!(pki.public_key(&key).unwrap(), intermediate.public_key);

    let own = rig.store.load_own_tls_key().unwrap().unwrap();
    let streams = rig.store.streams("certificate/").unwrap();
    assert_eq!(streams, vec![format!("certificate/hecate/{}", own.serial)]);
    let records = rig.store.load(&streams[0]).unwrap();
    assert_eq!(records.len(), 1);
    let CertificateEvent::CertificateIssued(issued) =
        CertificateEvent::decode(&records[0]).unwrap()
    else {
        panic!("not an issuance");
    };
    assert_eq!(issued.identity, IdentityName::hecate());
    assert_eq!(issued.serial, own.serial);
    assert_eq!(
        (issued.generation, issued.intermediate_version, issued.epoch),
        (1, 1, 0)
    );
    assert_eq!(issued.origin, CertificateOrigin::OwnCertificate);
    assert!(issued.call_key.is_none() && issued.renewed_from.is_none());
    assert!(issued.request_sha256.is_none() && issued.idempotency_key.is_none());
    assert_eq!(issued.not_before, now - Duration::minutes(1));
    assert_eq!(issued.not_after, now + Duration::hours(72));
    let hecate = CertificateProfile::Leaf {
        spiffe_id: SpiffeId::new(trust_domain(), IdentityName::hecate()),
    };
    assert!(pki.follows_profile(&issued.certificate, &hecate).unwrap());
    assert!(
        pki.is_signed_by(&issued.certificate, &signed.intermediate_der)
            .unwrap()
    );
    assert_eq!(
        pki.read_certificate(&issued.certificate)
            .unwrap()
            .public_key,
        pki.public_key(&own.private_key).unwrap()
    );
    assert_eq!(
        ca_rows(&rig),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::Success, "confirm".to_string()),
        ]
    );
}

#[test]
fn hecate_certificate_never_outlives_the_intermediate() {
    let rig = ca_rig();
    let now = rig.clock.now();
    let begun = begin(&rig);
    // A root CA that ends in ten hours caps the intermediate, and the intermediate caps Hécate.
    let signed = sign_with_root_issued_at(
        &rig,
        &begun,
        now - Duration::days(3650) + Duration::hours(10),
    );
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    let own = rig
        .service
        .status()
        .unwrap()
        .unwrap()
        .hecate_certificate
        .unwrap();
    assert_eq!(own.not_after, now + Duration::hours(10));
}

#[test]
fn confirm_checks_the_certificates_and_keeps_the_ceremony_after_a_refusal() {
    let rig = ca_rig();
    let now = rig.clock.now();
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    let other = sign(&rig, &begun);
    let other_key = RcgenPki::new().generate_key().unwrap();
    let pki = RcgenPki::new();
    let foreign_request = pki
        .create_request(
            &other_key,
            &CertificateProfile::Intermediate {
                trust_domain: trust_domain(),
                generation: 1,
                version: 1,
            },
        )
        .unwrap();
    let foreign_intermediate = signed
        .root_key
        .sign_intermediate(&signed.root_der, &foreign_request, &serial(0x9d), now)
        .unwrap();
    let wrong_version = pki
        .create_request(
            &other_key,
            &CertificateProfile::Intermediate {
                trust_domain: trust_domain(),
                generation: 1,
                version: 2,
            },
        )
        .unwrap();
    let second_version = signed
        .root_key
        .sign_intermediate(&signed.root_der, &wrong_version, &serial(0x9d), now)
        .unwrap();
    let future_root = sign_with_root_issued_at(&rig, &begun, now + Duration::days(1));
    let as_pem = |der: &[u8]| pem::encode("CERTIFICATE", der);
    for (root, intermediate) in [
        // An intermediate signed by another root CA.
        (signed.root.clone(), other.intermediate.clone()),
        // The root CA as its own intermediate, and the intermediate as the root CA.
        (signed.root.clone(), signed.root.clone()),
        (signed.intermediate.clone(), signed.intermediate.clone()),
        // An intermediate for a key that is not the key of the ceremony.
        (signed.root.clone(), as_pem(&foreign_intermediate)),
        // An intermediate of another name.
        (signed.root.clone(), as_pem(&second_version)),
        // A root CA that is not valid yet.
        (future_root.root.clone(), future_root.intermediate.clone()),
        // Not PEM.
        ("not a certificate".to_string(), signed.intermediate.clone()),
        (
            signed.root.clone(),
            "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n".to_string(),
        ),
    ] {
        let error = rig
            .service
            .create_confirm(&begun.ceremony_id, &root, &intermediate)
            .err()
            .unwrap();
        assert!(is_invalid(&error), "{error:?}");
    }
    assert!(nothing_stored(&rig));
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
}

#[test]
fn an_unknown_or_expired_ceremony_is_a_conflict() {
    let rig = ca_rig();
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    let unknown = rig
        .service
        .create_confirm("unknown", &signed.root, &signed.intermediate)
        .err()
        .unwrap();
    assert!(is_conflict(&unknown), "{unknown:?}");
    rig.clock
        .advance(Duration::minutes(15) + Duration::seconds(1));
    let expired = rig
        .service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .err()
        .unwrap();
    assert!(is_conflict(&expired), "{expired:?}");
    let abort = rig
        .service
        .create_abort(&begun.ceremony_id, "cancelled")
        .err()
        .unwrap();
    assert!(is_conflict(&abort), "{abort:?}");
    assert!(nothing_stored(&rig));
    begin(&rig);
}

#[test]
fn sealing_or_restarting_loses_the_ceremony_without_storing_anything() {
    let rig = ca_rig();
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    rig.sealing.seal().unwrap();
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
    let after_seal = rig
        .service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .err()
        .unwrap();
    assert!(is_conflict(&after_seal), "{after_seal:?}");
    assert!(nothing_stored(&rig));

    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    let restarted = rig.restart();
    restarted
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    let after_restart = restarted
        .service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .err()
        .unwrap();
    assert!(is_conflict(&after_restart), "{after_restart:?}");
    assert!(nothing_stored(&restarted));
}

#[test]
fn abort_discards_the_ceremony_and_records_its_reason() {
    let rig = ca_rig();
    let begun = begin(&rig);
    let bad_reason = rig
        .service
        .create_abort(&begun.ceremony_id, "bored")
        .err()
        .unwrap();
    assert!(is_invalid(&bad_reason), "{bad_reason:?}");
    rig.service
        .create_abort(&begun.ceremony_id, "backup_check_failed")
        .unwrap();
    let signed = sign(&rig, &begun);
    let gone = rig
        .service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .err()
        .unwrap();
    assert!(is_conflict(&gone), "{gone:?}");
    let cancelled = begin(&rig);
    rig.service
        .create_abort(&cancelled.ceremony_id, "cancelled")
        .unwrap();
    assert_eq!(
        ca_rows(&rig),
        vec![
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::InvalidInput, "abort".to_string()),
            (
                AccessResult::Success,
                "abort: backup_check_failed".to_string()
            ),
            (AccessResult::RejectedConflict, "confirm".to_string()),
            (AccessResult::Success, "begin".to_string()),
            (AccessResult::Success, "abort: cancelled".to_string()),
        ]
    );
    assert!(nothing_stored(&rig));
}

#[test]
fn the_rows_are_from_the_admin_and_the_ca_passphrase_is_never_kept() {
    let rig = ca_rig();
    let refused = rig.service.create_begin(TRUST_DOMAIN, "too short");
    assert!(refused.is_err());
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    let rows: Vec<_> = rig
        .access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == Operation::CaCreate)
        .collect();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_eq!(row.method, Method::Local);
        assert_eq!(row.subject.as_deref(), Some("admin"));
        assert!(row.identity.is_none() && row.certificate.is_none());
        let text = format!("{row:?}");
        assert!(
            !text.contains(CA_PASSPHRASE) && !text.contains("too short"),
            "{text}"
        );
    }
    let contents = rig.store.contents();
    assert!(
        !contents
            .windows(CA_PASSPHRASE.len())
            .any(|window| window == CA_PASSPHRASE.as_bytes())
    );
}

#[test]
fn a_power_cut_during_confirm_leaves_nothing_stored() {
    let rig = ca_rig();
    let begun = begin(&rig);
    let signed = sign(&rig, &begun);
    rig.store.fail_commits(true);
    assert!(
        rig.service
            .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
            .is_err()
    );
    assert!(nothing_stored(&rig));
    rig.store.fail_commits(false);
    rig.service
        .create_confirm(&begun.ceremony_id, &signed.root, &signed.intermediate)
        .unwrap();
    assert!(!nothing_stored(&rig));
}
