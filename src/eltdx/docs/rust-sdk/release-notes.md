# Rust SDK 0.1.0 候选说明

**状态：未发布、统一验证待开始。** 本页是候选变更说明，不是可用版本公告或性能结论。Python产品候选仍沿用3.3.5版本来源；新增SDK的0.1.0不要求Python用户安装第二个产品。

## 新增独立 Rust 产品

- 新增公开 `eltdx` Rust crate，复用 protocol/runtime，共同承载业务实现；普通Rust消费不依赖Python、PyO3、maturin或后台Python服务。
- 默认提供同步HQ服务和HQ helper；offline、F10、EX、MAC/MAC-EX及serde按feature启用，mac包含ex。没有伪装为异步的async功能。
- 惰性build、共享owner、显式关闭、可取消操作、整次期限/尝试/页/记录/字节预算、客户端metadata隔离和有来源的raw/单位/日期模型。
- HQ覆盖目录、报价/深度、K线/分钟/逐笔、竞价/日资金流、资本财务、资源/统计/公司/日历和组合helper；F10具名端点、动态表和历史资金流；扩展报价/位图/目录/板块/复权/下载；本地行情、股本、行业及专业财报DAT/ZIP。

具体签名和行为以[用户指南](user-guide.md)、[公共API](public-api.md)与分域规格为准。上述范围表示候选实现源码的目标覆盖，不表示每个接口已有通过结果。

## Python 兼容工作

标准Python入口按域接入同一Rust业务源码，公开Python类/属性/参数/返回模型和必要对象适配继续保留。原逐请求timeout、缓存identity、自定义transport/hook、异常context/cause、关闭后重开和文件append行为分别有合同。严格Rust规则不会自动改变Python默认。

普通Python用户继续使用匹配wheel，不需要额外Rust工具；源码构建和缺少wheel的平台另列。完整兼容矩阵、真实安装与非空查询均未执行，不能在此提前宣布“完全不影响”。见[Python迁移](python-migration.md)。

## 候选交付与证据

新增独立Rust和已安装wheel消费者、feature/MSRV/三平台矩阵、归档消费者、原生压力和固定语料新旧性能工具；每条实际记录应绑定source/lock/脚本/fixture/产物hash，复验重新读取原始日志。工具源码尚未运行，不预填速度提升、资源峰值或通过率。

SDK与下层库的版本、license/include、注册表身份和演练要求见[发布政策](release-policy.md)。本次没有发布、上传、tag、push或合并。新增SDK不改变仓库原有学习研究及非商业许可条件，使用前须阅读原LICENSE。

## 已知限制与不作承诺的范围

MAC-EX没有经独立生产样本证明的默认登录规则；optional MAC setup的身份/顺序/fence不能从合成ACK推导。category22仍属实验；未知单位、涨跌停权威规则和未采样市场/profile保持显式未知。不承诺真实服务器长期可用、覆盖所有市场或零缺陷。

当前依赖声明、lock、跨平台、Python兼容、包和压力性能仍需统一验收；首个源码候选不能替代这些结果。最终发布说明应补上实际候选SHA、工具链/平台、通过/失败/未运行项与证据位置；不存在的结果保持空缺，不删除已知限制。
