# Documentation

这是 `eltdx` 的文档首页。

如果你刚接触项目，建议不要从一堆文件里自己找入口，直接按下面的阅读顺序往下看。

## 推荐阅读顺序

### 第一步：先看项目首页

- [`../README.md`](../README.md)
  - 了解项目定位
  - 看安装方式
  - 看 30 秒上手示例
  - 看整体 API 总览与 FAQ

### 第二步：看完整 API 用法

- [`API_REFERENCE.md`](./API_REFERENCE.md)
  - `TdxClient` 如何初始化
  - `connect()` / `close()` / `with` 如何使用
  - 每个公开方法的参数、返回值、调用方式
  - 服务层对象怎么用

### 第三步：看可直接复制的示例

- [`EXAMPLES.md`](./EXAMPLES.md)
  - 一次性抓取怎么写
  - 长连接场景怎么写
  - 快照 / 分时 / 逐笔 / K 线 / 集合竞价怎么写
  - `include_raw=True` 调试怎么写

### 第四步：看字段到底是什么意思

- [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
  - dataclass 字段总表
  - 每个字段的中文含义
  - 原始值 -> 解析值的对照示例
  - 时间字段、价格字段、`*_milli` 字段怎么理解

### 第五步：看字段口径是否已经坐实

- [`FIELD_SEMANTICS_LOCKDOWN.md`](./FIELD_SEMANTICS_LOCKDOWN.md)
  - 哪些字段已经可以放心写进正式说明
  - 哪些字段仍应保持保守口径
  - 字段语义目前锁定到什么程度

## 按需求找文档

### 我想知道“某个函数怎么调用”

看：[`API_REFERENCE.md`](./API_REFERENCE.md)

### 我想直接复制一段能跑的代码

看：[`EXAMPLES.md`](./EXAMPLES.md)

### 我想知道“某个字段到底是什么意思”

看：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)

### 我想确认“这个字段解释靠不靠谱”

看：[`FIELD_SEMANTICS_LOCKDOWN.md`](./FIELD_SEMANTICS_LOCKDOWN.md)

### 我想调试 raw / hex / smoke

看：[`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)

### 我想看当前哪些 API 已经联网跑通

看：[`API_VALIDATION_SUMMARY.md`](./API_VALIDATION_SUMMARY.md)

### 我想看最终归纳和真实样例对照

看：[`API_FINAL_VALIDATION_REPORT.md`](./API_FINAL_VALIDATION_REPORT.md)

### 我想看项目内部怎么分层

看：[`ARCHITECTURE.md`](./ARCHITECTURE.md)

### 我想检查发布前还差什么

看：[`FIRST_RELEASE_CHECKLIST.md`](./FIRST_RELEASE_CHECKLIST.md)

### 我想把仓库传到 GitHub，并让别人能直接 `pip install eltdx`

看：[`PUBLISHING.md`](./PUBLISHING.md)

## 文档地图

| 文档 | 作用 | 适合谁看 |
| --- | --- | --- |
| [`../README.md`](../README.md) | 项目首页 | 第一次打开仓库的人 |
| [`API_REFERENCE.md`](./API_REFERENCE.md) | 完整 API 说明 | 想系统看全部方法的人 |
| [`EXAMPLES.md`](./EXAMPLES.md) | 可运行示例集合 | 想直接复制代码的人 |
| [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md) | 字段中文解释 | 想理解字段含义的人 |
| [`FIELD_SEMANTICS_LOCKDOWN.md`](./FIELD_SEMANTICS_LOCKDOWN.md) | 字段语义锁定状态 | 想确认字段口径是否稳的人 |
| [`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md) | 调试 / smoke / raw 对拍 | 想排查问题的人 |
| [`API_VALIDATION_SUMMARY.md`](./API_VALIDATION_SUMMARY.md) | 在线验证归纳 | 想看整体通过情况的人 |
| [`API_FINAL_VALIDATION_REPORT.md`](./API_FINAL_VALIDATION_REPORT.md) | 最终归纳 + 样例对照 | 想人工核对真实数据的人 |
| [`PROTOCOL_COMPARISON_CHECKLIST.md`](./PROTOCOL_COMPARISON_CHECKLIST.md) | 协议实现核对清单 | 想看实现边界的人 |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | 架构与分层 | 想看内部结构的人 |
| [`FIRST_RELEASE_CHECKLIST.md`](./FIRST_RELEASE_CHECKLIST.md) | 首发检查清单 | 准备打包发布的人 |
| [`PUBLISHING.md`](./PUBLISHING.md) | GitHub / PyPI 发布指南 | 准备正式公开发布的人 |
| [`API_VALIDATION_REVIEW.md`](./API_VALIDATION_REVIEW.md) | 语义审查记录 | 想看历史问题与修正点的人 |

## 调试与验证入口

### 调试 raw / hex

- [`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)
- `include_raw=True`
- `raw_frame_hex`
- `raw_payload_hex`

### 跑 smoke

常用命令：

```bash
python scripts/smoke_codes.py
python scripts/smoke_minute.py
python scripts/smoke_trade.py
python scripts/smoke_kline.py
python scripts/smoke_call_auction.py
python scripts/smoke_live_all.py
python scripts/export_live_validation.py
```

### 看真实导出样本

- `../artifacts/`
- 最新一套联网验证样本在对应时间戳目录下

## 当前文档分工

- `README.md`：让人快速理解项目与用法
- `API_REFERENCE.md`：把方法签名、参数、返回模型讲清楚
- `EXAMPLES.md`：给出可直接复制的代码
- `FIELD_REFERENCE.md`：把字段名翻译成能读懂的中文指标
- `FIELD_SEMANTICS_LOCKDOWN.md`：告诉你哪些字段解释已经足够稳
- 验证相关文档：告诉你当前版本哪些能力已经真正联网跑通过

## 如果你只看 3 份文档

建议优先看这 3 份：

1. [`../README.md`](../README.md)
2. [`API_REFERENCE.md`](./API_REFERENCE.md)
3. [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
