---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# 最新 ST 列表

按当前证券名称筛选 ST 类股票。

| 项目 | 内容 |
| --- | --- |
| 调用 | `client.helpers.latest_st(market=None)` |
| 返回 | `list[SecurityCode]` |
| 数据来源 | `0x044d` 代码表和证券名称规则 |

## 输入参数

| 参数 | 是否必填 | 说明 |
| --- | --- | --- |
| `market` | 否 | 可选 `sh`、`sz`、`bj`；传 `None` 扫描全部市场 |

## 筛选规则

名称包含 `ST`、`*ST`、`SST` 或 `S*ST` 的证券会被返回。`market` 可选 `sh`、`sz` 或 `bj`；不传时扫描全部 A 股代码表。该列表反映当前名称，不提供历史 ST 变更记录。

## 返回字段

返回 `list[SecurityCode]`。每条记录包含 `exchange`、`market_id`、`code`、`full_code`、`name`、`multiple`、`decimal`、`previous_close_price`、`volume_ratio_base`、`category`、`category_reason`、`board` 和 `board_reason`。

## 示例

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    rows = client.helpers.latest_st()
    print([(row.full_code, row.name) for row in rows[:10]])
```

## 真实返回样本

??? return-sample "真实返回 JSON · SecurityCode（当前结果节选）"
    <div class="return-sample-meta">
      <div><span>筛选范围</span><code>沪深北 A 股</code></div>
      <div><span>返回类型</span><code>list[SecurityCode]</code></div>
    </div>
    <p class="return-sample-note">真实采样；本次代码表没有筛出 ST 证券，结果为空列表。</p>

    ```json
    []
    ```
