# Python API 基线：签名、默认值、模型与导出

> 本文与 [baseline-python-api.json](baseline-python-api.json) 是迁移前的静态契约快照，基线为 `af471f1adf38757f6aa0188968db8110eb23f6ad`。不执行 Python 模块、不要求 native 扩展，也不把源码签名等同于已验证行为。

## 1. 为什么需要这份基线

[能力矩阵](capability-matrix.md) 用于分工，[逐项兼容案例](compatibility-cases.md) 用于行为验收；本快照固定源码中具体的参数、默认表达式、返回注解、dataclass 字段、属性、枚举、方法别名与 `__all__`。修改后应能够指出哪个契约发生了变化，不能仅以“主要方法还在”说明兼容。

JSON 覆盖基线提交 `src/eltdx/` 下全部第一方 `.py` 模块，排除文档镜像。它保留公共名称定义和构造/容器/上下文协议方法，也记录内部支持模块的候选定义；**候选定义数量不是稳定 API 数量**。对外兼容范围由源码导出、文档使用、调用方依赖和能力矩阵共同判定。

实际静态提取结果：134 个模块、231 个类、247 个模块函数、724 个类方法/协议方法、1220 个带注解的类字段。这些计数来自本文生成器；属性包含在方法计数中，公开重导出不会重复复制定义，不能相加称为“总接口数”。

本轮在同一环境重复生成后的JSON逐字节一致，SHA-256为 `f31ca22ea06c99df276358f148b7e66c7b20196e76073d2e4b65ce3141f29a28`。此摘要针对当前schema和生成器；生成器或schema有意变化时须重新记录，不能把不同输出覆盖为同一基线版本。

| 源码组 | 模块 | 模块函数 | 类 | 方法/协议方法 | 带注解类字段 |
| --- | --- | --- | --- | --- | --- |
| 根目录文件 | 29 | 110 | 22 | 25 | 51 |
| api | 17 | 2 | 15 | 79 | 0 |
| ex | 25 | 23 | 24 | 83 | 77 |
| f10 | 6 | 12 | 16 | 207 | 94 |
| helpers | 4 | 3 | 26 | 53 | 209 |
| mac | 11 | 20 | 52 | 105 | 144 |
| models | 14 | 0 | 51 | 108 | 514 |
| offline | 13 | 39 | 8 | 3 | 58 |
| protocol | 8 | 28 | 4 | 2 | 19 |
| transport | 7 | 10 | 13 | 59 | 54 |

## 2. 快照字段与解释

| 字段 | 定义 | 验收用途 |
| --- | --- | --- |
| `schema_version` | 快照格式版本，初始 1 | 格式修改必须先升级读取器 |
| `source_revision` | 完整基线提交 SHA | 不受工作区新增文档影响 |
| `extraction` | AST 静态提取；无 import/执行 | 明确证据边界 |
| `modules[].path/module/source_sha256` | 文件路径、导入路径、原字节摘要 | 确认读取的实际源码 |
| `scope` | `public_candidate` 或 `internal_support` | 私有路径不自动升级为稳定接口 |
| `all` | `__all__` 的静态表达式及可解析字符串列表 | 公共导出核对，`null` 不代表“没有导出” |
| `imports` | 模块级 import/re-export，保留相对层级和 alias | 重导出与直接定义分开检查 |
| `functions` | 模块级候选公开函数 | 精确比较参数与注解 |
| `classes` | 候选公开类、基类、decorator、方法、字段及成员 | 模型与服务 API 基线 |
| `aliases` | 模块/类中公开名称绑定表达式 | 如 `Client = TdxClient`、方法别名不能漏掉 |
| `signature/parameters/returns` | AST 规范化签名、参数顺序、种类、默认表达式、返回注解 | 区分位置参数、仅关键字参数、可变参数及默认值 |
| `fields` | 带注解字段，保留顺序、默认表达式 | dataclass 构造与序列化顺序核对 |
| `members` | 类中的公开赋值表达式 | 包含 enum/常量；不自动认定为 dataclass 字段 |
| `properties` | `@property` 或 setter/getter/deleter 方法索引 | 属性不能替换成要求调用的普通方法 |
| `entry_points` | `pyproject.toml` 的 console scripts | CLI/MCP/HTTP 入口名称不因 SDK 增加而丢失 |

`has_default=false` 与默认表达式字符串 `"None"` 不同。`annotation=null` 表示源码没有注解，不能据此推断返回 `None`。默认表达式例如 `DEFAULT_PROBE_TIMEOUT` 保存其名称；它的实际值必须在语义案例中另外固定。`ast.unparse` 会规范化空格与引号，比较的是语法结构，不是源码排版。

