# Documentation

这是 `eltdx` 的文档入口。

如果你只是想安装、调用接口、理解字段，优先看公开文档；更深入的实现、验证、发布资料放在维护者目录。

## 推荐阅读顺序

### 1. 项目首页

- [`../README.md`](../README.md)
  - 了解项目定位
  - 看安装方式
  - 看 30 秒上手示例
  - 看整体 API 总览与 FAQ

### 2. API 用法

- [`API_REFERENCE.md`](./API_REFERENCE.md)
  - `TdxClient` 如何初始化
  - `connect()` / `close()` / `with` 如何使用
  - 每个公开方法的参数、返回值、调用方式
  - 服务层对象怎么用

### 3. 可直接运行的示例

- [`EXAMPLES.md`](./EXAMPLES.md)
  - 一次性抓取怎么写
  - 长连接场景怎么写
  - 快照 / 分时 / 逐笔 / K 线 / 集合竞价怎么写
  - `include_raw=True` 调试怎么写

### 4. 字段说明

- [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
  - dataclass 字段总表
  - 每个字段的中文含义
  - 原始值 -> 解析值的对照示例
  - 时间字段、价格字段、`*_milli` 字段怎么理解

### 5. 调试与排查

- [`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)
  - raw / hex 怎么看
  - smoke 脚本怎么跑
  - 如何定位常见连接与解析问题

## 按需求找文档

### 我想知道“某个函数怎么调用”

看：[`API_REFERENCE.md`](./API_REFERENCE.md)

### 我想直接复制一段能跑的代码

看：[`EXAMPLES.md`](./EXAMPLES.md)

### 我想知道“某个字段到底是什么意思”

看：[`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)

### 我想调试 raw / hex / smoke

看：[`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md)

### 我想看架构、验证记录、发布流程

看：[`maintainers/README.md`](./maintainers/README.md)

## 文档地图

| 文档 | 作用 | 适合谁看 |
| --- | --- | --- |
| [`../README.md`](../README.md) | 项目首页 | 第一次打开仓库的人 |
| [`API_REFERENCE.md`](./API_REFERENCE.md) | 完整 API 说明 | 想系统看全部方法的人 |
| [`EXAMPLES.md`](./EXAMPLES.md) | 可运行示例集合 | 想直接复制代码的人 |
| [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md) | 字段中文解释 | 想理解字段含义的人 |
| [`DEBUG_GUIDE.md`](./DEBUG_GUIDE.md) | 调试指南 / smoke | 想排查问题的人 |
| [`maintainers/README.md`](./maintainers/README.md) | 维护者资料 | 想看架构、验证、发布的人 |

## 当前文档分工

- `README.md`：让人快速理解项目与用法
- `API_REFERENCE.md`：把方法签名、参数、返回模型讲清楚
- `EXAMPLES.md`：给出可直接复制的代码
- `FIELD_REFERENCE.md`：把字段名翻译成能读懂的中文指标
- `DEBUG_GUIDE.md`：把 raw、smoke、排查方法讲清楚
- `maintainers/`：存放架构、验证、发布等维护资料

## 如果你只看 3 份文档

建议优先看这 3 份：

1. [`../README.md`](../README.md)
2. [`API_REFERENCE.md`](./API_REFERENCE.md)
3. [`FIELD_REFERENCE.md`](./FIELD_REFERENCE.md)
