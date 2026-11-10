//! The PKI of the CA (research R2): rcgen issues certificates and requests, x509-parser reads them
//! and verifies their signatures, all with ECDSA P-256 and SHA-256 (contracts/formats.md). Keys
//! travel as the PKCS#8 of a `SecretBytes`; each operation that signs loads the key into a guard
//! that erases the rcgen key pair when dropped, also when the operation fails.

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair};
use hecate_application::ports::{
    CertificateInfo, CertificateProfile, CertificateRequest, Issuance, Issuer, PkiError,
    PkiProvider, RevokedCertificate,
};
use hecate_domain::ca::{INTERMEDIATE_LIFETIME, ROOT_LIFETIME};
use hecate_domain::events::RevocationReason;
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::names::{Serial, SpiffeId, TrustDomain};
use hecate_domain::pem;
use hecate_domain::sealing::errors::HecateError;
use hecate_domain::sensitive::SecretBytes;
use rcgen::string::Ia5String;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateRevocationListParams, DistinguishedName,
    DnType, ExtendedKeyUsagePurpose, IsCa, KeyIdMethod, KeyPair, KeyUsagePurpose,
    PKCS_ECDSA_P256_SHA256, PublicKeyData, RevokedCertParams, SanType, SerialNumber,
    SubjectPublicKeyInfo,
};
use std::sync::Arc;

use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, UnixTime};
use rustls::server::WebPkiClientVerifier;
use time::OffsetDateTime;
use x509_parser::certification_request::X509CertificationRequest;
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::oid_registry::{
    OID_EC_P256, OID_KEY_TYPE_EC_PUBLIC_KEY, OID_SIG_ECDSA_WITH_SHA256, OID_SIG_ED25519,
    OID_X509_COMMON_NAME, OID_X509_EXT_BASIC_CONSTRAINTS, OID_X509_EXT_KEY_USAGE,
};
use x509_parser::prelude::{FromDer, X509Certificate};
use x509_parser::x509::{SubjectPublicKeyInfo as PublicKeyInfo, X509Name, X509Version};
use zeroize::{Zeroize, ZeroizeOnDrop};

const CERTIFICATE_REQUEST: &str = "CERTIFICATE REQUEST";
const PUBLIC_KEY: &str = "PUBLIC KEY";
/// keyCertSign and cRLSign, and nothing else.
const CA_USAGE: u16 = 0x60;
/// digitalSignature, and nothing else.
const LEAF_USAGE: u16 = 0x01;

fn failed(reason: impl Into<String>) -> PkiError {
    PkiError(reason.into())
}

fn rcgen_failed(error: rcgen::Error) -> PkiError {
    PkiError(error.to_string())
}

/// An rcgen key pair that erases its copy of the PKCS#8 when dropped, also when the operation
/// fails; aws-lc frees its own copy of the key with `OPENSSL_free`, which cleanses the memory.
#[derive(ZeroizeOnDrop)]
struct KeyGuard(KeyPair);

impl KeyGuard {
    fn load(key: &SecretBytes) -> Result<Self, PkiError> {
        KeyPair::from_pkcs8_der_and_sign_algo(
            &PrivatePkcs8KeyDer::from(key.expose()),
            &PKCS_ECDSA_P256_SHA256,
        )
        .map(Self)
        .map_err(|_| failed("the key is not a P-256 key"))
    }

    fn public_key(&self) -> Vec<u8> {
        self.0.subject_public_key_info()
    }
}

/// The whole DER of one certificate.
fn parse(der: &[u8]) -> Result<X509Certificate<'_>, PkiError> {
    match X509Certificate::from_der(der) {
        Ok(([], certificate)) => Ok(certificate),
        _ => Err(failed("the certificate is not valid DER")),
    }
}

fn is_p256(key: &PublicKeyInfo<'_>) -> bool {
    key.algorithm.algorithm == OID_KEY_TYPE_EC_PUBLIC_KEY
        && key
            .algorithm
            .parameters
            .as_ref()
            .and_then(|parameters| parameters.as_oid().ok())
            == Some(OID_EC_P256)
}

