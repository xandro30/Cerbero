class RootHolder:
    """Keeps the root in a mutable buffer that is overwritten with zeros when released.

    Best effort: Python cannot guarantee that no other copy of the bytes remains in memory
    (an accepted risk, see DECISIONS).
    """

    __slots__ = ("_buf",)

    def __init__(self, value: bytes) -> None:
        self._buf: bytearray | None = bytearray(value)

    @property
    def released(self) -> bool:
        return self._buf is None

    def get(self) -> bytes:
        if self._buf is None:
            raise ValueError("root already released")
        return bytes(self._buf)

    def release(self) -> None:
        if self._buf is not None:
            for i in range(len(self._buf)):
                self._buf[i] = 0
            self._buf = None

    def __repr__(self) -> str:
        return "RootHolder(<hidden>)"

    __str__ = __repr__
