from __future__ import annotations

import ast
import inspect
import json
import re
from collections import Counter
from pathlib import Path

import pytest

from eltdx.protocol import COMMANDS


REPO_ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = REPO_ROOT / "docs" / "assets" / "interface-catalog-data.js"
README_BANNER_RELATIVE = ".github/assets/eltdx-readme-banner-v3.3.3.png"
README_BANNER_PATH = REPO_ROOT / README_BANNER_RELATIVE
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


def test_common_interface_docs_have_collapsible_real_json_samples() -> None:
    mkdocs = (REPO_ROOT / "mkdocs.yml").read_text(encoding="utf-8")
    stylesheet = REPO_ROOT / "docs" / "assets" / "return-samples.css"
    catalog = _catalog()
    return_sample_docs = [
        item["doc"] for item in catalog["items"] if item["source"] in {"7709", "Helper"}
    ]
    f10_docs = [item["doc"] for item in catalog["items"] if item["source"] == "F10"]

    assert "assets/return-samples.css" in mkdocs
    assert stylesheet.is_file()
    assert len(return_sample_docs) == 49
    for relative_path in return_sample_docs:
        detail = (REPO_ROOT / "docs" / relative_path).read_text(encoding="utf-8")
        assert '??? return-sample "' in detail, relative_path
        if relative_path in {"helpers/板块行情.md", "helpers/板块成分股行情.md"}:
            assert "示例数据" in detail, relative_path
        else:
            assert "真实采样" in detail, relative_path
        assert "```json" in detail, relative_path
    for relative_path in f10_docs:
        detail = (REPO_ROOT / "docs" / relative_path).read_text(encoding="utf-8")
        assert '??? return-sample "' not in detail, relative_path