/// The common name of a subject made of that attribute alone.
fn only_common_name(name: &X509Name<'_>) -> Option<String> {
    let mut attributes = name.iter_attributes();
    let first = attributes.next()?;
    if attributes.next().is_some() || *first.attr_type() != OID_X509_COMMON_NAME {
        return None;
    }
    first.as_str().ok().map(str::to_string)
}

fn uri(spiffe_id: &SpiffeId) -> Result<SanType, PkiError> {
    Ia5String::try_from(spiffe_id.to_string())
        .map(SanType::URI)
        .map_err(|_| failed("the SPIFFE ID is not ASCII"))
}

fn subject(profile: &CertificateProfile) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, profile.common_name());
    name
}

/// The parameters of a certificate of a profile (contracts/formats.md).
fn params(issuance: &Issuance<'_>) -> Result<CertificateParams, PkiError> {
    let mut params = CertificateParams::default();
    params.not_before = issuance.not_before;
    params.not_after = issuance.not_after;
    params.serial_number = Some(SerialNumber::from_slice(&issuance.serial.to_be_bytes()));
    params.distinguished_name = subject(issuance.profile);
    match issuance.profile {
        CertificateProfile::Root { .. } => {
            params.is_ca = IsCa::Ca(BasicConstraints::Constrained(1));
            params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        }
        CertificateProfile::Intermediate { .. } => {
            params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
            params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
            params.use_authority_key_identifier_extension = true;
        }
        CertificateProfile::Leaf { spiffe_id } => {
            params.is_ca = IsCa::ExplicitNoCa;
            params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
            params.extended_key_usages = vec![
                ExtendedKeyUsagePurpose::ServerAuth,
                ExtendedKeyUsagePurpose::ClientAuth,
            ];
            params.subject_alt_names = vec![uri(spiffe_id)?];
            params.use_authority_key_identifier_extension = true;
        }
    }
    Ok(params)
}

/// The only URI of the SAN that a certificate of a component or of Hécate carries.
fn only_uri<'a>(certificate: &'a X509Certificate<'_>) -> Option<&'a str> {
    let names = certificate.subject_alternative_name().ok()??;
    match names.value.general_names.as_slice() {
        [GeneralName::URI(uri)] => Some(uri),
        _ => None,
    }
}

fn follows(certificate: &X509Certificate<'_>, profile: &CertificateProfile) -> bool {
    let (ca, path_len, usage, leaf) = match profile {
        CertificateProfile::Root { .. } => (true, Some(1), CA_USAGE, None),
        CertificateProfile::Intermediate { .. } => (true, Some(0), CA_USAGE, None),
        CertificateProfile::Leaf { spiffe_id } => (false, None, LEAF_USAGE, Some(spiffe_id)),
    };
    let raw_serial = certificate.raw_serial();
    let serial = raw_serial.first().is_some_and(|first| *first < 0x80)
        && Serial::from_be_bytes(raw_serial).is_ok();
    let constraints = matches!(
        certificate.basic_constraints(),
        Ok(Some(found)) if found.critical
            && found.value.ca == ca
            && found.value.path_len_constraint == path_len
    );
    let key_usage = matches!(
        certificate.key_usage(),
        Ok(Some(found)) if found.critical && found.value.flags == usage
    );
    let extended = match (certificate.extended_key_usage(), leaf) {
        (Ok(None), None) => true,
        (Ok(Some(found)), Some(_)) => {
            let found = found.value;
            found.server_auth
                && found.client_auth
                && !(found.any || found.code_signing || found.email_protection)
                && !(found.time_stamping || found.ocsp_signing)
                && found.other.is_empty()
        }
        _ => false,
    };
    let names = match (certificate.subject_alternative_name(), leaf) {
        (Ok(None), None) => true,
        (Ok(Some(_)), Some(spiffe_id)) => only_uri(certificate) == Some(&spiffe_id.to_string()),
        _ => false,
    };
    let critical = certificate
        .extensions()
        .iter()
        .filter(|extension| extension.critical)
        .all(|extension| {
            extension.oid == OID_X509_EXT_BASIC_CONSTRAINTS
                || extension.oid == OID_X509_EXT_KEY_USAGE
        });
    certificate.version() == X509Version::V3
        && certificate.signature_algorithm.algorithm == OID_SIG_ECDSA_WITH_SHA256
        && is_p256(certificate.public_key())
        && only_common_name(certificate.subject()) == Some(profile.common_name())
        && serial
        && constraints
        && key_usage
        && extended
        && names
        && critical
}

