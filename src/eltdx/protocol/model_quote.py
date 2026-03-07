from __future__ import annotations

from ..exceptions import ProtocolError
from ..models import Quote, QuoteLevel
from .constants import TYPE_QUOTE
from .frame import RequestFrame, ResponseFrame
from .unit import (
    add_prefix,
    consume_price,
    consume_varint,
    decode_code,
    decode_k,
    decode_quote_datetime,
    get_volume,
    milli_to_float,
    price_divisor,
    uint16_le,
    uint32_le,
)


def build_quote_frame(codes: list[str], msg_id: int) -> RequestFrame:
    payload = bytearray([0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
    payload.extend(len(codes).to_bytes(2, "little", signed=False))
    for code in codes:
        exchange, number = decode_code(code)
        payload.append(exchange)
        payload.extend(number.encode("ascii"))
    return RequestFrame(msg_id=msg_id, msg_type=TYPE_QUOTE, data=bytes(payload))


def parse_quote_payload(codes: list[str], response: ResponseFrame) -> list[Quote]:
    payload = response.data
    if len(payload) < 4:
        raise ProtocolError("invalid quote payload")

    expected_codes = [add_prefix(code) for code in codes]
    count = uint16_le(payload[2:4])
    offset = 4
    quotes: list[Quote] = []

    for expected_code in expected_codes[:count]:
        exchange = payload[offset]
        code = payload[offset + 1 : offset + 7].decode("ascii")
        active1 = uint16_le(payload[offset + 7 : offset + 9])
        offset += 9

        quote_prices, offset = decode_k(payload, offset)
        server_time_raw, offset = consume_varint(payload, offset)
        _, offset = consume_varint(payload, offset)
        total_hand, offset = consume_varint(payload, offset)
        intuition, offset = consume_varint(payload, offset)
        amount = get_volume(uint32_le(payload[offset : offset + 4]))
        offset += 4
        inside_dish, offset = consume_varint(payload, offset)
        outer_disc, offset = consume_varint(payload, offset)
        _, offset = consume_varint(payload, offset)
        _, offset = consume_varint(payload, offset)

        buy_levels: list[QuoteLevel] = []
        sell_levels: list[QuoteLevel] = []
        current_milli = quote_prices["current"]
        divisor = price_divisor(expected_code)

        for _ in range(5):
            buy_delta, offset = consume_price(payload, offset)
            sell_delta, offset = consume_price(payload, offset)
            buy_number, offset = consume_varint(payload, offset)
            sell_number, offset = consume_varint(payload, offset)
            buy_milli = (buy_delta * 10 + current_milli) // divisor
            sell_milli = (sell_delta * 10 + current_milli) // divisor
            buy_levels.append(QuoteLevel(buy=True, price=milli_to_float(buy_milli), price_milli=buy_milli, number=buy_number))
            sell_levels.append(QuoteLevel(buy=False, price=milli_to_float(sell_milli), price_milli=sell_milli, number=sell_number))

        offset += 2
        _, offset = consume_varint(payload, offset)
        _, offset = consume_varint(payload, offset)
        _, offset = consume_varint(payload, offset)
        _, offset = consume_varint(payload, offset)
        rate = uint16_le(payload[offset : offset + 2]) / 100.0
        active2 = uint16_le(payload[offset + 2 : offset + 4])
        offset += 4

        quotes.append(
            Quote(
                exchange={0: "sz", 1: "sh", 2: "bj"}.get(exchange, "unknown"),
                code=code,
                active1=active1,
                active2=active2,
                server_time_raw=server_time_raw,
                server_time=decode_quote_datetime(server_time_raw),
                last_price=milli_to_float(quote_prices["current"] // divisor),
                last_price_milli=quote_prices["current"] // divisor,
                open_price=milli_to_float(quote_prices["open"] // divisor),
                open_price_milli=quote_prices["open"] // divisor,
                high_price=milli_to_float(quote_prices["high"] // divisor),
                high_price_milli=quote_prices["high"] // divisor,
                low_price=milli_to_float(quote_prices["low"] // divisor),
                low_price_milli=quote_prices["low"] // divisor,
                last_close_price=milli_to_float(quote_prices["last_close"] // divisor),
                last_close_price_milli=quote_prices["last_close"] // divisor,
                total_hand=total_hand,
                intuition=intuition,
                amount=amount,
                inside_dish=inside_dish,
                outer_disc=outer_disc,
                buy_levels=buy_levels,
                sell_levels=sell_levels,
                rate=rate,
            )
        )

    if len(quotes) != len(expected_codes):
        raise ProtocolError(f"expected {len(expected_codes)} quotes, got {len(quotes)}")
    return quotes
