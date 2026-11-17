"""Authored compatibility contracts; deferred until the full source gate."""
from __future__ import annotations

import pytest
import math
import struct
from types import SimpleNamespace

from eltdx.offline import history_financial as financial
from eltdx import financial as financial_api


def test_file_list_keeps_unicode_signed_arbitrary_precision_and_raw_fields():
    data = (
        "  first.zip , raw hash , -١_٢  ,ignored\n"
        "large.zip,hash,184467440737095516160000\n"
        "zero.zip,hash,-0\n"
        "skip.zip,hash,1__2\n"
        "missing,field\n"
    ).encode()
    assert financial.parse_financial_file_list(data) == [
        ("first.zip ", " raw hash ", -12),
        ("large.zip", "hash", 184467440737095516160000),
        ("zero.zip", "hash", 0),
    ]


def test_file_list_keeps_replacement_and_python_whitespace():
    assert financial.parse_financial_file_list(b"\x1c\xff.zip,hash,2\x1f\n") == [("\ufffd.zip", "hash", 2)]
    assert financial.parse_financial_file_list(b"") == []


def test_file_list_custom_decode_object_keeps_python_adapter():
    class Source:
        def decode(self, encoding, *, errors):
            assert (encoding, errors) == ("utf-8", "replace")
            return "custom.zip,hash,9"

    assert financial.parse_financial_file_list(Source()) == [("custom.zip", "hash", 9)]


def test_file_list_uses_runtime_integer_and_only_skips_value_error(monkeypatch):
    calls = []

    def integer(value):
        calls.append(value)
        if value == "skip":
            raise ValueError("ignored")
        if value == "stop":
            raise TypeError("original integer failure")
        return 27

    monkeypatch.setattr(financial, "int", integer, raising=False)
    assert financial.parse_financial_file_list(b"a,h,skip\nb,h,ok") == [("b", "h", 27)]
    assert calls == ["skip", "ok"]
    with pytest.raises(TypeError, match="original integer failure"):
        financial.parse_financial_file_list(b"c,h,stop")


def _dat(*, size=4, date=20260331, entries=None, payload=None):
    if entries is None:
        entries = [(b"600000", 255, 31)]
    if payload is None:
        payload = struct.pack("<f", 1.5)
    return (
        struct.pack("<hIHIII", -7, date, len(entries), 0, size, 0)
        + b"".join(struct.pack("<6sBI", *row) for row in entries)
        + payload
    )


@pytest.mark.parametrize("date", [-1, 2**100, None, "20260331"])
def test_dat_preserves_arbitrary_report_date_object(date):
    rows = financial.parse_financial_dat(_dat(), date)
    assert rows == [("600000", 255, date, [1.5])]
    assert rows[0][2] is date


def test_dat_date_comparison_precedes_zero_size_and_field_limit(monkeypatch):
    events = []

    class Date:
        def __eq__(self, other):
            events.append(("date", other))
            monkeypatch.setattr(financial, "MAX_HISTORY_REPORT_FIELDS", -1)
            return False

    date = Date()
    assert financial.parse_financial_dat(_dat(size=0), date) == []
    assert events == [("date", 0)]
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(_dat(), date)
    assert str(caught.value) == "财报记录字段数超出上限: 1 > -1（report_size=4）"


def test_dat_short_header_does_not_compare_date_or_load_extension(monkeypatch):
    from eltdx import _native_abi

    def unexpected(*args):
        raise AssertionError("short input must return before native/date work")

    class Date:
        __eq__ = unexpected

    monkeypatch.setattr(_native_abi, "load_native", unexpected)
    assert financial.parse_financial_dat(b"\x00" * 19, Date()) == []


def test_dat_exact_alignment_and_runtime_total_limit_errors(monkeypatch):
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(_dat(size=6))
    assert str(caught.value) == "财报记录长度非法: report_size=6（必须是 4 的整数倍）"
    monkeypatch.setattr(financial, "MAX_HISTORY_TOTAL_FLOATS", 1)
    data = _dat(entries=[(b"600000", 1, 42), (b"000001", 0, 46)], payload=struct.pack("<2f", 1, 2))
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(data)
    assert str(caught.value) == "财报总解析量超出上限: 2 > 1 个字段（已解析 2/2 条，report_size=4）"


def test_dat_duplicate_accepted_offset_and_repeated_invalid_offsets():
    data = _dat(entries=[(b"600000", 1, 42), (b"000001", 0, 42)])
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.parse_financial_dat(data)
    assert str(caught.value) == "财报索引偏移重复: offset=42（第 1 条，code=000001）"
    invalid = _dat(entries=[(b"600000", 1, 2**32-1), (b"000001", 0, 2**32-1)])
    assert financial.parse_financial_dat(invalid) == []