/// The PKI of Hécate and of the CLI.
#[derive(Debug, Default)]
pub struct RcgenPki;

impl RcgenPki {
    pub fn new() -> Self {
        Self
    }
}

impl PkiProvider for RcgenPki {
    fn generate_key(&self) -> Result<SecretBytes, PkiError> {
        let document =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &SystemRandom::new())
                .map_err(|_| failed("cannot generate a key"))?;
        Ok(SecretBytes::new(document.as_ref().to_vec()))
    }

    fn public_key(&self, key: &SecretBytes) -> Result<Vec<u8>, PkiError> {
        Ok(KeyGuard::load(key)?.public_key())
    }

    fn create_request(
        &self,
        key: &SecretBytes,
        profile: &CertificateProfile,
    ) -> Result<String, PkiError> {
        let key = KeyGuard::load(key)?;
        let mut params = CertificateParams::default();
        params.distinguished_name = subject(profile);
        if let CertificateProfile::Leaf { spiffe_id } = profile {
            params.subject_alt_names = vec![uri(spiffe_id)?];
        }
        let request = params.serialize_request(&key.0).map_err(rcgen_failed)?;
        Ok(pem::encode(CERTIFICATE_REQUEST, request.der()))
    }

    fn read_request(&self, text: &str) -> Result<CertificateRequest, HecateError> {
        let der = pem::single(text, CERTIFICATE_REQUEST)?;
        let request = match X509CertificationRequest::from_der(&der) {
            Ok(([], request)) => request,
            _ => return Err(HecateError::invalid("the certificate request is not valid")),
        };
        let info = &request.certification_request_info;
        if request.signature_algorithm.algorithm != OID_SIG_ECDSA_WITH_SHA256
            || !is_p256(&info.subject_pki)
        {
            return Err(HecateError::invalid(
                "the certificate request is not for a P-256 key signed with SHA-256",
            ));
        }
        if request.verify_signature().is_err() {
            return Err(HecateError::invalid(
                "the signature of the certificate request does not verify",
            ));
        }
        let mut requested_uri = None;
        for extension in request.requested_extensions().into_iter().flatten() {
            if let ParsedExtension::SubjectAlternativeName(names) = extension {
                match names.general_names.as_slice() {
                    [GeneralName::URI(uri)] if requested_uri.is_none() => {
                        requested_uri = Some(uri.to_string());
                    }
                    _ => {
                        return Err(HecateError::invalid(
                            "the certificate request asks for names other than one URI",
                        ));
                    }
                }
            }
        }
        Ok(CertificateRequest {
            public_key: info.subject_pki.raw.to_vec(),
            requested_uri,
            common_name: only_common_name(&info.subject),
        })
    }

    fn self_signed(&self, key: &SecretBytes, issuance: &Issuance<'_>) -> Result<Vec<u8>, PkiError> {
        let key = KeyGuard::load(key)?;
        if key.public_key() != issuance.public_key {
            return Err(failed("a self-signed certificate is for its own key"));
        }
        let certificate = params(issuance)?
            .self_signed(&key.0)
            .map_err(rcgen_failed)?;
        Ok(certificate.der().to_vec())
    }

    fn issue(&self, issuer: &Issuer<'_>, issuance: &Issuance<'_>) -> Result<Vec<u8>, PkiError> {
        let ca = parse(issuer.certificate)?;
        if issuance.not_after > ca.validity().not_after.to_datetime() {
            return Err(failed("a certificate never outlives its issuer"));
        }
        let key = KeyGuard::load(issuer.key)?;
        if key.public_key() != ca.public_key().raw {
            return Err(failed("the key is not the key of the issuer's certificate"));
        }
        let signer =
            rcgen::Issuer::from_ca_cert_der(&CertificateDer::from(issuer.certificate), &key.0)
                .map_err(rcgen_failed)?;
        let subject_key = SubjectPublicKeyInfo::from_der(issuance.public_key)
            .map_err(|_| failed("the public key is not a SubjectPublicKeyInfo"))?;
        let certificate = params(issuance)?
            .signed_by(&subject_key, &signer)
            .map_err(rcgen_failed)?;
        Ok(certificate.der().to_vec())
    }

    fn read_certificate(&self, der: &[u8]) -> Result<CertificateInfo, PkiError> {
        let certificate = parse(der)?;
        Ok(CertificateInfo {
            serial: Serial::from_be_bytes(certificate.raw_serial())
                .map_err(|_| failed("the serial is not a positive number of 128 bits"))?,
            subject: certificate.subject().to_string(),
            issuer: certificate.issuer().to_string(),
            spiffe_id: only_uri(&certificate).and_then(|uri| SpiffeId::parse(uri).ok()),
            not_before: certificate.validity().not_before.to_datetime(),
            not_after: certificate.validity().not_after.to_datetime(),
            is_ca: certificate.is_ca(),
            public_key: certificate.public_key().raw.to_vec(),
        })
    }

    fn is_signed_by(&self, der: &[u8], issuer_der: &[u8]) -> Result<bool, PkiError> {
        let (certificate, issuer) = (parse(der)?, parse(issuer_der)?);
        Ok(certificate.issuer().as_raw() == issuer.subject().as_raw()
            && certificate
                .verify_signature(Some(issuer.public_key()))
                .is_ok())
    }

    fn follows_profile(&self, der: &[u8], profile: &CertificateProfile) -> Result<bool, PkiError> {
        Ok(follows(&parse(der)?, profile))
    }

    fn read_call_key(&self, text: &str) -> Result<CallKey, HecateError> {
        let invalid = || HecateError::invalid("the call key is not an Ed25519 public key");
        let der = pem::single(text, PUBLIC_KEY)?;
        let key = match PublicKeyInfo::from_der(&der) {
            Ok(([], key)) => key,
            _ => return Err(invalid()),
        };
        if key.algorithm.algorithm != OID_SIG_ED25519
            || key.algorithm.parameters.is_some()
            || key.subject_public_key.unused_bits != 0
        {
            return Err(invalid());
        }
        CallKey::from_bytes(&key.subject_public_key.data).map_err(|_| invalid())
    }

    fn crl(
        &self,
        issuer: &Issuer<'_>,
        revoked: &[RevokedCertificate],
        this_update: OffsetDateTime,
        next_update: OffsetDateTime,
        number: u64,
    ) -> Result<Vec<u8>, PkiError> {
        let key = KeyGuard::load(issuer.key)?;
        let signer =
            rcgen::Issuer::from_ca_cert_der(&CertificateDer::from(issuer.certificate), &key.0)
                .map_err(rcgen_failed)?;
        let params = CertificateRevocationListParams {
            this_update,
            next_update,
            crl_number: SerialNumber::from_slice(&number.to_be_bytes()),
            issuing_distribution_point: None,
            revoked_certs: revoked
                .iter()
                .map(|entry| RevokedCertParams {
                    serial_number: SerialNumber::from_slice(&entry.serial.to_be_bytes()),
                    revocation_time: entry.revoked_at,
                    // RFC 5280 §5.3.1: `unspecified` goes without the reason code extension.
                    reason_code: match entry.reason {
                        RevocationReason::SuspiciousRenewal => {
                            Some(rcgen::RevocationReason::KeyCompromise)
                        }
                        RevocationReason::Admin => None,
                    },
                    invalidity_date: None,
                })
                .collect(),
            key_identifier_method: KeyIdMethod::Sha256,
        };
        let crl = params.signed_by(&signer).map_err(rcgen_failed)?;
        Ok(crl.der().to_vec())
    }

    /// The `WebPkiClientVerifier` of rustls with the root CA as its only anchor, at the time
    /// given, which the application takes from its clock (research R1).
    fn verify_client_chain(
        &self,
        chain: &[Vec<u8>],
        root: &[u8],
        now: OffsetDateTime,
    ) -> Result<(), PkiError> {
        let (certificate, intermediates) = chain
            .split_first()
            .ok_or_else(|| failed("no client certificate"))?;
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(root.to_vec()))
            .map_err(|error| failed(error.to_string()))?;
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
            .build()
            .map_err(|error| failed(error.to_string()))?;
        let intermediates: Vec<CertificateDer<'_>> = intermediates
            .iter()
            .map(|der| CertificateDer::from(der.as_slice()))
            .collect();
        let seconds =
            u64::try_from(now.unix_timestamp()).map_err(|_| failed("the time is before 1970"))?;
        verifier
            .verify_client_cert(
                &CertificateDer::from(certificate.as_slice()),
                &intermediates,
                UnixTime::since_unix_epoch(std::time::Duration::from_secs(seconds)),
            )
            .map(|_| ())
            .map_err(|error| failed(error.to_string()))
    }
}

