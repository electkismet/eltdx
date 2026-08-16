"""Public text codec compatibility helpers."""


def encode(message: str) -> bytes:
    """Encode a text message to bytes."""

    return message.encode("utf-8")


def decode(payload: bytes) -> str:
    """Decode bytes to a text message."""

    return payload.decode("utf-8")