def test_dat_unique_overlap_ascii_replacement_and_float_projection():
    data = _dat(entries=[(b"A\xffB", 255, 42), (b"000001", 0, 43)], payload=b"\x00" * 5)
    assert financial.parse_financial_dat(data) == [
        ("A\ufffdB", 255, 20260331, [0.0]), ("000001", 0, 20260331, [0.0])
    ]
    values = financial.parse_financial_dat(_dat(size=16, payload=struct.pack("<4I", 0x80000000, 0x7F800000, 0xFF800000, 0x7FC12345)))[0][3]
    assert math.copysign(1.0, values[0]) == -1
    assert values[1:3] == [math.inf, -math.inf]
    assert math.isnan(values[3])


def test_dat_custom_buffers_and_struct_override_keep_adapter(monkeypatch):
    data = _dat()
    assert financial.parse_financial_dat(bytearray(data)) == [("600000", 255, 20260331, [1.5])]
    calls = []
    unpack = struct.unpack

    def custom_unpack(fmt, value):
        calls.append(fmt)
        return unpack(fmt, value)

    monkeypatch.setattr(financial.struct, "unpack", custom_unpack)
    assert financial.parse_financial_dat(data) == [("600000", 255, 20260331, [1.5])]
    assert calls == ["<1hI1H3L", "<6s1c1L", "<1f"]


def test_dat_streaming_limit_keeps_runtime_comparisons_and_exception_identity(monkeypatch):
    calls = []
    failure = RuntimeError("custom limit comparison")

    class Total:
        def __lt__(self, total):
            calls.append(total)
            if total == 2:
                raise failure
            return False

    monkeypatch.setattr(financial, "MAX_HISTORY_TOTAL_FLOATS", Total())
    data = _dat(entries=[(b"600000", 1, 42), (b"000001", 0, 46)], payload=struct.pack("<2f", 1, 2))
    with pytest.raises(RuntimeError) as caught:
        financial.parse_financial_dat(data)
    assert caught.value is failure
    assert calls == [1, 2]


def test_dat_keeps_incomplete_index_and_short_data_skips():
    # One accepted row, one out-of-range row and a final incomplete index.
    data = bytearray(_dat(entries=[(b"600000", 1, 20), (b"000001", 0, 500)]))
    data[6:8] = struct.pack("<H", 3)
    rows = financial.parse_financial_dat(bytes(data))
    assert len(rows) == 1
    assert rows[0][:3] == ("600000", 1, 20260331)


class _Archive:
    def __init__(self, infos, payload=b"data"):
        self.infos = infos
        self.payload = payload
        self.reads = []

    def infolist(self):
        return self.infos

    def read(self, info):
        self.reads.append(info)
        return self.payload


def _info(name, *, size=4, flags=0):
    return SimpleNamespace(filename=name, file_size=size, flag_bits=flags)


def test_zip_selects_first_distinct_dat_and_reads_exact_validated_info():
    first = _info("first.dat")
    archive = _Archive([_info("UPPER.DAT"), first, _info("second.dat", size=999, flags=1)])
    assert financial.read_dat_from_zip(archive, max_dat_bytes=4) == b"data"
    assert archive.reads == [first]


def test_zip_duplicate_names_are_sorted_and_rejected_before_reading():
    archive = _Archive([_info("z.dat"), _info("a.dat"), _info("z.dat"), _info("a.dat")])
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.read_dat_from_zip(archive, source="archive.zip")
    assert str(caught.value) == "zip 中存在重复的 .dat 文件名: ['a.dat', 'z.dat']（archive.zip）"
    assert archive.reads == []


@pytest.mark.parametrize(
    "infos,options,expected",
    [
        ([_info("a.dat"), _info("b.dat")], {"max_entries": 1}, "zip 条目过多: 2 > 1（<zip>）"),
        ([_info("A.DAT")], {}, "zip 中未找到 .dat 文件: <zip>"),
        ([_info("a.dat", flags=1)], {}, "zip 条目已加密: a.dat"),
        ([_info("a.dat", size=5)], {"max_dat_bytes": 4}, "a.dat 解压后超出上限 4 字节（声明 5 字节，<zip>）"),
    ],
)
def test_zip_errors_preserve_order_and_messages(infos, options, expected):
    archive = _Archive(infos)
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.read_dat_from_zip(archive, **options)
    assert str(caught.value) == expected
    assert archive.reads == []