def test_pages_catalog_has_expected_public_interfaces() -> None:
    catalog = _catalog()
    items = catalog["items"]

    assert catalog["schema_version"] == 16
    assert len(items) == 124
    assert Counter(item["source"] for item in items) == {
        "7709": 26,
        "F10": 25,
        "Helper": 23,
        "EX": 12,
        "MAC": 24,
        "MAC-EX": 14,
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
    assert [call["label"] for call in items["7709-code-count"]["calls"]] == [
        "市场全部代码数",
        "仅 A 股数量",
    ]
    assert [call["label"] for call in items["7709-code-list"]["calls"]] == [
        "推荐 · 市场全量",
        "常用 · 沪深北 A 股",
        "手动分页",
    ]
    assert [call["label"] for call in items["7709-special-limits"]["calls"]] == [
        "单页读取",
        "连续扫描",
    ]


def test_market_news_catalog_entries_link_complete_individual_docs() -> None:
    catalog = _catalog()
    assert catalog["release"]["mcp_tools"] == 35
    items = {item["id"]: item for item in catalog["items"]}
    for item_id, method in (
        ("f10-market-news", "market_news"),
        ("f10-channel-news", "channel_news"),
        ("f10-news-channels", "news_channels"),
    ):
        item = items[item_id]
        text = (REPO_ROOT / "docs" / item["doc"]).read_text(encoding="utf-8")
        assert f"client.f10.{method}(" in text
        assert item["return_model"] in text
        assert f'"method": "f10.{method}"' in text
        assert "自 v3.3.2 起可用" in text
    assert items["f10-news-channels"]["kind"] == "静态枚举"


def test_batch_missing_policy_is_documented_on_both_api_pages() -> None:
    for name in ("7709-批量快照.md", "7709-财务基础信息.md"):
        text = (REPO_ROOT / "docs" / "methods" / name).read_text(encoding="utf-8")
        for term in ('on_missing="skip"', "MissingCodesError", "MissingCodesWarning", "BATCH_MISSING.md"):
            assert term in text


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
    detail_docs = {
        item["doc"]
        for item in _catalog()["items"]
        if item["source"] in {"7709", "F10", "Helper"}
    }
    expected_header = """---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }
"""

    assert len(detail_docs) == 74
    for relative_path in detail_docs:
        detail = (REPO_ROOT / "docs" / relative_path).read_text(encoding="utf-8")
        assert detail.startswith(expected_header), relative_path
        assert "  - toc" not in detail.split("---", 2)[1], relative_path


def test_pages_catalog_has_six_flat_source_menus() -> None:
    catalog = _catalog()
    ordered_layers = catalog["taxonomy"]["layers"]
    assignments = _taxonomy_assignments(catalog)

    assert [(layer["id"], layer["label"]) for layer in ordered_layers] == [
        ("7709", "7709 原生协议接口"),
        ("7615", "7615 原生 Entry 接口"),
        ("helpers", "Helpers 封装"),
        ("ex", "扩展行情（7727）"),
        ("mac", "MAC 行情"),
        ("macex", "MAC-EX 基金与外盘"),
    ]
    assert Counter(layer_id for layer_id, _ in assignments.values()) == {
        "7709": 26,
        "7615": 25,
        "helpers": 23,
        "ex": 12,
        "mac": 24,
        "macex": 14,
    }
    expected_counts = {"7709": 26, "7615": 25, "helpers": 23}
    assert all(layer["description"] for layer in ordered_layers)
    assert all(
        layer["description"].startswith(f"{expected_counts[layer['id']]} 个")
        for layer in ordered_layers
        if layer["id"] in expected_counts
    )
    assert all("groups" not in layer for layer in ordered_layers)
    assert {layer["source"] for layer in ordered_layers} == {
        "7709",
        "F10",
        "Helper",
        "EX",
        "MAC",
        "MAC-EX",
    }
    assert assignments["f10-generic-entry"] == ("7615", None)
    assert assignments["7709-local-factors"] == ("helpers", None)
    assert assignments["helper-server-stats"] == ("helpers", None)
    assert assignments["helper-server-download"] == ("helpers", None)
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
            if item["id"] not in assigned and item["category"] in group.get(
                "categories", []
            ):
                assigned[item["id"]] = group["id"]
    fallback_groups = [group for group in groups if group.get("fallback")]
    assert len(fallback_groups) == 1
    for item in items:
        if item["id"] not in assigned:
            assigned[item["id"]] = fallback_groups[0]["id"]

    assert set(assigned) == {item["id"] for item in items}
    assert len({group["id"] for group in groups}) == len(groups)
    assert [(group["id"], group["label"]) for group in groups] == [
        ("basics", "基础接口（非手动调用）"),
        ("codes", "证券代码"),
        ("realtime", "实时行情"),
        ("history", "历史行情"),
        ("bars", "K 线与复权"),
        ("auction-shortline", "集合竞价与短线"),
        ("f10", "F10"),
        ("common", "常用封装"),
        ("ex", "扩展行情（7727）"),
        ("mac", "MAC 行情"),
        ("macex", "MAC-EX 基金与外盘"),
    ]
    assert Counter(assigned.values()) == {
        "basics": 2,
        "codes": 5,
        "realtime": 15,
        "history": 4,
        "bars": 4,
        "auction-shortline": 8,
        "f10": 29,
        "common": 7,
        "ex": 12,
        "mac": 24,
        "macex": 14,
    }
    assert assigned["f10-stock-info"] == "f10"
    assert assigned["f10-limit-board-ladder"] == "f10"
    assert assigned["7709-minute-history"] == "history"
    assert assigned["7709-special-limits"] == "common"


def test_function_catalog_assigns_unknown_items_to_common_fallback() -> None:
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(
        encoding="utf-8"
    )

    assert "fallbackFunctionalGroups" in app
    assert 'taxonomyErrors.push("功能目录只能设置一个兜底分类")' in app


def test_pages_catalog_wraps_long_mobile_labels() -> None:
    styles = (REPO_ROOT / "docs" / "assets" / "interface-catalog.css").read_text(
        encoding="utf-8"
    )

    mobile = styles.split("@media screen and (max-width: 38rem)", 1)[1]
    assert ".catalog-heading h1" in mobile
    assert "overflow-wrap: anywhere" in mobile
    assert ".interface-stat span" in mobile
    assert "white-space: normal" in mobile


def test_function_view_uses_function_group_instead_of_internal_category() -> None:
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(
        encoding="utf-8"
    )

    assert 'currentView() === "function"' in app
    assert "functionalMeta[item.id].label" in app
    assert "item.category" in app
    assert "data-directory-detail" in app


def test_pages_catalog_covers_every_registered_7709_command() -> None:
    catalog = _catalog()
    assignments = _taxonomy_assignments(catalog)
    binary_ids = {
        item_id
        for item_id, (layer_id, _) in assignments.items()
        if layer_id == "7709"
        and next(item for item in catalog["items"] if item["id"] == item_id).get(
            "legacy_registry", True
        )
    }
    binary_protocols = [
        item["protocol"].lower()
        for item in catalog["items"]
        if item["id"] in binary_ids
    ]

    assert len(COMMANDS) == len(binary_ids) == 25
    assert Counter(binary_protocols) == Counter(
        command.hex for command in COMMANDS.values()
    )


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
        "7709-批量快照.md": ("client.quotes.get_snapshots", "QuoteSnapshot", "0x054c"),
        "7709-增量刷新推送队列.md": (
            "client.quotes.refresh",
            "QuoteRefreshPage",
            "0x0547",
        ),
        "7709-旧版批量行情.md": ("client.quotes.legacy", "LegacyQuote", "0x053e"),
    }
    for filename, (api, model, command) in expected_roles.items():
        text = (REPO_ROOT / "docs" / "methods" / filename).read_text(encoding="utf-8")
        assert api in text
        assert model in text
        assert f"../COMMANDS_7709.md#cmd-{command}" in text

    items = {item["id"]: item for item in _catalog()["items"]}
    assert items["7709-quote-snapshots"]["protocol"] == "0x054c"
    assert "旧版完整快照" in items["7709-legacy-quotes"]["summary"]
    assert "自动维护游标" in items["7709-quote-refresh"]["summary"]


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
    assert refresh["api"] == "client.quotes.subscribe(codes, callback)"
    assert refresh["doc"] == "methods/7709-增量刷新推送队列.md"

    snapshot_doc = (REPO_ROOT / "docs" / snapshot["doc"]).read_text(encoding="utf-8")
    complete_doc = (REPO_ROOT / "docs" / complete["doc"]).read_text(encoding="utf-8")
    assert "../helpers/完整行情.md" in snapshot_doc
    assert "7709-增量刷新推送队列.md" in snapshot_doc
    assert "主要调用 | `client.quotes.get_snapshots(codes)`" in snapshot_doc
    assert "from eltdx import TdxClient" in snapshot_doc
    assert "quotes = client.quotes.get_snapshots(" in snapshot_doc
    assert "client.helpers.full_quotes(codes)" in complete_doc


