//! FR-007 (research R14): the transit secrets of the domain live in types that erase themselves
//! when dropped, and `zeroize` leaves their bytes at zero.

use hecate_domain::sealing::passphrase::Passphrase;
use hecate_domain::sealing::share::RecoveryShare;
use hecate_domain::sensitive::{SecretBytes, SecretKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Compiles only for types that erase themselves when dropped.
fn erased_when_dropped<T: ZeroizeOnDrop>() {}

#[test]
fn every_transit_secret_of_the_domain_is_erased_when_dropped() {
    erased_when_dropped::<Passphrase>();
    erased_when_dropped::<RecoveryShare>();
    erased_when_dropped::<SecretKey>();
    erased_when_dropped::<SecretBytes>();
}

#[test]
fn zeroize_leaves_a_key_at_zero() {
    let mut key = SecretKey::new([0xA5; 32]);
    key.zeroize();
    assert_eq!(key.expose(), &[0; 32]);
}

/// A buffer is zeroed over its whole capacity and then emptied, so what can still be read of it
/// is nothing.
#[test]
fn zeroize_empties_the_passphrase_the_bytes_and_the_values_of_a_share() {
    let mut passphrase = Passphrase::new("correct horse battery").unwrap();
    passphrase.zeroize();
    assert!(passphrase.expose().is_empty());

    let mut bytes = SecretBytes::new(vec![0xA5; 48]);
    bytes.zeroize();
    assert!(bytes.expose().is_empty());

    let mut share = RecoveryShare::new(2, *b"abcd", vec![0xA5; 32]).unwrap();
    share.zeroize();
    assert!(share.values().is_empty());
    assert_eq!((share.index(), *share.share_set_id()), (0, [0; 4]));
}
