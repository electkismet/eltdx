from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

from eltdx.protocol import COMMANDS


REPO_ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = REPO_ROOT / "docs" / "assets" / "interface-catalog-data.js"
README_BANNER_PATH = REPO_ROOT / ".github" / "assets" / "eltdx-readme-banner.png"
SPONSOR_BANNER_PATH = REPO_ROOT / "docs" / "assets" / "astlane-sponsor.svg"


def _catalog() -> dict:
    text = CATALOG_PATH.read_text(encoding="utf-8")
    prefix = "window.ELTDX_CATALOG = "
    assert text.startswith(prefix)
    payload = text[len(prefix) :].strip()
    assert payload.endswith(";")
    return json.loads(payload[:-1])


def _taxonomy_assignments(catalog: dict) -> dict[str, tuple[str, str | None]]:
    items = catalog["items"]
    item_ids = {item["id"] for item in items}
    assignments: dict[str, tuple[str, str | None]] = {}

    def assign(item_id: str, layer_id: str, group_id: str | None) -> None:
        assert item_id in item_ids
        assert item_id not in assignments
        assignments[item_id] = (layer_id, group_id)

    for layer in catalog["taxonomy"]["layers"]:
        for item_id in layer.get("item_ids", []):
            assign(item_id, layer["id"], None)
        for group in layer.get("groups", []):
            for item_id in group.get("item_ids", []):
                assign(item_id, layer["id"], group["id"])

    for layer in catalog["taxonomy"]["layers"]:
        for group in layer.get("groups", []):
            if source := group.get("source"):
                for item in items:
                    if item["id"] not in assignments and item["source"] == source:
                        assign(item["id"], layer["id"], group["id"])

    for layer in catalog["taxonomy"]["layers"]:
        if source := layer.get("source"):
            for item in items:
                if item["id"] not in assignments and item["source"] == source:
                    assign(item["id"], layer["id"], None)

    assert set(assignments) == item_ids
    return assignments


def test_pages_catalog_has_expected_public_interfaces() -> None:
    catalog = _catalog()
    items = catalog["items"]

    assert catalog["schema_version"] == 11
    assert len(items) == 57
    assert Counter(item["source"] for item in items) == {
        "7709": 21,
        "F10": 21,
        "Helper": 15,
    }
    assert len({item["id"] for item in items}) == len(items)


def test_catalog_labels_every_multi_call_entry() -> None:
    catalog = _catalog()
    multi_call_items = [item for item in catalog["items"] if " / " in item["api"]]

    assert multi_call_items
    assert all(item.get("calls") for item in multi_call_items)
    for item in multi_call_items:
        assert all(set(call) == {"label", "api"} for call in item["calls"])

    items = {item["id"]: item for item in catalog["items"]}
    assert items["7709-code-count"]["api"] == "client.codes.count(market)"
    assert [call["label"] for call in items["7709-code-count"]["calls"]] == ["市场全部代码数", "仅 A 股数量"]
    assert [call["label"] for call in items["7709-code-list"]["calls"]] == [
        "推荐 · 市场全量",
        "常用 · 沪深北 A 股",
        "手动分页",
    ]
    assert [call["label"] for call in items["7709-special-limits"]["calls"]] == ["单页读取", "连续扫描"]


def test_multi_call_detail_docs_mirror_catalog_roles() -> None:
    for item in _catalog()["items"]:
        calls = item.get("calls")
        if not calls:
            continue
        detail = (REPO_ROOT / "docs" / item["doc"]).read_text(encoding="utf-8")
        for call in calls:
            assert f"| {call['label']} |" in detail, item["id"]
            assert call["api"].split("(", 1)[0] in detail, item["id"]


