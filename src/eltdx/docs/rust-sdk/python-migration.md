# Python 用户兼容与开发迁移

本次目标是新增独立 Rust 产品，并让 Python 复用同一份 Rust 业务实现。现有 Python 用户无需因为 SDK 新增而改写调用；这项兼容承诺须由候选 wheel 的旧合同与差分回归验收，当前尚无本轮通过结论。Rust 新接口的严格规则不会自动成为 Python 新默认。

## 普通用户安装

支持平台应安装匹配 Python/ABI/架构的预编译 wheel。wheel 已包含 native 扩展，不要求再安装 Cargo SDK、rustc、maturin 或常驻服务。源码安装、贡献开发和没有匹配 wheel 的平台才需要 Rust 构建工具。不能为了让 wheel 消费测试通过而安装编译器或允许 sdist 回退。

`eltdx` 的 Python 产品版本继续由原 workspace 版本来源管理，候选为3.3.5；Rust SDK 首个候选为0.1.0，两者版本线不同。是否能从公开索引安装本次候选取决于另外的发布流程，本文不是发布公告。

```python
from eltdx import TdxClient

client = TdxClient()
try:
    # 保持原有 Python 入口、参数与返回模型。
    # 实际联网只在使用者明确调用查询时发生。
    service = client.codes
finally:
    client.close()
```

此例仅展示原有构造/属性/关闭入口，不以构造成功代替真实查询成功。具体查询继续使用原 Python 文档；Rust 使用方法见[Rust 指南](user-guide.md)。Python 用户不需要理解 Rust service getter 的函数写法。

## 必须保留的原有合同

| 范围 | 保留内容 | 主要验收来源 |
| --- | --- | --- |
| 公开外观 | import、导出、继承、签名/default、别名、property、dataclass 字段和冻结/slots行为 | 冻结静态基线、运行反射、public API/model合同 |
| 查询结果 | 旧行序、重复、缺失、raw 类型、非有限/负零、日期及字段名投影 | 黄金样本、旧输出、逐域 native bridge 差分 |
| 调用时序 | 原逐请求 timeout、计时起点、页数/重试/缺失恢复次数 | deadline合同与受控回环 |
| Python 扩展机制 | custom transport、子类/实例覆盖、动态 model/converter、任意 Python 对象和 codec hook | bridge 合同、custom adapter 反例 |
| 缓存与关闭 | 原缓存 key/identity/clear、弱引用、close 后原重开语义 | 生命周期与 cache/generation 合同 |
| 异常 | 原异常类型、捕获范围、活动 context/cause、KeyboardInterrupt 与 finally 清理 | 错误与中断用例 |
| 文件 | 原 append 语义、调用前末条阈值、残尾拒绝、daily fsync及minute/EX差异 | 逐字节文件与副作用断言 |
| 安装 | 既有 wheel/包内文档/CLI及可选 extras 边界 | 真实构建、隔离安装和已安装消费者 |

例如 Rust 默认8秒为整次高层操作预算，Python 原有每页8秒的行为保持逐请求；不能因为内部共用 Rust 业务函数就把两页共16秒的 Python 调用压成8秒。Rust close 成功永久封闭 owner；Python 原重开路径通过新 generation 实现，而不是把旧 Rust owner 重新打开。

## 维护者为何仍能看到 Python 代码

共享业务算法和 Python 对象协议不是同一层。PyO3 负责对象/异常/DTO 转换及运行 native；Python facade 仍负责原公开类、属性、上下文管理、with/except/finally、动态 hooks 和必须遵守原对象协议的适配路径。自定义 Python decode/struct/transport 不应被强行变成严格 Rust 类型后破坏行为。

这些保留项有退出条件和证据库存：HQ 见[hq-implementation-index.md](hq-implementation-index.md)，F10/本地财报见[t23-migration-index.md](t23-migration-index.md)，扩展见[t27-migration-index.md](t27-migration-index.md)。`__python_compat` 与 `python_legacy` 是绑定实现接缝，不供普通 Rust 消费者使用，也不扩大稳定 Rust API。

迁移属于有界修改和增量接入，并非完全不改 Python 源码。标准内置路径已按域接入 Rust，必要适配保留；不借迁移顺带清理用户可观察的怪异旧行为。发现旧 bug 应另立兼容决策、版本和原始失败样本，不能修改黄金结果来遮盖回归。

## 如何验收“不影响 Python 用户”

先保留旧 revision `af471f1`、原样本及来源，再构建新绑定；不能仅比较两条都调用同一新 Rust 函数的路径。按 Python3.10–3.14 的原合同矩阵、所有逐域 native bridge、本地回环和适配器/CLI/HTTP/MCP 相关套件验收。在线真实服务器、压力和无工具链安装分别记录，不能由 import 成功推出。

独立 wheel 消费者为 `tests/consumers/python-wheel/consumer.py`，通过实际安装的公开 API 发起非空 HQ/F10/EX/MAC/MAC-EX 回环查询，并验证财报、本地文件、raw和关闭。它复制到源码树外，以 `python -I` 运行；记录真实 import/native 路径和 hash，禁止从 `src` 借回未打包文件。构建与消费 job 分离，Linux 纯 Python 镜像与桌面进程 PATH 隔离的证明强度分别保留。

只有上述结果支持候选的兼容结论；写齐桥接源码、出现相同方法名或单次 import 成功都不够。具体待执行项与缺失证据见[最终验收索引](final-acceptance.md)。