/// The key of the root CA in the CLI during a ceremony: its PKCS#8 lives in memory that is erased
/// when dropped, and it never reaches Hécate's process (FR-002, FR-028).
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_infrastructure::pki::RootCaKey>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_infrastructure::pki::RootCaKey>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_infrastructure::pki::RootCaKey>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RootCaKey(SecretBytes);

impl RootCaKey {
    pub fn generate() -> Result<Self, PkiError> {
        RcgenPki.generate_key().map(Self)
    }

    /// The key that a copy holds, once checked that it is a P-256 key.
    pub fn from_pkcs8(pkcs8: SecretBytes) -> Result<Self, PkiError> {
        KeyGuard::load(&pkcs8)?;
        Ok(Self(pkcs8))
    }

    pub fn pkcs8(&self) -> &SecretBytes {
        &self.0
    }

    /// The SubjectPublicKeyInfo in DER.
    pub fn public_key(&self) -> Result<Vec<u8>, PkiError> {
        RcgenPki.public_key(&self.0)
    }

    /// The self-signed root CA of a trust domain, from now and for 3,650 days.
    pub fn root_certificate(
        &self,
        trust_domain: &TrustDomain,
        serial: &Serial,
        now: OffsetDateTime,
    ) -> Result<Vec<u8>, PkiError> {
        let profile = CertificateProfile::Root {
            trust_domain: trust_domain.clone(),
        };
        RcgenPki.self_signed(
            &self.0,
            &Issuance {
                profile: &profile,
                public_key: &self.public_key()?,
                serial,
                not_before: now,
                not_after: now + ROOT_LIFETIME,
            },
        )
    }