def test_catalog_detail_pages_hide_global_navigation() -> None:
    detail_docs = {item["doc"] for item in _catalog()["items"]}
    expected_header = """---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }
"""

    assert len(detail_docs) == 56
    for relative_path in detail_docs:
        detail = (REPO_ROOT / "docs" / relative_path).read_text(encoding="utf-8")
        assert detail.startswith(expected_header), relative_path
        assert "  - toc" not in detail.split("---", 2)[1], relative_path


def test_pages_catalog_has_three_flat_source_menus() -> None:
    catalog = _catalog()
    ordered_layers = catalog["taxonomy"]["layers"]
    assignments = _taxonomy_assignments(catalog)

    assert [(layer["id"], layer["label"]) for layer in ordered_layers] == [
        ("7709", "7709 原生协议接口"),
        ("7615", "7615 原生 Entry 接口"),
        ("helpers", "Helpers 封装"),
    ]
    assert Counter(layer_id for layer_id, _ in assignments.values()) == {
        "7709": 21,
        "7615": 21,
        "helpers": 15,
    }
    assert all("groups" not in layer for layer in ordered_layers)
    assert {layer["source"] for layer in ordered_layers} == {"7709", "F10", "Helper"}
    assert assignments["f10-generic-entry"] == ("7615", None)
    assert assignments["7709-turnover"] == ("helpers", None)
    assert assignments["helper-server-stats"] == ("helpers", None)
    assert all(item["source"] != "MCP" for item in catalog["items"])
    assert (REPO_ROOT / "docs" / "MCP.md").is_file()


def test_pages_catalog_has_complete_function_menus() -> None:
    catalog = _catalog()
    groups = catalog["taxonomy"]["functional_groups"]
    items = catalog["items"]
    assigned: dict[str, str] = {}
    for group in groups:
        for item_id in group.get("item_ids", []):
            assert item_id not in assigned
            assigned[item_id] = group["id"]
    for group in groups:
        for item in items:
            if item["id"] not in assigned and item["category"] in group.get("categories", []):
                assigned[item["id"]] = group["id"]

    assert set(assigned) == {item["id"] for item in items}
    assert len({group["id"] for group in groups}) == len(groups)
    assert [(group["id"], group["label"]) for group in groups[:3]] == [
        ("basics", "基础接口（非手动调用）"),
        ("codes", "证券代码"),
        ("entry", "高级调用（需手动指定 Entry 和参数）"),
    ]
    assert sum(1 for group_id in assigned.values() if group_id == "basics") == 2
    assert sum(1 for group_id in assigned.values() if group_id == "codes") == 2
    assert sum(1 for group_id in assigned.values() if group_id == "entry") == 1
    assert sum(1 for group_id in assigned.values() if group_id == "quotes") == 15
    assert sum(1 for group_id in assigned.values() if group_id == "company") == 17


def test_pages_catalog_wraps_long_mobile_labels() -> None:
    styles = (REPO_ROOT / "docs" / "assets" / "interface-catalog.css").read_text(encoding="utf-8")

    mobile = styles.split("@media screen and (max-width: 38rem)", 1)[1]
    assert ".catalog-heading h1" in mobile
    assert "overflow-wrap: anywhere" in mobile
    assert ".interface-stat span" in mobile
    assert "white-space: normal" in mobile


def test_pages_catalog_covers_every_registered_7709_command() -> None:
    catalog = _catalog()
    assignments = _taxonomy_assignments(catalog)
    binary_ids = {item_id for item_id, (layer_id, _) in assignments.items() if layer_id == "7709"}
    binary_protocols = [item["protocol"].lower() for item in catalog["items"] if item["id"] in binary_ids]

    assert len(COMMANDS) == len(binary_ids) == 21
    assert Counter(binary_protocols) == Counter(command.hex for command in COMMANDS.values())


def test_pages_catalog_links_to_existing_docs() -> None:
    for item in _catalog()["items"]:
        doc_path = REPO_ROOT / "docs" / item["doc"]
        assert doc_path.is_file(), item["id"]
        if anchor := item.get("doc_anchor"):
            assert f"{{#{anchor}}}" in doc_path.read_text(encoding="utf-8"), item["id"]


