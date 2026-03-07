# 更新日志

这里记录 `eltdx` 的公开版本变化。

## Unreleased

### 修改

- 修正 `get_quote()` 中快照价格字段的语义映射：`last_price` 对应最新价，`last_close_price` 对应昨收 / 前收价
- 将 `Quote.close_price` / `Quote.close_price_milli` 重命名为 `Quote.last_close_price` / `Quote.last_close_price_milli`
- 同步更新快照相关代码、测试、示例和字段说明，统一快照命名口径

## 0.1.4 - 2026-03-07

主要修正 PyPI 展示和发布元数据。

### 修改

- 修复 `pyproject.toml` 中的包摘要文字，避免 PyPI 顶部简介显示异常
- 补充 `docs/README.md` 与 `scripts/README.md` 的双向导航，方便用户在文档与脚本之间切换
- 更新 README 中的 PyPI 版本徽章缓存参数，方便新版发布后更快刷新

## 0.1.3 - 2026-03-07

修正文档导航与 PyPI 展示细节。

### 修改

- 调整 README 首页文案，优化 PyPI 页面阅读体验
- 统一仓库首页、GitHub About 和 PyPI 展示用链接
- 整理 `scripts/` 目录与说明文档，便于查找 smoke 和 validation 脚本

## 0.1.2 - 2026-03-07

继续完善发布资料与文档说明。

### 修改

- 修正 PyPI 项目页显示用 README 内容
- README 中明确 Python 版本要求为 `Python 3.10+`
- 补充 `docs/API_REFERENCE.md` 的字段说明和使用示例

## 0.1.1 - 2026-03-07

修复 Python 3.10 环境兼容问题。

### 修改

- 处理 Python 3.10 下缺少 `math.exp2()` 的兼容逻辑
- 调整 CI 与构建验证，覆盖 Python 3.10
- 确保 wheel 可在 Python 3.10 环境正常安装导入

## 0.1.0 - 2026-03-07

首个公开版本，提供统一的行情客户端接口。

### 主要能力

- 提供 `TdxClient` 统一入口
- 默认使用 `pool_size=2` 的连接池
- `get_quote()` 内置自动分批，默认 `batch_size=80`
- 支持 `with TdxClient() as client:` 上下文管理
- 返回模型统一为 dataclass，时间字段转换为 Python 原生 `datetime` / `date`
- 支持 `include_raw=True` 查看原始十六进制数据

### 首批接口

- `get_call_auction()`
- `get_quote()`
- `get_count()`
- `get_codes()`
- `get_codes_all()`
- `get_stock_codes_all()`
- `get_etf_codes_all()`
- `get_index_codes_all()`
- `get_minute()`
- `get_trades()`
- `get_trades_all()`
- `get_kline()`
- `get_kline_all()`

### 兼容说明

- 提供 `get_history_minute()`、`get_trade()`、`get_trade_all()`、`get_history_trade()`、`get_history_trade_day()` 等兼容别名
- 兼容旧的 API 调用顺序，例如 `get_kline(freq, code)` 与 `get_kline(code, freq)`

### 测试

- 提供单元测试与可选联网集成测试
- 联网测试可通过 `ELTDX_RUN_LIVE=1` 开启
