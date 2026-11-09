use hecate_domain::sensitive::SecretKey;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// The root while Hécate is unsealed; dropping it, when sealing, erases it. It can be neither
/// cloned nor shown.
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_application::sealing::root_holder::RootHolder>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_application::sealing::root_holder::RootHolder>();
/// ```
///
/// ```compile_fail
/// fn shown<T: std::fmt::Debug>() {}
/// shown::<hecate_application::sealing::root_holder::RootHolder>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RootHolder(SecretKey);

impl RootHolder {
    pub fn new(root: SecretKey) -> Self {
        Self(root)
    }

    /// A copy for one operation, erased when the caller drops it.
    pub fn copy(&self) -> SecretKey {
        SecretKey::new(*self.0.expose())
    }

    pub fn key(&self) -> &SecretKey {
        &self.0
    }
}