def test_file_resource_catalog_documents_download_and_stats_parsing() -> None:
    items = {item["id"]: item for item in _catalog()["items"]}
    resource = items["7709-file-content"]
    download = items["helper-server-download"]
    helper = items["helper-server-stats"]
    method_doc = (REPO_ROOT / "docs" / resource["doc"]).read_text(encoding="utf-8")

    assert resource["protocol"].lower() == "0x06b9"
    assert "read(" in resource["api"] and "download_file()" not in resource["api"]
    assert resource["return_model"] == "FileContentChunk"
    assert download["api"].startswith("client.resources.download_file(")
    assert download["return_model"] == "bytes"
    assert helper["source"] == "Helper"
    assert helper["api"].startswith("client.resources.read_stats(")
    assert "TdxStatsResource" in helper["return_model"]
    stats_doc = (REPO_ROOT / "docs" / helper["doc"]).read_text(encoding="utf-8")
    assert helper["doc_anchor"] == "stats-resource"
    assert "不是新的二进制命令" in stats_doc
    assert all(name in stats_doc for name in ("tdxstat.cfg", "tdxstat2.cfg"))
    assert all(
        name in method_doc for name in ("FileContentChunk", "content", "chunk_len")
    )


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
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(
        encoding="utf-8"
    )
    pyproject = (REPO_ROOT / "pyproject.toml").read_text(encoding="utf-8")
    gitignore = (REPO_ROOT / ".gitignore").read_text(encoding="utf-8")

    assert "fetch(" not in app
    assert "XMLHttpRequest" not in app
    assert "WebSocket" not in app
    assert "dependencies = []" in pyproject
    assert "site/" in gitignore.splitlines()


