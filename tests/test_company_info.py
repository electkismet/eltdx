"""F10 company-info and block-file metadata command tests."""

from __future__ import annotations

from eltdx.api.company_info import CompanyInfoApi
from eltdx.api.resources import ResourceApi
from eltdx.models import BlockMeta, CompanyInfoCategory
from eltdx.protocol.commands import COMMANDS, command_code
from eltdx.protocol.constants import (
    TYPE_BLOCK_META,
    TYPE_COMPANY_INFO_CATEGORY,
    TYPE_COMPANY_INFO_CONTENT,
)
from eltdx.transport.memory import InMemoryTransport


def test_company_info_category_assembles_payload_and_returns_catalog() -> None:
    transport = InMemoryTransport()
    catalog = [
        CompanyInfoCategory(name="公司简介", filename="000001.txt", start=0, length=4096),
    ]
    transport.register_response(TYPE_COMPANY_INFO_CATEGORY, catalog)
    api = CompanyInfoApi(transport)

    assert api.category("000001", "sz") == catalog
    assert transport.calls[-1] == (
        TYPE_COMPANY_INFO_CATEGORY,
        {"code": "000001", "market": "sz"},
    )


def test_company_info_content_assembles_payload_and_returns_text() -> None:
    transport = InMemoryTransport()
    transport.register_response(TYPE_COMPANY_INFO_CONTENT, "公司简介正文")
    api = CompanyInfoApi(transport)

    result = api.content("000001", "sz", "000001.txt", 0, 4096)

    assert result == "公司简介正文"
    assert transport.calls[-1] == (
        TYPE_COMPANY_INFO_CONTENT,
        {
            "code": "000001",
            "market": "sz",
            "filename": "000001.txt",
            "offset": 0,
            "length": 4096,
        },
    )


def test_block_meta_assembles_payload_and_returns_metadata() -> None:
    transport = InMemoryTransport()
    transport.register_response(TYPE_BLOCK_META, BlockMeta(size=12345, md5="deadbeef"))
    api = ResourceApi(transport)

    assert api.block_meta("block_zs.dat") == BlockMeta(size=12345, md5="deadbeef")
    assert transport.calls[-1] == (TYPE_BLOCK_META, {"filename": "block_zs.dat"})


def test_unregistered_commands_return_the_memory_envelope() -> None:
    transport = InMemoryTransport()
    api = CompanyInfoApi(transport)

    assert api.category("000001", "sz") == {
        "command": "0x02cf",
        "payload": {"code": "000001", "market": "sz"},
        "status": "memory_transport",
    }


def test_registry_exposes_the_new_commands() -> None:
    assert COMMANDS["block_meta"].code == 0x02C5
    assert COMMANDS["company_info_category"].code == 0x02CF
    assert COMMANDS["company_info_content"].code == 0x02D0
    assert COMMANDS["block_meta"].required_for_1_0 is False
    assert COMMANDS["company_info_category"].hex == "0x02cf"
    assert command_code("block_meta") == TYPE_BLOCK_META
    assert command_code("company_info_category") == TYPE_COMPANY_INFO_CATEGORY
    assert command_code("company_info_content") == TYPE_COMPANY_INFO_CONTENT