def test_quote_command_docs_explain_the_three_distinct_roles() -> None:
    command_map = (REPO_ROOT / "docs" / "COMMANDS_7709.md").read_text(encoding="utf-8")
    assert "## 三个行情命令的边界" in command_map
    assert "不能互换解析器" in command_map
    assert "client.helpers.full_quotes(codes)" in command_map

    expected_roles = {
        "7709-批量快照.md": "无游标的一次性基础快照",
        "7709-增量刷新推送队列.md": "按代码和游标刷新行情",
        "7709-旧版批量行情.md": "无游标的旧版完整快照",
    }
    for filename, role in expected_roles.items():
        text = (REPO_ROOT / "docs" / "methods" / filename).read_text(encoding="utf-8")
        lower_text = text.lower()
        assert "## 与 `0x" in text
        assert role in text
        assert all(command in lower_text for command in ("0x054c", "0x0547", "0x053e"))

    items = {item["id"]: item for item in _catalog()["items"]}
    assert "一次性基础快照" in items["7709-quote-snapshots"]["summary"]
    assert "旧版完整快照" in items["7709-legacy-quotes"]["summary"]
    assert "代码和游标" in items["7709-quote-refresh"]["summary"]


def test_quote_catalog_keeps_each_public_entry_in_one_primary_doc() -> None:
    items = {item["id"]: item for item in _catalog()["items"]}
    snapshot = items["7709-quote-snapshots"]
    complete = items["7709-quote-depth"]
    refresh = items["7709-quote-refresh"]

    assert snapshot["api"] == "client.quotes.get_snapshots(codes)"
    assert snapshot["doc"] == "methods/7709-批量快照.md"
    assert complete["api"] == "client.helpers.full_quotes(codes)"
    assert complete["doc"] == "helpers/完整行情.md"
    assert "get_depth" not in complete["api"]
    assert "client.quotes.get_depth()" in refresh["api"]
    assert refresh["doc"] == "methods/7709-增量刷新推送队列.md"

    snapshot_doc = (REPO_ROOT / "docs" / snapshot["doc"]).read_text(encoding="utf-8")
    complete_doc = (REPO_ROOT / "docs" / complete["doc"]).read_text(encoding="utf-8")
    assert snapshot_doc.count("client.helpers.full_quotes(") == 1
    assert snapshot_doc.count("client.quotes.get_depth(") == 1
    assert "主要调用 | `client.quotes.get_snapshots(codes)`" in snapshot_doc
    assert "from eltdx import TdxClient" in snapshot_doc
    assert "quotes = client.quotes.get_snapshots(" in snapshot_doc
    assert "client.helpers.full_quotes(codes)" in complete_doc


def test_file_resource_catalog_documents_download_and_stats_parsing() -> None:
    items = {item["id"]: item for item in _catalog()["items"]}
    resource = items["7709-file-content"]
    helper = items["helper-server-stats"]
    method_doc = (REPO_ROOT / "docs" / resource["doc"]).read_text(encoding="utf-8")

    assert resource["protocol"].lower() == "0x06b9"
    assert "read(" in resource["api"] and "download_file()" not in resource["api"]
    assert resource["return_model"] == "FileContentChunk"
    assert helper["source"] == "Helper"
    assert all(name in helper["api"] for name in ("download_file()", "read_stats()"))
    assert "TdxStatsResource" in helper["return_model"]
    assert helper["doc_anchor"] == "stats-resource"
    assert "不是新的二进制命令" in method_doc
    assert all(name in method_doc for name in ("tdxstat.cfg", "tdxstat2.cfg", "free_float_shares_10k", "open_amount_10k"))


