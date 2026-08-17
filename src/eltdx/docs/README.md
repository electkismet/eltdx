# eltdx 文档入口

这里放的是 `eltdx` 的产品和工程文档，默认面向使用者和后续开发者阅读。

## 推荐阅读顺序

| 顺序 | 文档 | 用途 |
| --- | --- | --- |
| 1 | [PRODUCT.md](PRODUCT.md) | 看这个库能查什么、适合怎么用 |
| 2 | [releases/v3.0.2.md](releases/v3.0.2.md) | 看成交明细真实成交视图和返回样本文档 |
| 3 | [releases/v3.0.1.md](releases/v3.0.1.md) | 看历史集合竞价过程、聚合口径和升级说明 |
| 4 | [releases/v3.0.0.md](releases/v3.0.0.md) | 看 3.0 Rust 重写、平台 wheel、兼容和迁移边界 |
| 5 | [MIGRATION_FROM_OLD.md](MIGRATION_FROM_OLD.md) | 从 1.x 迁移到当前模块化 API |
| 6 | [helpers/README.md](helpers/README.md) | 按常用问题进入调用说明 |
| 7 | [METHOD_REFERENCE.md](METHOD_REFERENCE.md) | 按调用方法看参数、底层接口和解析字段 |
| 8 | [methods/README.md](methods/README.md) | 按单个调用方法看独立说明页 |
| 9 | [API_REFERENCE.md](API_REFERENCE.md) | 看 `TdxClient` 应该怎么调用 |
| 10 | [EXAMPLES.md](EXAMPLES.md) | 直接复制常见调用示例 |
| 11 | [FIELD_REFERENCE.md](FIELD_REFERENCE.md) | 看返回模型字段总表 |
| 12 | [F10_7615.md](F10_7615.md) | 看 F10 / 资料 / 题材 / 公告怎么查 |
| 13 | [MCP.md](MCP.md) | 看 MCP 工具怎么启动、有哪些工具 |
| 14 | [DEBUG_GUIDE.md](DEBUG_GUIDE.md) | 连接失败、主站慢、字段排查 |
| 15 | [COMMANDS_7709.md](COMMANDS_7709.md) | 看每个业务 API 对应哪个 `7709` 命令 |
| 16 | [ARCHITECTURE.md](ARCHITECTURE.md) | 看项目分层和实现结构 |
| 17 | [FIELD_MIGRATION.md](FIELD_MIGRATION.md) | 看历史字段和当前字段怎么对应 |
| 18 | [UPDATE_FROM_0_5_1.md](UPDATE_FROM_0_5_1.md) | 历史归档：从 `v0.5.1` 到 `v1.0.0` 的更新说明 |
| 19 | [ROADMAP.md](ROADMAP.md) | 历史归档：1.0 实现记录 |

## 文档说明

`docs/` 目录说明 Python API 怎么用，以及 3.0 Rust native backend 怎么安装、排错、开发和发布。根 `docs/` 是唯一人工编辑源；构建前会生成 byte-identical 的包内 `eltdx/docs` 镜像。

普通受支持平台通过 `cp310-abi3` wheel 安装，不需要 Rust；没有匹配 wheel 时从 sdist 构建需要 Rust 1.89。3.0 不提供纯 Python 7709 fallback，详见 [架构](ARCHITECTURE.md) 和 [排错指南](DEBUG_GUIDE.md)。

底层协议字段、payload 结构、抓包样本和字段中文对照，以仓库内协议文档为准。

`7615` 的 F10 / HTTP 接口已经作为 `eltdx.f10` 接入；使用者可以从 `TdxClient.f10` 或 `F10Client` 调用。

MCP 工具服务通过 `eltdx-mcp` 启动，具体工具列表见 [MCP.md](MCP.md)。

常用问题入口见 [helpers/README.md](helpers/README.md)。