    /// Signs the intermediate that Hécate asks for in its request: the intermediate of the name of
    /// the request, from now and for 365 days without outliving this root CA, whose certificate
    /// has to be of this key and of the same trust domain.
    pub fn sign_intermediate(
        &self,
        root_certificate: &[u8],
        request: &str,
        serial: &Serial,
        now: OffsetDateTime,
    ) -> Result<Vec<u8>, PkiError> {
        let request = RcgenPki
            .read_request(request)
            .map_err(|error| failed(error.to_string()))?;
        let profile = request
            .common_name
            .as_deref()
            .and_then(CertificateProfile::intermediate_named)
            .ok_or_else(|| failed("the request is not for an intermediate"))?;
        let CertificateProfile::Intermediate { trust_domain, .. } = &profile else {
            return Err(failed("the request is not for an intermediate"));
        };
        let root = RcgenPki.read_certificate(root_certificate)?;
        let root_name = CertificateProfile::Root {
            trust_domain: trust_domain.clone(),
        };
        if root.subject != format!("CN={}", root_name.common_name()) {
            return Err(failed(
                "the request is for an intermediate of another root CA",
            ));
        }
        if root.public_key != self.public_key()? {
            return Err(failed("the key is not the key of that root CA"));
        }
        RcgenPki.issue(
            &Issuer {
                certificate: root_certificate,
                key: &self.0,
            },
            &Issuance {
                profile: &profile,
                public_key: &request.public_key,
                serial,
                not_before: now,
                not_after: (now + INTERMEDIATE_LIFETIME).min(root.not_after),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn erased_when_dropped<T: ZeroizeOnDrop>() {}

    #[test]
    fn the_key_pair_of_an_operation_is_erased_when_it_ends() {
        erased_when_dropped::<KeyGuard>();
        let key = RcgenPki.generate_key().unwrap();
        let mut guard = KeyGuard::load(&key).unwrap();
        assert!(!guard.0.serialized_der().is_empty());
        guard.0.zeroize();
        assert!(guard.0.serialized_der().is_empty());
    }
}
