# Maintainers

这组文档主要面向仓库维护、联调验证、发布与后续贡献，不是日常使用 `eltdx` 的必读材料。

如果你只是想安装和调用接口，优先看：

- [`../../README.md`](../../README.md)
- [`../API_REFERENCE.md`](../API_REFERENCE.md)
- [`../EXAMPLES.md`](../EXAMPLES.md)
- [`../FIELD_REFERENCE.md`](../FIELD_REFERENCE.md)
- [`../DEBUG_GUIDE.md`](../DEBUG_GUIDE.md)

## 维护者资料目录

| 文档 | 作用 |
| --- | --- |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | 记录项目分层、模块职责与内部组织 |
| [`API_VALIDATION_SUMMARY.md`](./API_VALIDATION_SUMMARY.md) | 汇总最近一次联网验证结果 |
| [`API_FINAL_VALIDATION_REPORT.md`](./API_FINAL_VALIDATION_REPORT.md) | 按 API 归纳真实样例与返回字段 |
| [`API_VALIDATION_REVIEW.md`](./API_VALIDATION_REVIEW.md) | 保留历史问题、修正点与语义审查记录 |
| [`FIELD_SEMANTICS_LOCKDOWN.md`](./FIELD_SEMANTICS_LOCKDOWN.md) | 记录字段解释的实现依据、实测样本与业务口径 |
| [`PROTOCOL_COMPARISON_CHECKLIST.md`](./PROTOCOL_COMPARISON_CHECKLIST.md) | 记录协议覆盖范围、边界与后续关注点 |
| [`FIRST_RELEASE_CHECKLIST.md`](./FIRST_RELEASE_CHECKLIST.md) | 首发版本的发布前核对清单 |
| [`PUBLISHING.md`](./PUBLISHING.md) | GitHub / PyPI 发布步骤与自动化说明 |

## 适合什么时候看

- 要整理或扩展协议实现时
- 要核对某个字段的实现依据时
- 要回看最近一轮联网验证时
- 要做 GitHub / PyPI 发布时