def test_zip_actual_size_and_custom_read_exception_identity():
    archive = _Archive([_info("a.dat", size=4)], b"12345")
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.read_dat_from_zip(archive, max_dat_bytes=4)
    assert str(caught.value) == "a.dat 实际解压长度 5 超出上限 4 字节（<zip>）"
    failure = RuntimeError("custom ZIP reader")

    def read(info):
        raise failure

    archive.read = read
    with pytest.raises(RuntimeError) as caught:
        financial.read_dat_from_zip(archive)
    assert caught.value is failure


def test_zip_keeps_directory_named_dat_and_read_return_identity():
    # Legacy selection depends only on suffix, unlike strict Rust ZIP metadata.
    info = _info("directory.dat")
    info.is_dir = lambda: True
    payload = bytearray(b"data")
    archive = _Archive([info], payload)
    assert financial.read_dat_from_zip(archive) is payload


def test_zip_infolist_exception_identity_and_runtime_limits():
    failure = OSError("custom inventory failed")
    archive = _Archive([])

    def infolist():
        raise failure

    archive.infolist = infolist
    with pytest.raises(OSError) as caught:
        financial.read_dat_from_zip(archive)
    assert caught.value is failure
    archive = _Archive([_info("a.dat")])
    assert financial.read_dat_from_zip(archive, max_dat_bytes=float("inf"), max_entries=float("inf")) == b"data"


def test_financial_list_composition_keeps_resource_and_model_hooks(monkeypatch):
    calls = []

    class Resources:
        def download_file(self, path):
            calls.append(path)
            return b"source"

    monkeypatch.setattr(financial_api, "parse_financial_file_list", lambda data: [("f", "h", -3)])
    monkeypatch.setattr(financial_api, "FinancialFileInfo", lambda **kw: kw)
    assert financial_api.list_archive_files(SimpleNamespace(resources=Resources())) == [
        {"filename": "f", "hash": "h", "filesize": -3}
    ]
    assert calls == [financial_api.FINANCIAL_LIST_PATH]


def test_financial_archive_forwards_original_path_and_runtime_limit(monkeypatch):
    path = object()
    result = object()
    limit = object()
    calls = []

    class Resources:
        def download_file(self, filename, *, max_bytes):
            calls.append((filename, max_bytes))
            return result

    monkeypatch.setattr(financial_api, "MAX_FINANCIAL_ZIP_BYTES", limit)
    assert financial_api.download_archive(SimpleNamespace(resources=Resources()), path) is result
    assert calls == [(path, limit)]


def test_financial_record_projection_uses_runtime_parser_and_models(monkeypatch):
    import io
    import zipfile

    payload = io.BytesIO()
    with zipfile.ZipFile(payload, "w") as archive:
        archive.writestr("first.dat", b"selected")
        archive.writestr("second.dat", b"ignored")
    calls = []
    original_date = object()

    def parser(data, date):
        calls.append((data, date))
        return [("custom", 7, original_date, [1, 2])]

    monkeypatch.setattr(financial_api, "download_archive", lambda client, name: payload.getvalue())
    monkeypatch.setattr(financial_api, "parse_financial_dat", parser)
    monkeypatch.setattr(financial_api, "FinancialRecord", lambda **kw: kw)
    rows = financial_api.download_records(object(), "tdxfin/gpcw２０２６０３３１.zip")
    assert calls == [(b"selected", 20260331)]
    assert rows == [{"code": "custom", "market_id": 7, "report_date": original_date, "fields": (1, 2)}]


def test_financial_empty_download_returns_before_zip_and_date_hooks(monkeypatch):
    monkeypatch.setattr(financial_api, "download_archive", lambda *args: b"")

    def unexpected(*args, **kwargs):
        raise AssertionError("empty archive should return immediately")

    monkeypatch.setattr(financial_api.zipfile, "ZipFile", unexpected)
    assert financial_api.download_records(object(), object()) == []


def test_local_financial_projection_keeps_parser_and_record_hooks(tmp_path, monkeypatch):
    path = tmp_path / "gpcw20991231.dat"
    path.write_bytes(b"source")
    calls = []

    def parser(data):
        calls.append(data)
        return [("code", 3, -1, [2])]

    monkeypatch.setattr(financial, "parse_financial_dat", parser)
    monkeypatch.setattr(financial, "FinancialRecord", lambda **kw: kw)
    assert financial.read_history_financial(path) == [
        {"code": "code", "market_id": 3, "report_date": -1, "fields": (2,)}
    ]
    assert calls == [b"source"]


