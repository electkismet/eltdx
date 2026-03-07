from .constants import TYPE_CONNECT, TYPE_HEART
from .frame import RequestFrame


def build_connect_frame(msg_id: int) -> RequestFrame:
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_CONNECT, data=b"\x01")


def build_heart_frame(msg_id: int) -> RequestFrame:
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_HEART)