## 3. 兼容检查规则

1. 先比较所有公开 import/re-export 和 `__all__` 名称；建立真实 Python 导入检查，检测 import-time 异常及缺少 native ABI。静态快照不替代实际导入。
2. 原位置参数、仅关键字参数、默认值和返回模型不得因为 Rust 类型更严格而自动改变。新增内部参数不能泄露到旧签名。
3. dataclass 的字段名、顺序、默认值、frozen/slots、构造方式及派生属性逐项核对；新增诊断优先走独立诊断通道，不能直接给旧模型加必填字段。
4. enum 数值、别名和重导出路径分别核对；不要按名称相同合并 HQ/EX/MAC 市场类型。
5. 同名重复定义按源顺序保留；运行时后定义覆盖前定义，P0 需将有效定义与历史重复明确登记。
6. 继承方法、动态 `setattr`、条件 import、执行时生成的导出、decorator 改写后的签名和任意表达式运行值不由本提取器猜测。P0 用真实对象的 `inspect.signature`、导入、dataclass/enum 反射补证，并保存为独立 runtime 基线。
7. 方法存在不证明输出语义。超时/重试、数据单位、缺值、异常、缓存、文件副作用按 [兼容案例](compatibility-cases.md) 和各域规格验证。

原 Python 产品的外观与行为是迁移约束，新 Rust 产品可以采用严格类型和显式状态。确需修复旧行为时，单独列出旧/新样本和发布决策，不能自动覆盖本快照来掩盖变化。

## 4. 可重复生成方法

以下代码是**静态文档快照生成器**，只读取 Git 基线、写入本目录 JSON；不修改运行实现。它固定按 Git tree 读取，避免把本轮文档或其他工作区改动误当基线。Python 3.11+ 可执行；输出不带当前时间，因此同环境重复生成可逐字节比较。

将本节 Python 围栏保存为临时脚本或在仓库根运行。默认目标是当前仓库的 `docs/rust-sdk/baseline-python-api.json`；不能将用户工作目录替换成其他仓库后仍声称同一基线。

