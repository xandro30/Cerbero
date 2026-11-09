//! The software `CryptoProvider` (research R3): aws-lc-rs for AES-256-GCM, HKDF, HMAC and random
//! bytes, libsodium for Argon2id and vsss-rs for Shamir in GF(256).

use std::convert::Infallible;

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use aws_lc_rs::{hkdf, hmac};
use hecate_application::ports::{CryptoError, CryptoProvider};
use hecate_domain::sensitive::{SecretBytes, SecretKey};
use vsss_rs::Gf256;
use zeroize::Zeroizing;

const NONCE_LEN: usize = 12;
const THRESHOLD: usize = 2;
const SHARE_COUNT: usize = 3;

pub struct SoftwareCryptoProvider {
    random: SystemRandom,
}

impl SoftwareCryptoProvider {
    pub fn new() -> Self {
        Self {
            random: SystemRandom::new(),
        }
    }

    fn fill(&self, bytes: &mut [u8]) {
        if self.random.fill(bytes).is_err() {
            panic!("the system random generator failed");
        }
    }

    fn cipher(key: &SecretKey) -> Result<LessSafeKey, CryptoError> {
        UnboundKey::new(&AES_256_GCM, key.expose())
            .map(LessSafeKey::new)
            .map_err(|_| CryptoError("invalid key".into()))
    }
}

impl Default for SoftwareCryptoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CryptoProvider for SoftwareCryptoProvider {
    fn random_bytes(&self, n: usize) -> Vec<u8> {
        let mut bytes = vec![0; n];
        self.fill(&mut bytes);
        bytes
    }

    fn random_key(&self) -> SecretKey {
        let mut bytes = Zeroizing::new([0u8; SecretKey::LEN]);
        self.fill(bytes.as_mut());
        SecretKey::new(*bytes)
    }

    fn derive_key(
        &self,
        passphrase: &str,
        salt: &[u8],
        opslimit: u64,
        memlimit: usize,
    ) -> Result<SecretKey, CryptoError> {
        libsodium_rs::ensure_init().map_err(|_| CryptoError("key derivation failed".into()))?;
        let key = libsodium_rs::crypto_pwhash::argon2id::pwhash(
            SecretKey::LEN,
            passphrase.as_bytes(),
            salt,
            opslimit,
            memlimit,
        )
        .map(Zeroizing::new)
        .map_err(|_| CryptoError("key derivation failed".into()))?;
        SecretKey::from_slice(&key).ok_or_else(|| CryptoError("key derivation failed".into()))
    }

    fn aead_encrypt(
        &self,
        key: &SecretKey,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        let mut nonce = [0u8; NONCE_LEN];
        self.fill(&mut nonce);
        let mut sealed = plaintext.to_vec();
        Self::cipher(key)?
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad),
                &mut sealed,
            )
            .map_err(|_| CryptoError("encryption failed".into()))?;
        Ok((nonce.to_vec(), sealed))
    }

    fn aead_decrypt(
        &self,
        key: &SecretKey,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<SecretBytes, CryptoError> {
        let failed = || CryptoError("decryption failed".into());
        let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| failed())?;
        let mut opened = Zeroizing::new(ciphertext.to_vec());
        let plaintext = Self::cipher(key)?
            .open_in_place(nonce, Aad::from(aad), opened.as_mut())
            .map_err(|_| failed())?;
        Ok(SecretBytes::new(plaintext.to_vec()))
    }

    fn hmac_sha256(&self, key: &SecretKey, data: &[u8]) -> [u8; 32] {
        let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key.expose()), data);
        let mut digest = [0u8; 32];
        digest.copy_from_slice(tag.as_ref());
        digest
    }

    fn hkdf_sha256(&self, key: &SecretKey, info: &[u8]) -> SecretKey {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(key.expose());
        let mut out = Zeroizing::new([0u8; SecretKey::LEN]);
        let info = [info];
        let filled = prk
            .expand(&info, hkdf::HKDF_SHA256)
            .and_then(|okm| okm.fill(out.as_mut()));
        if filled.is_err() {
            panic!("HKDF-SHA256 cannot fail for a 32-byte output");
        }
        SecretKey::new(*out)
    }

    fn shamir_split(&self, secret: &SecretKey) -> Result<Vec<(u8, SecretBytes)>, CryptoError> {
        let rng = rand_core::UnwrapErr(SystemRng(SystemRandom::new()));
        let shares = Gf256::split_bytes(THRESHOLD, SHARE_COUNT, secret.expose(), rng)
            .map_err(|_| CryptoError("secret sharing failed".into()))?;
        Ok(shares
            .into_iter()
            .map(|share| {
                let share = Zeroizing::new(share);
                (share[0], SecretBytes::new(share[1..].to_vec()))
            })
            .collect())
    }

    fn shamir_combine(&self, shares: &[(u8, &[u8])]) -> Result<SecretKey, CryptoError> {
        let mut indices: Vec<u8> = shares.iter().map(|(index, _)| *index).collect();
        indices.sort_unstable();
        indices.dedup();
        if indices.len() < THRESHOLD || indices.len() != shares.len() {
            return Err(CryptoError("not enough distinct shares".into()));
        }
        if shares
            .iter()
            .any(|(_, values)| values.len() != SecretKey::LEN)
        {
            return Err(CryptoError("invalid share length".into()));
        }
        let encoded: Vec<Zeroizing<Vec<u8>>> = shares
            .iter()
            .map(|(index, values)| {
                let mut share = Vec::with_capacity(1 + values.len());
                share.push(*index);
                share.extend_from_slice(values);
                Zeroizing::new(share)
            })
            .collect();
        let plain: Vec<Vec<u8>> = encoded.iter().map(|share| share.to_vec()).collect();
        let plain = Zeroizing::new(plain);
        let secret = Gf256::combine_bytes(plain.as_slice())
            .map(Zeroizing::new)
            .map_err(|_| CryptoError("cannot combine the shares".into()))?;
        SecretKey::from_slice(&secret)
            .ok_or_else(|| CryptoError("cannot combine the shares".into()))
    }
}

/// The aws-lc-rs generator behind the `rand_core` traits that vsss-rs asks for.
struct SystemRng(SystemRandom);

#[derive(Debug, thiserror::Error)]
#[error("the system random generator failed")]
struct RandomFailure;

impl rand_core::TryRng for SystemRng {
    type Error = RandomFailure;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut bytes = [0u8; 4];
        self.try_fill_bytes(&mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut bytes = [0u8; 8];
        self.try_fill_bytes(&mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        self.0.fill(dst).map_err(|_| RandomFailure)
    }
}

impl rand_core::TryCryptoRng for SystemRng {}

const _: fn() = || {
    fn infallible<T: rand_core::TryRng<Error = Infallible>>() {}
    infallible::<rand_core::UnwrapErr<SystemRng>>();
};