def test_shortline_indicator_docs_explain_all_field_meanings() -> None:
    doc = (REPO_ROOT / "docs" / "helpers" / "短线指标.md").read_text(encoding="utf-8")
    fields = {
        "beta_60d",
        "pe_ttm",
        "free_float_shares",
        "prev_amount",
        "prev_seal_amount",
        "prev2_seal_amount",
        "prev_open_volume_hand",
        "prev_open_amount",
        "limit_stat_days",
        "limit_up_count_in_stat_days",
        "limit_up_streak_days",
        "year_limit_up_days",
        "free_float_market_value",
        "open_turnover_z",
        "open_prev_amount_ratio",
        "auction_prev_volume_ratio",
        "open_prev_seal_ratio",
        "seal_to_float_ratio",
        "seal_prev_ratio",
        "limit_board_text",
        "ladder_level",
    }

    assert all(f"`{field}`" in doc for field in fields)
    assert all(heading in doc for heading in ("中文名称", "业务含义", "单位"))
    assert "不是统计学里的 Z-score" in doc
    assert "必须结合 `limit_status` 使用" in doc


def test_pages_remains_static_and_outside_runtime_dependencies() -> None:
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(encoding="utf-8")
    pyproject = (REPO_ROOT / "pyproject.toml").read_text(encoding="utf-8")
    gitignore = (REPO_ROOT / ".gitignore").read_text(encoding="utf-8")

    assert "fetch(" not in app
    assert "XMLHttpRequest" not in app
    assert "WebSocket" not in app
    assert "dependencies = []" in pyproject
    assert "site/" in gitignore.splitlines()


def test_pages_catalog_ui_exposes_taxonomy_navigation() -> None:
    page = (REPO_ROOT / "docs" / "index.md").read_text(encoding="utf-8")
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(encoding="utf-8")
    styles = (REPO_ROOT / "docs" / "assets" / "interface-catalog.css").read_text(encoding="utf-8")

    assert "data-interface-tree" in page
    assert "data-interface-scope-select" in page
    assert 'data-catalog-view="function"' in page
    assert 'data-catalog-view="interface"' in page
    assert 'function/all' in app
    assert 'catalogView' in app
    assert "window.location.hash" in app
    assert '"wrapper/helpers": "helpers"' in app
    assert '"7709/commands": "7709"' in app
    assert '"7615/features": "7615"' in app
    assert 'return "7709";' in app
    assert "catalog-tree-leaf" in app
    assert "按 7709 原生协议接口、7615 原生 Entry 接口和 Helpers 封装组织" in app
    assert 'classList.toggle("interface-catalog-page"' in app
    assert ".interface-catalog-page .md-grid" in styles
    assert "\n.md-grid {\n" not in styles


def test_interface_details_promote_back_link_to_header() -> None:
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(encoding="utf-8")
    styles = (REPO_ROOT / "docs" / "assets" / "interface-catalog.css").read_text(encoding="utf-8")
    config = (REPO_ROOT / "mkdocs.yml").read_text(encoding="utf-8")

    assert app.index("promoteDetailBackLink();") < app.index('if (!root)')
    assert 'header.insertBefore(link, logo)' in app
    assert 'window.history.back()' in app
    assert 'document.referrer && window.history.length > 1' in app
    assert 'link.setAttribute("aria-label", "返回上一页")' in app
    assert ".md-header .interface-header-back" in styles
    assert "primary: red" in config
    assert "primary: teal" not in config
    assert "#087f72" not in styles


def test_readme_promotes_the_static_pages_catalog() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    banner = README_BANNER_PATH.read_bytes()

    assert '<a href="https://electkismet.github.io/eltdx/"><strong>接口一览</strong></a>' in readme
    assert "<strong>在线文档</strong>" not in readme
    assert 'src=".github/assets/eltdx-readme-banner.png"' in readme
    assert README_BANNER_PATH.is_file()
    assert banner.startswith(b"\x89PNG\r\n\x1a\n")
    assert (int.from_bytes(banner[16:20], "big"), int.from_bytes(banner[20:24], "big")) == (1250, 696)