def test_zip_inventory_keeps_interleaved_iteration_and_filename_access():
    events = []
    failure = RuntimeError("filename access")

    class Info:
        @property
        def filename(self):
            events.append("filename")
            raise failure

    class Inventory:
        def __len__(self):
            return 2

        def __iter__(self):
            events.append("first")
            yield Info()
            events.append("second")
            raise AssertionError("must not fetch ahead of the first filename")

    archive = _Archive(Inventory())
    with pytest.raises(RuntimeError) as caught:
        financial.read_dat_from_zip(archive)
    assert caught.value is failure
    assert events == ["first", "filename"]


def test_financial_projection_preserves_iterable_unpack_errors(monkeypatch):
    from eltdx._native_abi import load_native

    native = load_native()
    with pytest.raises(ValueError, match="not enough values to unpack \\(expected 4, got 3\\)"):
        native.financial_project_records([(1, 2, 3)], financial)
    consumed = []

    def too_long():
        for value in range(7):
            consumed.append(value)
            yield value

    with pytest.raises(ValueError, match="too many values to unpack \\(expected 4\\)"):
        native.financial_project_records([too_long()], financial)
    assert consumed == [0, 1, 2, 3, 4]


def test_financial_projection_fetches_constructor_before_fields_conversion(monkeypatch):
    from eltdx._native_abi import load_native

    calls = []

    def first(**fields):
        calls.append("first")
        return fields

    def second(**fields):
        calls.append("second")
        return fields

    class Fields:
        def __iter__(self):
            monkeypatch.setattr(financial, "FinancialRecord", second)
            yield 7

    monkeypatch.setattr(financial, "FinancialRecord", first)
    rows = load_native().financial_project_records([("a", 0, 1, Fields()), ("b", 0, 2, [])], financial)
    assert calls == ["first", "second"]
    assert rows[0]["fields"] == (7,)


def test_financial_filename_date_retains_first_match_and_runtime_integer(monkeypatch):
    from eltdx._native_abi import load_native

    assert load_native().financial_filename_date("gpcw20269999_20260331.zip", financial_api) == 20269999
    marker = object()
    calls = []

    def integer(value):
        calls.append(value)
        return marker

    monkeypatch.setattr(financial_api, "int", integer, raising=False)
    assert load_native().financial_filename_date("gpcw２０２６０３３１.zip", financial_api) is marker
    assert calls == ["２０２６０３３１"]


def test_financial_badzip_cause_and_truncated_download_behavior(monkeypatch):
    import zipfile

    monkeypatch.setattr(financial_api, "download_archive", lambda *args: b"PK truncated")
    with pytest.raises(financial.ResourceFormatError) as caught:
        financial_api.download_records(object(), "tdxfin/gpcw.zip")
    assert str(caught.value) == "无效的财报 zip 文件: tdxfin/gpcw.zip"
    assert isinstance(caught.value.__cause__, zipfile.BadZipFile)


def test_zip_source_uses_python_format_protocol():
    class Source:
        def __str__(self):
            return "wrong"

        def __format__(self, spec):
            assert spec == ""
            return "formatted-source"

    with pytest.raises(financial.ResourceFormatError) as caught:
        financial.read_dat_from_zip(_Archive([]), source=Source())
    assert str(caught.value) == "zip 中未找到 .dat 文件: formatted-source"


def test_local_financial_missing_path_and_case_insensitive_zip(tmp_path, monkeypatch):
    missing = tmp_path / "absent.dat"
    with pytest.raises(FileNotFoundError) as caught:
        financial.read_history_financial(missing)
    assert str(caught.value) == f"历史财务数据文件不存在: {missing}"
    path = tmp_path / "report.ZIP"
    path.write_bytes(b"archive-adapter")
    calls = []

    def reader(value):
        calls.append(value)
        return b"decoded"

    monkeypatch.setattr(financial, "_read_from_zip", reader)
    monkeypatch.setattr(financial, "parse_financial_dat", lambda data: [])
    assert financial.read_history_financial(path) == []
    assert calls == [path]


def test_financial_calc_constructor_keeps_lazy_arguments(monkeypatch):
    import eltdx.client

    calls = []
    monkeypatch.setattr(eltdx.client, "TdxClient", lambda **args: calls.append(args) or args)
    assert financial_api.open_calc_client("", timeout=2.5) == {
        "host": financial_api.CALC_HOSTS[0], "timeout": 2.5, "probe_hosts": False,
    }
    assert financial_api.open_calc_client("server", timeout=9.0)["host"] == "server"
    assert len(calls) == 2
