//! The format of the copy of the root CA key (contracts/formats.md): a PEM block `HECATE ROOT CA
//! KEY` whose content is a 48-byte header and the AES-256-GCM ciphertext with its tag. The
//! associated data are the header and the 32 bytes of the root fingerprint, so the copy only opens
//! next to the certificate of its root CA.

use crate::identity::names::RootFingerprint;

pub const BACKUP_LABEL: &str = "HECATE ROOT CA KEY";
pub const MAGIC: [u8; 4] = *b"HRK1";
pub const HEADER_LEN: usize = 48;
pub const TAG_LEN: usize = 16;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;

/// The Argon2id parameters, the salt and the nonce of the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupHeader {
    pub opslimit: u64,
    pub memlimit: u64,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
}

/// A text that is not a copy of this format: it never says more, so a wrong passphrase and the
/// copy of another CA look the same to whoever reads the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the file is not a copy of the root CA key")]
pub struct NotABackup;

impl BackupHeader {
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0; HEADER_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4..12].copy_from_slice(&self.opslimit.to_be_bytes());
        bytes[12..20].copy_from_slice(&self.memlimit.to_be_bytes());
        bytes[20..36].copy_from_slice(&self.salt);
        bytes[36..].copy_from_slice(&self.nonce);
        bytes
    }

    fn from_bytes(bytes: &[u8; HEADER_LEN]) -> Result<Self, NotABackup> {
        if bytes[..4] != MAGIC {
            return Err(NotABackup);
        }
        let mut header = Self {
            opslimit: 0,
            memlimit: 0,
            salt: [0; SALT_LEN],
            nonce: [0; NONCE_LEN],
        };
        let mut number = [0; 8];
        number.copy_from_slice(&bytes[4..12]);
        header.opslimit = u64::from_be_bytes(number);
        number.copy_from_slice(&bytes[12..20]);
        header.memlimit = u64::from_be_bytes(number);
        header.salt.copy_from_slice(&bytes[20..36]);
        header.nonce.copy_from_slice(&bytes[36..]);
        Ok(header)
    }
}

/// The 48 bytes of the header followed by the 32 bytes of the root fingerprint.
pub fn associated_data(
    header: &BackupHeader,
    fingerprint: &RootFingerprint,
) -> [u8; HEADER_LEN + 32] {
    let mut aad = [0; HEADER_LEN + 32];
    aad[..HEADER_LEN].copy_from_slice(&header.to_bytes());
    aad[HEADER_LEN..].copy_from_slice(fingerprint.bytes());
    aad
}

pub fn encode(header: &BackupHeader, ciphertext: &[u8]) -> String {
    let mut content = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    content.extend_from_slice(&header.to_bytes());
    content.extend_from_slice(ciphertext);
    crate::pem::encode(BACKUP_LABEL, &content)
}

/// The header and the ciphertext of a copy: one block of its label, its mark and at least a tag.
pub fn decode(text: &str) -> Result<(BackupHeader, Vec<u8>), NotABackup> {
    let content = crate::pem::single(text, BACKUP_LABEL).map_err(|_| NotABackup)?;
    if content.len() < HEADER_LEN + TAG_LEN {
        return Err(NotABackup);
    }
    let (header, ciphertext) = content.split_at(HEADER_LEN);
    let header = BackupHeader::from_bytes(header.try_into().map_err(|_| NotABackup)?)?;
    Ok((header, ciphertext.to_vec()))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::identity::names::RootFingerprint;

    fn header() -> BackupHeader {
        BackupHeader {
            opslimit: 4,
            memlimit: 1 << 30,
            salt: [1; 16],
            nonce: [2; 12],
        }
    }

    #[test]
    fn the_backup_is_a_pem_of_its_label_with_the_header_and_the_ciphertext() {
        let ciphertext = vec![3; 48];
        let text = encode(&header(), &ciphertext);
        assert!(
            text.starts_with("-----BEGIN HECATE ROOT CA KEY-----\n"),
            "{text}"
        );
        assert!(
            text.ends_with("-----END HECATE ROOT CA KEY-----\n"),
            "{text}"
        );
        let content = crate::pem::single(&text, BACKUP_LABEL).unwrap();
        assert_eq!(&content[..4], b"HRK1");
        assert_eq!(&content[4..12], &4u64.to_be_bytes());
        assert_eq!(&content[12..20], &(1u64 << 30).to_be_bytes());
        assert_eq!(&content[20..36], &[1; 16]);
        assert_eq!(&content[36..48], &[2; 12]);
        assert_eq!(&content[48..], ciphertext.as_slice());
        let (read, read_ciphertext) = decode(&text).unwrap();
        assert_eq!(read, header());
        assert_eq!(read_ciphertext, ciphertext);
    }

    #[test]
    fn the_associated_data_are_the_header_and_the_root_fingerprint() {
        let fingerprint = RootFingerprint::of_der(b"root");
        let aad = associated_data(&header(), &fingerprint);
        assert_eq!(aad.len(), 80);
        assert_eq!(&aad[..48], header().to_bytes().as_slice());
        assert_eq!(&aad[48..], fingerprint.bytes().as_slice());
    }

    #[test]
    fn an_unknown_mark_a_short_content_or_another_label_is_rejected() {
        let mut content = header().to_bytes().to_vec();
        content.extend_from_slice(&[3; 16]);
        let good = crate::pem::encode(BACKUP_LABEL, &content);
        assert!(decode(&good).is_ok());
        let mut wrong_mark = content.clone();
        wrong_mark[3] = b'2';
        let short = content[..HEADER_LEN + TAG_LEN - 1].to_vec();
        for bad in [
            crate::pem::encode(BACKUP_LABEL, &wrong_mark),
            crate::pem::encode(BACKUP_LABEL, &short),
            crate::pem::encode("PRIVATE KEY", &content),
            "not a backup".to_string(),
        ] {
            assert_eq!(decode(&bad).unwrap_err(), NotABackup, "{bad}");
        }
    }

    proptest! {
        #[test]
        fn headers_round_trip(
            opslimit in any::<u64>(),
            memlimit in any::<u64>(),
            salt in proptest::array::uniform16(any::<u8>()),
            nonce in proptest::array::uniform12(any::<u8>()),
            ciphertext in proptest::collection::vec(any::<u8>(), TAG_LEN..200),
        ) {
            let header = BackupHeader { opslimit, memlimit, salt, nonce };
            let (read, read_ciphertext) = decode(&encode(&header, &ciphertext)).unwrap();
            prop_assert_eq!(read, header);
            prop_assert_eq!(read_ciphertext, ciphertext);
        }
    }
}