def test_pages_catalog_ui_exposes_taxonomy_navigation() -> None:
    page = (REPO_ROOT / "docs" / "index.md").read_text(encoding="utf-8")
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(
        encoding="utf-8"
    )
    styles = (REPO_ROOT / "docs" / "assets" / "interface-catalog.css").read_text(
        encoding="utf-8"
    )

    assert "data-interface-tree" in page
    assert "data-interface-scope-select" in page
    assert 'data-catalog-view="function"' in page
    assert 'data-catalog-view="interface"' in page
    assert "function/all" in app
    assert "catalogView" in app
    assert "window.location.hash" in app
    assert '"wrapper/helpers": "helpers"' in app
    assert '"7709/commands": "7709"' in app
    assert '"7615/features": "7615"' in app
    assert 'return "7709";' in app
    assert "catalog-tree-leaf" in app
    assert "按 7709、7615 F10、Helpers、扩展行情（7727）、MAC 和 MAC-EX 组织。" in app
    assert 'classList.toggle("interface-catalog-page"' in app
    assert ".interface-catalog-page .md-grid" in styles
    assert "\n.md-grid {\n" not in styles


def test_interface_details_promote_back_link_to_header() -> None:
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(
        encoding="utf-8"
    )
    styles = (REPO_ROOT / "docs" / "assets" / "interface-catalog.css").read_text(
        encoding="utf-8"
    )
    config = (REPO_ROOT / "mkdocs.yml").read_text(encoding="utf-8")

    assert app.index("promoteDetailBackLink();") < app.index("if (!root)")
    assert "header.insertBefore(link, logo)" in app
    assert "window.history.back()" in app
    assert "document.referrer && window.history.length > 1" in app
    assert 'link.setAttribute("aria-label", "返回上一页")' in app
    assert ".md-header .interface-header-back" in styles
    assert "primary: red" in config
    assert "primary: teal" not in config
    assert "#087f72" not in styles


def test_catalog_search_is_global_and_method_syntax_is_tokenized() -> None:
    app = (REPO_ROOT / "docs" / "assets" / "interface-catalog.js").read_text(
        encoding="utf-8"
    )
    config = (REPO_ROOT / "mkdocs.yml").read_text(encoding="utf-8")

    assert "var matchesScope = isSearching || rowMatchesScope(row, activeScope);" in app
    assert "updateNavigation(isSearching ? globalScopeId : activeScopeId);" in app
    assert 'heading.textContent = isSearching ? "搜索结果" : activeScope.label;' in app
    assert "separator:" in config
    assert "()\\[\\]" in config


def test_readme_promotes_the_static_pages_catalog() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    banner = README_BANNER_PATH.read_bytes()

    assert (
        '<a href="https://electkismet.github.io/eltdx/"><strong>接口一览</strong></a>'
        in readme
    )
    assert "<strong>在线文档</strong>" not in readme
    assert f'src="{README_BANNER_RELATIVE}"' in readme
    assert README_BANNER_PATH.is_file()
    assert banner.startswith(b"\x89PNG\r\n\x1a\n")
    assert (
        int.from_bytes(banner[16:20], "big"),
        int.from_bytes(banner[20:24], "big"),
    ) == (1250, 696)


def test_readme_shows_the_astlane_sponsor_banner() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    sponsor = SPONSOR_BANNER_PATH.read_text(encoding="utf-8")

    catalog_position = readme.index(f'src="{README_BANNER_RELATIVE}"')
    license_position = readme.index("## 许可证")
    sponsor_position = readme.index('src="docs/assets/astlane-sponsor.svg"')
    assert catalog_position < sponsor_position < license_position
    assert 'href="https://api.astlane.com/"' in readme
    assert '<title id="title">Astlane 赞助 eltdx token</title>' in sponsor


