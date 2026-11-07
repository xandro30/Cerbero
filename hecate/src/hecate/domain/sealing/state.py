from enum import StrEnum


class SealState(StrEnum):
    UNINITIALIZED = "uninitialized"
    SEALED = "sealed"
    UNSEALED = "unsealed"


class UnsealMethod(StrEnum):
    PASSPHRASE = "passphrase"
    RECOVERY = "recovery"
