# 脚本说明

这里放的是仓库附带的辅助脚本，按用途分成两类。

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
```

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
