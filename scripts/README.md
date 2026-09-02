# 脚本说明

这里放源码仓库自带的辅助脚本。普通用户直接调用 `eltdx` API 或安装后的命令行入口即可；这些脚本主要用于开发、发布前自测和排查真实主站问题。

安装后可直接使用的命令：

```bash
eltdx-smoke --help
eltdx-f10-smoke --help
```

MCP 工具服务启动后会占用当前终端作为 stdio 服务：

```bash
eltdx-mcp
```

下面的 `python scripts/...` 写法适用于源码仓库目录。

## smoke

目录：`scripts/smoke/`

用途：快速确认当前环境的真实 `7709` 主站连接状态，以及核心 API 返回情况。

查看帮助：

```bash
eltdx-smoke --help
python scripts/smoke/live_smoke.py --help
```

轻量检查：

```bash
python scripts/smoke/live_smoke.py
python scripts/smoke/smoke_codes.py
python scripts/smoke/smoke_kline.py
python scripts/smoke/smoke_minute.py
python scripts/smoke/smoke_trade.py
python scripts/smoke/smoke_workday.py
```

F10 / 资料网关检查：

```bash
eltdx-f10-smoke --code 000034 --timeout 8
python scripts/smoke/smoke_f10.py --code 000034 --timeout 8
```

指定代码、主站和日期：

```bash
python scripts/smoke/live_smoke.py --code sz000001 --history-date 2026-05-20 --host 116.205.183.150:7709
```

发布前完整一点的检查：

```bash
python scripts/smoke/live_smoke.py --deep --probe-hosts --pool-size 2
```

`--deep` 会多跑全量 K 线、全量成交明细、全市场代码表、股本/复权因子等，耗时会比默认检查长。

批量导出每日 09:25 竞价成交快照：

```bash
python scripts/smoke/export_auction_925_daily.py --code sz000001 --start 2026-04-01 --end 2026-04-30
```

输出 CSV 默认写到 `output/auction_0925/`，可以用 `--output` 指定完整文件路径。
