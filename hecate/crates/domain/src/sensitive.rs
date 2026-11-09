//! Transit secrets: values that only live during an operation and are erased when dropped.

use zeroize::{Zeroize, ZeroizeOnDrop};

/// A 32-byte secret key (the root and the derived, recovery, scope and version keys): it erases
/// itself when dropped and can be neither cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_domain::sensitive::SecretKey>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_domain::sensitive::SecretKey>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_domain::sensitive::SecretKey>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; 32]);

impl SecretKey {
    pub const LEN: usize = 32;

    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        <[u8; 32]>::try_from(bytes).ok().map(Self)
    }

    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Secret bytes of any length (a plaintext value or the values of a recovery share): they are
/// erased when dropped and can be neither cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_domain::sensitive::SecretBytes>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_domain::sensitive::SecretBytes>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_domain::sensitive::SecretBytes>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}