def test_current_docs_match_v2_pagination_and_parameter_contracts() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    code_count = (REPO_ROOT / "docs" / "methods" / "7709-代码数量.md").read_text(
        encoding="utf-8"
    )
    code_table = (REPO_ROOT / "docs" / "methods" / "7709-代码表.md").read_text(
        encoding="utf-8"
    )
    bars = (REPO_ROOT / "docs" / "methods" / "7709-K线周期线.md").read_text(
        encoding="utf-8"
    )
    finance = (REPO_ROOT / "docs" / "methods" / "7709-财务基础信息.md").read_text(
        encoding="utf-8"
    )

    assert "client.codes.count()" not in readme
    assert "不需要先查数量" in code_count
    assert "不需要先调用 `client.codes.count(market)`" in code_table
    assert "不限 A 股" in code_count
    assert "client.codes.a_share_count(market)" in code_count
    assert "`refresh`" not in code_count
    assert "`refresh`" not in code_table
    assert "不接受 `refresh` 参数" in finance
    assert "| 停止条件 | 主站返回空页 |" in code_table
    assert "主站返回空页才停止" in bars
    assert "短页不代表数据已经结束" in code_table
    assert "短页不代表结束" in bars


def test_current_docs_match_v2_cache_and_migration_contracts() -> None:
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    fields = (REPO_ROOT / "docs" / "FIELD_REFERENCE.md").read_text(encoding="utf-8")
    methods = (REPO_ROOT / "docs" / "METHOD_REFERENCE.md").read_text(encoding="utf-8")
    migration = (REPO_ROOT / "docs" / "FIELD_MIGRATION.md").read_text(encoding="utf-8")
    historical_update = (REPO_ROOT / "docs" / "UPDATE_FROM_0_5_1.md").read_text(
        encoding="utf-8"
    )

    for text in (readme, fields, methods):
        assert "代码数量、全量代码表、股本变迁、财务" not in text
    assert "`client.corporate.capital_changes()`" in fields
    assert 'client.bars.get("sz000001", period="day", include_raw=True)' in migration
    assert "client.corporate.adjustment_factors(" in migration
    assert "start_date=raw.bars[0].time.date()" in migration
    assert "client.corporate.capital_changes()` 股本变迁结果 | 否" in migration
    assert 'warning "历史版本文档"' in historical_update
    assert "当前 `v2.0.5` 已移除这些入口" in historical_update


def test_trade_docs_separate_raw_records_from_actual_trades() -> None:
    fields = (REPO_ROOT / "docs" / "FIELD_REFERENCE.md").read_text(encoding="utf-8")
    methods = (REPO_ROOT / "docs" / "METHOD_REFERENCE.md").read_text(encoding="utf-8")
    today = (REPO_ROOT / "docs" / "methods" / "7709-当日成交明细.md").read_text(
        encoding="utf-8"
    )
    history = (REPO_ROOT / "docs" / "methods" / "7709-历史成交明细.md").read_text(
        encoding="utf-8"
    )

    for text in (methods, today, history):
        assert "实际成交条数" not in text
        assert "混合记录条数" in text
    assert "`actual_trades`" in fields
    assert "不把它们解释为虚拟匹配量或未匹配量" in fields
    assert (
        "完整秒级过程、虚拟匹配量和未匹配量使用 `client.auctions.series()`" in methods
    )
    assert "15:05-15:30 的 `status=5` 是盘后固定价格真实成交" in methods
    assert 'client.trades.all_history("sz000001", "2026-05-20")' in methods


def test_trade_detail_docs_include_return_fields_and_examples() -> None:
    docs = {
        "当日撮合": REPO_ROOT / "docs" / "methods" / "7709-当日0925正式撮合.md",
        "历史撮合": REPO_ROOT / "docs" / "methods" / "7709-历史0925正式撮合.md",
    }
    for path in docs.values():
        text = path.read_text(encoding="utf-8")
        assert "## 返回示例" in text
        assert "## 返回字段" in text
        assert "示例结果" in text
        assert "TradeTick" in text

    for key in ("当日撮合", "历史撮合"):
        text = docs[key].read_text(encoding="utf-8")
        assert "event_kind" in text
        assert "opening_match" in text
        assert "is_opening_match" in text

    series = (REPO_ROOT / "docs" / "methods" / "7709-集合竞价明细.md").read_text(
        encoding="utf-8"
    )
    assert 'client.auctions.series("sz000001", "2026-06-12")' in series
    assert "不按固定时间范围过滤、截断或去重" in series


