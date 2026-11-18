# Rust SDK 候选能力说明

本页描述候选能力和已知限制。当前 Python 与 Rust 统一测试版见 [v3.4.0-beta.1 发布说明](../releases/v3.4.0-beta.1.md)。历史验证记录按候选提交保存在 [统一验证](unified-validation.md)。

## 新增独立 Rust 产品

- 新增公开 `eltdx` Rust crate，复用 protocol/runtime，共同承载业务实现；普通Rust消费不依赖Python、PyO3、maturin或后台Python服务。
- 默认提供同步HQ服务和HQ helper；offline、F10、EX、MAC/MAC-EX及serde按feature启用，mac包含ex。没有伪装为异步的async功能。
- 惰性build、共享owner、显式关闭、可取消操作、整次期限/尝试/页/记录/字节预算、客户端metadata隔离和有来源的raw/单位/日期模型。
- HQ覆盖目录、报价/深度、K线/分钟/逐笔、竞价/日资金流、资本财务、资源/统计/公司/日历和组合helper；F10具名端点、动态表和历史资金流；扩展报价/位图/目录/板块/复权/下载；本地行情、股本、行业及专业财报DAT/ZIP。

具体签名和行为以[用户指南](user-guide.md)、[公共API](public-api.md)与分域规格为准。上述范围表示候选实现源码的目标覆盖，不表示每个接口已有通过结果。

## Python 兼容工作

标准Python入口按域接入同一Rust业务源码，公开Python类/属性/参数/返回模型和必要对象适配继续保留。原逐请求timeout、缓存identity、自定义transport/hook、异常context/cause、关闭后重开和文件append行为分别有合同。严格Rust规则不会自动改变Python默认。

普通 Python 用户继续使用匹配 wheel，不需要额外 Rust 工具；源码构建和缺少 wheel 的平台另列。见 [Python 迁移](python-migration.md)。

## 候选交付与证据

发布流程验证 Rust 归档与 Python wheel 安装。详尽的性能和生产证据按验证候选单独记录。

SDK 与下层库的版本、许可及注册表身份见 [发布政策](release-policy.md)。新增 SDK 不改变仓库原有学习研究及非商业许可条件。

## 已知限制与不作承诺的范围

MAC-EX没有经独立生产样本证明的默认登录规则；optional MAC setup的身份/顺序/fence不能从合成ACK推导。category22仍属实验；未知单位、涨跌停权威规则和未采样市场/profile保持显式未知。不承诺真实服务器长期可用、覆盖所有市场或零缺陷。
