# 脚本说明

这里放的是仓库附带的辅助脚本，按用途分成两类。

如果你想先看库本身怎么安装、怎么调用 API、怎么查字段，可以先看：[`../docs/README.md`](../docs/README.md)

## smoke

- 目录：`scripts/smoke/`
- 作用：快速验证单个接口或整条联网链路是否正常
- 适合：临时自测、环境联通检查、升级后快速回归

常用命令：

```bash
python scripts/smoke/smoke_codes.py
python scripts/smoke/smoke_minute.py
python scripts/smoke/smoke_trade.py
python scripts/smoke/smoke_kline.py
python scripts/smoke/smoke_call_auction.py
python scripts/smoke/smoke_live_all.py
python scripts/smoke/export_auction_925_daily.py --start 2026-04-01 --end 2026-04-09 --export-dir output/auction_0925
```

补充：

- `export_auction_925_daily.py` 会按交易日输出一个 CSV
- 默认列是 `date`、`code`、`has_auction_0925`、`open_price`、`auction_volume_hand`、`auction_amount_yuan`
- 底层走 `get_auction_0925()`，适合做历史 `09:25` 批量导出

## validation

- 目录：`scripts/validation/`
- 作用：导出联网验证样本、整理核对结果、生成汇总 CSV
- 适合：协议对拍、人工核验、版本验收

常用命令：

```bash
python scripts/validation/export_live_validation.py
python scripts/validation/export_validation_csv.py
python scripts/validation/export_executive_summary_csv.py
```
