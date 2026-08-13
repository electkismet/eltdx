---
hide:
  - navigation
---

[← 返回接口目录](../index.md){ .interface-detail-back }

# F10-高级调用（需手动指定 Entry 和参数）

## 作用

由用户手动指定 Entry 和参数，直接调用任意 `7615/TQLEX` Entry。它不会由 SDK 自动执行，适合调试、临时验证新 Entry，或调用 SDK 还没有单独封装的方法。普通业务查询优先使用 `client.f10.company_profile()`、`client.f10.finance_report()` 等具名方法。

| 项目 | 内容 |
| --- | --- |
| 请求体调用 | `client.f10.call(entry, body=...)` |
| 参数调用 | `client.f10.params(entry, ...)` |
| 底层入口 | [`7615/TQLEX`](../F10_7615.md#tqlex-gateway) |
| 返回模型 | `F10Response` |

## 示例

```python
from eltdx import F10Client

f10 = F10Client(timeout=3)

profile = f10.call("CWServ.tdxf10_gg_gsgk", params=["8", "000034", ""])
theme = f10.call("HQServ.hq_nlp_tcihq", [{
    "ReqId": "200743",
    "setcode": 0,
    "code": "000034",
    "Page": -1,
    "PageSize": "-1",
    "modname": "mod_tcihq.dll",
}])

print(profile.rows[:2])
print(theme.rows[:2])
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `entry` | TQLEX Entry 名称 |
| `params` | CWServ 常用参数数组，会包装成 `{"Params": [...]}` |
| `body` | 完整 JSON 请求体，常用于 `HQServ.*` 或 `CWSearch.*` |

## 请求对照

```python
f10.params("CWServ.tdxf10_gg_gsgk", "8", "000034", "")
# 等价于
f10.call("CWServ.tdxf10_gg_gsgk", params=["8", "000034", ""])
```

实际请求体：

```python
{"Params": ["8", "000034", ""]}
```

## 返回示例

```python
{
    "entry": "CWServ.tdxf10_gg_gsgk",
    "request_body": {"Params": ["8", "000034", ""]},
    "ok": True,
    "rows": [{"上市日期": "1991-04-03", "发行价": "40.00"}],
}
```

字段名来自服务端返回的 `ColName` / `ColDes`，不同 Entry 可能返回中文字段、拼音字段或 `T001` 这类原生字段。