```python
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

REVISION = "af471f1adf38757f6aa0188968db8110eb23f6ad"
ROOT = Path(subprocess.check_output(
    ["git", "rev-parse", "--show-toplevel"], text=True, encoding="utf-8"
).strip())
DESTINATION = ROOT / "docs/rust-sdk/baseline-python-api.json"
SPECIAL = {
    "__init__", "__enter__", "__exit__", "__getitem__", "__len__",
    "__iter__", "__contains__", "__next__", "__bool__", "__call__",
    "__aenter__", "__aexit__", "__aiter__", "__anext__",
}

def git_bytes(path):
    return subprocess.check_output(
        ["git", "show", f"{REVISION}:{path}"], cwd=ROOT
    )

def expr(node):
    return ast.unparse(node) if node is not None else None

def visible(name):
    return not name.startswith("_") or name in SPECIAL

def parameter(arg, kind, default=None, has_default=False):
    return {
        "name": arg.arg,
        "kind": kind,
        "annotation": expr(arg.annotation),
        "has_default": has_default,
        "default_expr": expr(default) if has_default else None,
    }

def function(node):
    args = node.args
    positional = list(args.posonlyargs) + list(args.args)
    first_default = len(positional) - len(args.defaults)
    params = []
    for i, arg in enumerate(positional):
        has_default = i >= first_default
        default = args.defaults[i - first_default] if has_default else None
        kind = "positional_only" if i < len(args.posonlyargs) else "positional_or_keyword"
        params.append(parameter(arg, kind, default, has_default))
    if args.vararg:
        params.append(parameter(args.vararg, "var_positional"))
    for arg, default in zip(args.kwonlyargs, args.kw_defaults):
        params.append(parameter(arg, "keyword_only", default, default is not None))
    if args.kwarg:
        params.append(parameter(args.kwarg, "var_keyword"))
    decorators = [expr(item) for item in node.decorator_list]
    returns = expr(node.returns)
    return {
        "name": node.name,
        "line": node.lineno,
        "async": isinstance(node, ast.AsyncFunctionDef),
        "signature": f"{node.name}({ast.unparse(args)})" + (f" -> {returns}" if returns else ""),
        "parameters": params,
        "returns": returns,
        "decorators": decorators,
    }

def assignments(body):
    result = []
    for node in body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and not target.id.startswith("_"):
                    result.append({"name": target.id, "line": node.lineno,
                                   "value_expr": expr(node.value)})
    return result

def class_record(node):
    methods = [function(item) for item in node.body
               if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))
               and visible(item.name)]
    fields = [{"name": item.target.id, "line": item.lineno,
               "annotation": expr(item.annotation),
               "has_default": item.value is not None,
               "default_expr": expr(item.value)}
              for item in node.body if isinstance(item, ast.AnnAssign)
              and isinstance(item.target, ast.Name)
              and not item.target.id.startswith("_")]
    return {
        "name": node.name, "line": node.lineno,
        "bases": [expr(item) for item in node.bases],
        "keywords": [{"name": item.arg, "value_expr": expr(item.value)}
                     for item in node.keywords],
        "decorators": [expr(item) for item in node.decorator_list],
        "fields": fields, "methods": methods,
        "properties": [item["name"] for item in methods
                       if any(d == "property" or d.endswith((".setter", ".getter", ".deleter"))
                              for d in item["decorators"])],
        "members": assignments(node.body),
    }

def module_record(path):
    raw = git_bytes(path)
    tree = ast.parse(raw.decode("utf-8-sig"), filename=path)
    relative = Path(path).relative_to("src")
    parts = list(relative.with_suffix("").parts)
    if parts[-1] == "__init__":
        parts.pop()
    module = ".".join(parts)
    private = any(part.startswith("_") and part != "__init__.py"
                  for part in Path(path).relative_to("src/eltdx").parts)
    imports = []
    exports = None
    for node in tree.body:
        if isinstance(node, ast.ImportFrom):
            imports.append({"from": node.module, "level": node.level,
                            "names": [{"name": a.name, "asname": a.asname}
                                      for a in node.names]})
        elif isinstance(node, ast.Import):
            imports.append({"from": None, "level": 0,
                            "names": [{"name": a.name, "asname": a.asname}
                                      for a in node.names]})
        elif isinstance(node, ast.Assign) and any(
            isinstance(t, ast.Name) and t.id == "__all__" for t in node.targets
        ):
            try:
                value = ast.literal_eval(node.value)
            except (ValueError, TypeError):
                value = None
            exports = {"expression": expr(node.value), "literal_names": value}
    return {
        "path": path, "module": module,
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "scope": "internal_support" if private else "public_candidate",
        "all": exports, "imports": imports,
        "functions": [function(n) for n in tree.body
                      if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
                      and visible(n.name)],
        "classes": [class_record(n) for n in tree.body
                    if isinstance(n, ast.ClassDef) and visible(n.name)],
        "aliases": assignments(tree.body),
    }

paths = subprocess.check_output(
    ["git", "ls-tree", "-r", "--name-only", REVISION, "src/eltdx"],
    text=True, encoding="utf-8", cwd=ROOT,
).splitlines()
paths = sorted(p for p in paths if p.endswith(".py") and "/docs/" not in p)
modules = [module_record(path) for path in paths]
project = tomllib.loads(git_bytes("pyproject.toml").decode("utf-8"))
result = {
    "schema_version": 1,
    "source_revision": REVISION,
    "extraction": "static_ast_no_import_no_execution",
    "summary": {
        "modules": len(modules),
        "classes": sum(len(m["classes"]) for m in modules),
        "module_functions": sum(len(m["functions"]) for m in modules),
        "methods": sum(len(c["methods"]) for m in modules for c in m["classes"]),
        "annotated_class_fields": sum(len(c["fields"]) for m in modules for c in m["classes"]),
    },
    "entry_points": project["project"]["scripts"],
    "modules": modules,
}
DESTINATION.write_text(
    json.dumps(result, ensure_ascii=False, indent=2) + "\n",
    encoding="utf-8", newline="\n",
)
print(json.dumps(result["summary"], ensure_ascii=False))
```

## 5. P0 使用顺序与输出

| 步骤 | 输入 | 输出与完成条件 |
| --- | --- | --- |
| 固定静态契约 | 本 JSON、基线提交 | 签名/字段/导出覆盖清单；全部分类，内部候选不误称稳定 API |
| 补运行期表面 | 匹配 Python/native ABI 的旧产品 | 真实 import、继承属性、decorator 签名、enum aliases、dataclass 反射记录 |
| 固定语义 | 兼容案例、本地脚本服务器与合成文件 | 原结果/异常/请求顺序的独立期望；不使用新实现生成唯一 expected |
| 接入新绑定 | 相同输入、相同服务器时间线 | 旧新 Python 结果及副作用一致；Rust 按自己的严格契约比较 |
| 处理差异 | 逐项差异报告 | 原样兼容、只改变新 Rust、或单独批准变更三种决定之一；没有静默接受差异 |

P0 完成报告必须同时指出尚无真实录包的能力。静态 JSON、合成用例和真实行情验证分别记录，不互相冒充。