def test_readme_shows_the_astlane_sponsor_banner() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    sponsor = SPONSOR_BANNER_PATH.read_text(encoding="utf-8")

    catalog_position = readme.index('src=".github/assets/eltdx-readme-banner.png"')
    sponsor_position = readme.index('src="docs/assets/astlane-sponsor.svg"')
    assert catalog_position < sponsor_position
    assert 'href="https://api.astlane.com/"' in readme
    assert '<title id="title">Astlane 赞助 eltdx token</title>' in sponsor


def test_current_docs_match_v2_pagination_and_parameter_contracts() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    code_count = (REPO_ROOT / "docs" / "methods" / "7709-代码数量.md").read_text(encoding="utf-8")
    code_table = (REPO_ROOT / "docs" / "methods" / "7709-代码表.md").read_text(encoding="utf-8")
    all_bars = (REPO_ROOT / "docs" / "methods" / "7709-全量K线分页.md").read_text(encoding="utf-8")
    finance = (REPO_ROOT / "docs" / "methods" / "7709-财务基础信息.md").read_text(encoding="utf-8")

    assert "client.codes.count()" not in readme
    assert "不需要先查数量" in code_count
    assert "不需要先调用 `client.codes.count(market)`" in code_table
    assert "不限 A 股" in code_count
    assert "client.codes.a_share_count(market)" in code_count
    assert "`refresh`" not in code_count
    assert "`refresh`" not in code_table
    assert "不接受 `refresh` 参数" in finance
    assert "| 停止条件 | 主站返回空页 |" in code_table
    assert "| 停止条件 | 主站返回空页 |" in all_bars
    assert "短页不代表数据已经结束" in code_table
    assert "短页不代表数据已经结束" in all_bars


def test_current_docs_match_v2_cache_and_migration_contracts() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    fields = (REPO_ROOT / "docs" / "FIELD_REFERENCE.md").read_text(encoding="utf-8")
    methods = (REPO_ROOT / "docs" / "METHOD_REFERENCE.md").read_text(encoding="utf-8")
    migration = (REPO_ROOT / "docs" / "FIELD_MIGRATION.md").read_text(encoding="utf-8")
    historical_update = (REPO_ROOT / "docs" / "UPDATE_FROM_0_5_1.md").read_text(encoding="utf-8")

    for text in (readme, fields, methods):
        assert "代码数量、全量代码表、股本变迁、财务" not in text
    assert "`client.codes.count()`、`client.codes.all()` 和 `client.corporate.finance_batch()` 都会直接请求主站" in fields
    assert 'client.bars.get("sz000001", period="day", include_raw=True)' in migration
    assert 'client.helpers.factors("sz000001")' in migration
    assert 'client.helpers.capital_changes("sz000001", refresh=True)' in migration
    assert 'warning "历史版本文档"' in historical_update
    assert "当前 `v2.0.1` 已移除这些入口" in historical_update


def test_trade_docs_describe_mixed_records_instead_of_only_trades() -> None:
    fields = (REPO_ROOT / "docs" / "FIELD_REFERENCE.md").read_text(encoding="utf-8")
    methods = (REPO_ROOT / "docs" / "METHOD_REFERENCE.md").read_text(encoding="utf-8")
    today = (REPO_ROOT / "docs" / "methods" / "7709-当日成交明细.md").read_text(encoding="utf-8")
    history = (REPO_ROOT / "docs" / "methods" / "7709-历史成交明细.md").read_text(encoding="utf-8")

    for text in (methods, today, history):
        assert "实际成交条数" not in text
        assert "混合记录条数" in text
    assert "竞价快照时是虚拟匹配量" in fields
    assert "竞价快照仅为估算，不代表实际成交金额" in methods
    assert 'client.trades.all_history("sz000001", "2026-05-20")' in methods
