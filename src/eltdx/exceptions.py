class EltdxError(Exception):
    pass


class ProtocolError(EltdxError):
    pass


class ResponseTimeoutError(EltdxError):
    pass


class ConnectionClosedError(EltdxError):
    pass

