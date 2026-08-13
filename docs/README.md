# eltdx 文档入口

这里放的是 `eltdx` 的产品和工程文档，默认面向使用者和后续开发者阅读。

## 推荐阅读顺序

| 顺序 | 文档 | 用途 |
| --- | --- | --- |
| 1 | [PRODUCT.md](PRODUCT.md) | 看这个库能查什么、适合怎么用 |
| 2 | [releases/v2.0.0.md](releases/v2.0.0.md) | 看当前版本的模块化 API、旧兼容入口清理与实时五档推送说明 |
| 3 | [UPDATE_FROM_0_5_1.md](UPDATE_FROM_0_5_1.md) | 看从 `v0.5.1` 到 `v1.0.0` 更新了什么 |
| 4 | [helpers/README.md](helpers/README.md) | 按常用问题进入调用说明 |
| 5 | [METHOD_REFERENCE.md](METHOD_REFERENCE.md) | 按调用方法看参数、底层接口和解析字段 |
| 6 | [methods/README.md](methods/README.md) | 按单个调用方法看独立说明页 |
| 7 | [API_REFERENCE.md](API_REFERENCE.md) | 看 `TdxClient` 应该怎么调用 |
| 8 | [EXAMPLES.md](EXAMPLES.md) | 直接复制常见调用示例 |
| 9 | [FIELD_REFERENCE.md](FIELD_REFERENCE.md) | 看返回模型字段总表 |
| 10 | [F10_7615.md](F10_7615.md) | 看 F10 / 资料 / 题材 / 公告怎么查 |
| 11 | [MCP.md](MCP.md) | 看 MCP 工具怎么启动、有哪些工具 |
| 12 | [DEBUG_GUIDE.md](DEBUG_GUIDE.md) | 连接失败、主站慢、字段排查 |
| 13 | [COMMANDS_7709.md](COMMANDS_7709.md) | 看每个业务 API 对应哪个 `7709` 命令 |
| 14 | [ARCHITECTURE.md](ARCHITECTURE.md) | 看项目分层和实现结构 |
| 15 | [FIELD_MIGRATION.md](FIELD_MIGRATION.md) | 看历史字段和当前字段怎么对应 |
| 16 | [MIGRATION_FROM_OLD.md](MIGRATION_FROM_OLD.md) | 看历史代码整理记录 |
| 17 | [ROADMAP.md](ROADMAP.md) | 看 1.0 的实现顺序 |

## 文档说明

`docs/` 目录说明 Python 项目怎么用、怎么开发、怎么发布。

底层协议字段、payload 结构、抓包样本和字段中文对照，以仓库内协议文档为准。

`7615` 的 F10 / HTTP 接口已经作为 `eltdx.f10` 接入；使用者可以从 `TdxClient.f10` 或 `F10Client` 调用。

MCP 工具服务通过 `eltdx-mcp` 启动，具体工具列表见 [MCP.md](MCP.md)。

常用问题入口见 [helpers/README.md](helpers/README.md)。