def test_local_factor_docs_explain_affine_coefficients() -> None:
    detail = (REPO_ROOT / "docs" / "methods" / "7709-本地复权系数.md").read_text(
        encoding="utf-8"
    )
    fields = (REPO_ROOT / "docs" / "FIELD_REFERENCE.md").read_text(encoding="utf-8")
    api = (REPO_ROOT / "docs" / "API_REFERENCE.md").read_text(encoding="utf-8")
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")

    assert "将这些系数按日期应用到本地保存的不复权 OHLC" in detail
    assert "client.bars.get()" in detail
    assert 'adjust="qfq"' in detail
    assert '"hfq"' in detail
    assert "根据 `0x000f` 返回的标签 `1` 权息事件" in detail
    assert "raw * scale + offset" in detail
    assert "bar_date < factor.date" in detail
    assert "factor.date <= bar_date" in detail
    assert "不舍入" in detail
    assert "`start_date=None` 表示使用全部有日期的权息事件" in detail
    assert "`0.01～0.02`" in detail
    assert "必须以第一根 K 线日期设置 `start_date`" in api
    assert "docs/methods/7709-本地复权系数.md" in readme
    assert "`anchor_date`" in fields
    assert "`start_date`" in fields


@pytest.mark.parametrize(
    ("source", "class_name", "module_path", "prefix"),
    [
        ("MAC", "MacClient", "mac/client.py", "mac"),
        ("EX", "ExTdxClient", "ex/client.py", "ex"),
        ("MAC-EX", "MacExClient", "ex/mac_client.py", "macex"),
    ],
)
def test_extended_catalog_covers_every_public_method(
    source: str, class_name: str, module_path: str, prefix: str
) -> None:
    tree = ast.parse(
        (REPO_ROOT / "src" / "eltdx" / module_path).read_text(encoding="utf-8")
    )
    client = next(
        node
        for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == class_name
    )
    lifecycle = {
        "from_best_host",
        "ping_all",
        "connect",
        "close",
        "disconnect",
        "ensure_connected",
        "refresh_hosts",
    }
    methods = {
        node.name: node
        for node in client.body
        if isinstance(node, ast.FunctionDef)
        and not node.name.startswith("_")
        and not any(
            isinstance(decorator, ast.Name) and decorator.id == "property"
            for decorator in node.decorator_list
        )
    }
    items = [item for item in _catalog()["items"] if item["source"] == source]
    business = {
        item["api"].split(".", 1)[1].split("(", 1)[0]: item
        for item in items
        if item["api"].startswith(f"{class_name}.")
    }
    assert set(business) == set(methods) - lifecycle
    assert len({item["doc"] for item in items}) == len(items)
    for name, item in business.items():
        assert item["doc"] == f"extended/{prefix}-{name}.md"
        detail = (REPO_ROOT / "docs" / item["doc"]).read_text(encoding="utf-8")
        for heading in (
            "调用签名",
            "参数",
            "使用示例",
            "分页、单位与限制",
            "返回字段",
        ):
            assert f"## {heading}" in detail, item["doc"]
        args = re.sub(r"^self,?\s*|^cls,?\s*", "", ast.unparse(methods[name].args))
        returns = ast.unparse(methods[name].returns)
        assert f"{class_name}.{name}({args}) -> {returns}" in detail
        for arg in methods[name].args.args[1:] + methods[name].args.kwonlyargs:
            assert f"| `{arg.arg}` |" in detail, (name, arg.arg)
    connection = next(item for item in items if item["id"] == f"{prefix}-connection")
    connection_doc = (REPO_ROOT / "docs" / connection["doc"]).read_text(
        encoding="utf-8"
    )
    for name in set(methods) & lifecycle:
        assert f"{class_name}.{name}(" in connection_doc
    for node in client.body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Name):
            if node.value.id in business:
                detail = (
                    REPO_ROOT / "docs" / business[node.value.id]["doc"]
                ).read_text(encoding="utf-8")
                for target in node.targets:
                    if isinstance(target, ast.Name) and not target.id.startswith("_"):
                        assert f"{class_name}.{target.id}(" in detail


