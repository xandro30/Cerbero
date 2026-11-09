use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::sealing::errors::HecateError;

pub const MIN_LENGTH: usize = 12;

/// The admin's passphrase: at least 12 characters, erased when dropped and never shown.
///
/// ```compile_fail
/// let passphrase = hecate_domain::sealing::passphrase::Passphrase::new("super secret phrase").unwrap();
/// let shown = format!("{passphrase:?}");
/// ```
///
/// ```compile_fail
/// let passphrase = hecate_domain::sealing::passphrase::Passphrase::new("super secret phrase").unwrap();
/// let shown = format!("{passphrase}");
/// ```
///
/// ```
/// fn erased<T: zeroize::ZeroizeOnDrop>() {}
/// erased::<hecate_domain::sealing::passphrase::Passphrase>();
/// ```
///
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<hecate_domain::sealing::passphrase::Passphrase>();
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Passphrase(String);

impl Passphrase {
    pub fn new(value: &str) -> Result<Self, HecateError> {
        if value.chars().count() < MIN_LENGTH {
            return Err(HecateError::invalid(format!(
                "passphrase must have at least {MIN_LENGTH} characters"
            )));
        }
        Ok(Self(value.to_string()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // py: unit/domain/test_passphrase.py::test_accepts_twelve_or_more_characters
    #[test]
    fn accepts_twelve_or_more_characters() {
        assert_eq!(
            Passphrase::new("exactly12chr").unwrap().expose(),
            "exactly12chr"
        );
        assert!(Passphrase::new("doce con eñe").is_ok());
    }

    // py: unit/domain/test_passphrase.py::test_rejects_fewer_than_twelve_characters
    #[test]
    fn rejects_fewer_than_twelve_characters() {
        for value in ["", "short", "elevenchars", "once con ñ "] {
            let error = Passphrase::new(value).err().unwrap();
            assert_eq!(
                error,
                HecateError::invalid("passphrase must have at least 12 characters")
            );
        }
    }

    // py: unit/domain/test_passphrase.py::test_repr_and_str_do_not_show_the_value
    #[test]
    fn repr_and_str_do_not_show_the_value() {
        let passphrase = Passphrase::new("super secret phrase").unwrap();
        assert_eq!(passphrase.expose(), "super secret phrase");
    }
}
