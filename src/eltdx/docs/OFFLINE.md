# 本地行情文件读写 `eltdx.offline`

`eltdx.offline` 直接读写**通达信客户端安装目录**下的行情文件，纯标准库实现，不访问网络。适合把程序取到的数据落盘成通达信可读的文件，或直接分析本地已有的历史数据。

## 入口

| 能力 | 入口 |
| --- | --- |
| 日线 `.day` 读 | `offline.read_daily_bar(...)` |
| 日线 `.day` 写 / 追加 | `offline.write_daily_bar(...)` / `offline.append_daily_bar(...)` |
| 分钟线 `.lc1` / `.lc5` / `.5` 读 | `offline.read_min_bar(...)` / `offline.read_5min_bar(...)` |
| 分钟线写 / 追加 | `offline.write_min_bar(...)` / `offline.write_5min_bar(...)` / `append_*` |
| 扩展市场日线读 / 写 | `offline.read_ex_daily_bar(...)` / `offline.write_ex_daily_bar(...)` |
| 板块文件 | `offline.read_block(...)` / `read_customer_block(...)` |
| 股本变迁 GBBQ | `offline.read_gbbq(...)` |
| 历史财务文件 | `offline.read_history_financial(...)` |
| 行业分类 cfg | `offline.read_industry_cfg(...)` / `parse_tdxhy_cfg(...)` |
| 路径推断 | `offline.detect_tdx_home(...)` / `resolve_vipdoc(...)` / `market_to_exchange(...)` |

## 示例

```python
from eltdx import offline

# 读本地日线
bars = offline.read_daily_bar("sz000001")

# 写回（覆盖 / 追加由不同函数区分）
offline.write_daily_bar("sz000001", bars)

# 分钟线
mins = offline.read_min_bar("sz000001")
offline.write_min_bar("sz000001", mins)

# 扩展市场日线
exbars = offline.read_ex_daily_bar(...)
offline.write_ex_daily_bar(..., exbars)
```

## 模型

读取结果使用 `eltdx.offline` 下的 dataclass：`DailyBar`、`MinBar`、`ExDailyBar`、`BlockInfo`、`CustomerBlock`、`GbbqRecord`、`FinancialFileInfo`、`FinancialRecord`。

### 截断文件：能读多少读多少

`.day` / `.lc1` / `.lc5` / `.5` / 扩展市场 `.day` 的读取**只遍历完整记录**，末尾不足一条记录的残尾被忽略；文件不足一条记录时返回空列表。通达信在写盘或下载中断时经常留下「整数条 + 残尾」的文件，旧实现会因此抛 `ResourceFormatError` 而让整份历史读不出来，现已与 `easy_tdx` 行为对齐。

### 港股 `ds/*.day` 的成交额（`hk_stock_amount`）

扩展市场日线的第 6 个字段（成交额槽位）**口径随品种而变**，`ExDailyBar` 同时保留两种视图：

| 品种 | 该槽位含义 | 应使用 |
| --- | --- | --- |
| 期货 / 外盘 | 无符号整数成交额 | `amount` |
| 港股 | `float32` 成交额 | `hk_stock_amount` |

同一 4 字节 `0x4996B43C`：按整数读是 `1234613308`（IEEE-754 位模式），按 `float32` 读才是真实成交额 `1234567.5`。写回时使用 `amount`（原样字节），因此「读出来再写回去」不会丢字节。

## 说明

- 只依赖标准库与 `eltdx.exceptions`，不涉及网络请求。
- 除权标志、金额、成交量等字段按通达信文件的小端序布局解析。
- 路径不存在时抛 `FileNotFoundError`；板块文件头 / 记录长度非法、历史财务 zip 无效等**结构性**错误仍抛 `eltdx.exceptions.ResourceFormatError`（行情文件的末尾残尾不属于此类）。