def test_extended_python_examples_compile_and_bind_without_network() -> None:
    from eltdx.ex import ExTdxClient, MacExClient
    from eltdx.mac import MacClient

    clients = {cls.__name__: cls for cls in (MacClient, ExTdxClient, MacExClient)}
    for path in (REPO_ROOT / "docs" / "extended").glob("*.md"):
        detail = path.read_text(encoding="utf-8")
        for code in re.findall(r"```python\n(.*?)\n```", detail, re.DOTALL):
            tree = ast.parse(code, filename=str(path))
            compile(tree, str(path), "exec")
            namespace: dict = {}
            for node in tree.body:
                if isinstance(node, (ast.Import, ast.ImportFrom)):
                    exec(
                        compile(
                            ast.Module(body=[node], type_ignores=[]), str(path), "exec"
                        ),
                        namespace,
                    )
            for block in (node for node in tree.body if isinstance(node, ast.With)):
                context = block.items[0].context_expr
                assert isinstance(context, ast.Call) and isinstance(
                    context.func, ast.Name
                )
                client = clients[context.func.id]
                for call in ast.walk(block):
                    if not isinstance(call, ast.Call) or not isinstance(
                        call.func, ast.Attribute
                    ):
                        continue
                    if (
                        not isinstance(call.func.value, ast.Name)
                        or call.func.value.id != "client"
                    ):
                        continue
                    args = [
                        eval(compile(ast.Expression(arg), str(path), "eval"), namespace)
                        for arg in call.args
                    ]
                    kwargs = {
                        keyword.arg: eval(
                            compile(ast.Expression(keyword.value), str(path), "eval"),
                            namespace,
                        )
                        for keyword in call.keywords
                    }
                    inspect.signature(getattr(client, call.func.attr)).bind(
                        object(), *args, **kwargs
                    )


def test_extended_reference_lists_all_model_fields_and_enum_members() -> None:
    from eltdx.mac import FieldBit, PresetField
    from eltdx.mac import enums

    model_doc = (REPO_ROOT / "docs" / "extended" / "返回模型.md").read_text(
        encoding="utf-8"
    )
    for relative in ("mac/models.py", "ex/models.py"):
        tree = ast.parse(
            (REPO_ROOT / "src" / "eltdx" / relative).read_text(encoding="utf-8")
        )
        for model in (node for node in tree.body if isinstance(node, ast.ClassDef)):
            section = model_doc.split(f"## {model.name}\n", 1)[1].split("\n## ", 1)[0]
            for field in (
                node for node in model.body if isinstance(node, ast.AnnAssign)
            ):
                assert f"| `{field.target.id}` |" in section, (
                    model.name,
                    field.target.id,
                )
    enum_doc = (REPO_ROOT / "docs" / "extended" / "枚举与字段位图.md").read_text(
        encoding="utf-8"
    )
    for enum_name in (
        "Period",
        "Adjust",
        "BoardType",
        "Category",
        "SortType",
        "SortOrder",
        "FilterType",
        "ExMarket",
        "ExBoardType",
    ):
        enum = getattr(enums, enum_name)
        section = enum_doc.split(f"## {enum_name}\n", 1)[1].split("\n## ", 1)[0]
        for name in enum.__members__:
            assert f"`{name}`" in section, (enum_name, name)
    for name in FieldBit.__members__:
        assert f"`{name}`" in enum_doc, name
    for name in PresetField.__members__:
        assert f"| {name} |" in enum_doc, name


def test_transaction_docs_distinguish_server_seconds_and_aggregation() -> None:
    docs = REPO_ROOT / "docs"
    mac = (docs / "extended" / "mac-get_transactions.md").read_text(encoding="utf-8")
    assert "0x122F" in mac
    assert "uint32 当日秒数" in mac
    assert "聚合成交明细" in mac
    assert "不等于 Level-2" in mac
    assert "2026-10-30" in mac and "13:51:09" in mac
    for name in ("7709-当日成交明细.md", "7709-历史成交明细.md"):
        text = (docs / "methods" / name).read_text(encoding="utf-8")
        assert "../extended/mac-get_transactions.md" in text
