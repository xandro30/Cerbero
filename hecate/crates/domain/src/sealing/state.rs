use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealState {
    Uninitialized,
    Sealed,
    Unsealed,
}

impl SealState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uninitialized => "uninitialized",
            Self::Sealed => "sealed",
            Self::Unsealed => "unsealed",
        }
    }
}

impl fmt::Display for SealState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsealMethod {
    Passphrase,
    Recovery,
}

impl UnsealMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passphrase => "passphrase",
            Self::Recovery => "recovery",
        }
    }
}
