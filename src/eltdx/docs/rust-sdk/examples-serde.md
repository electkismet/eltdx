# Serde：数值、bytes、日期和未知值

需要启用 `serde` feature；它为明确支持的类型增加 Serde traits，不提供 HTTP 服务，也不需要 Python。序列化结果采用当前候选的派生结构，并非跨版本稳定的存储协议。应用若长期保存或公开 JSON，应定义自己的带版本 DTO 和校验规则。以下格式与合同源码目前都未编译或执行。

## 普通序列化调用

以下函数可交给任意 Serde serializer；不要求 SDK 的生产依赖启用 `serde_json`。应用使用 JSON 时自行声明该库。

```rust
use eltdx::types::{NumericValue, Price, ValueProvenance};
use serde::{Serialize, Serializer};

fn write_price<S: Serializer>(serializer: S) -> Result<S::Ok, S::Error> {
    let price = Price::with_value_and_provenance(
        NumericValue::ScaledInteger { value: 12345, scale: 3 },
        ValueProvenance::WireRule,
    );
    price.serialize(serializer)
}
```

对 JSON，上例是 `{"value":{"ScaledInteger":{"value":12345,"scale":3}},"provenance":"WireRule"}`。缩放值仍保留整数与 scale，不先转浮点或抹去来源。`NumericValue::F32` 和 `F64` 保留变体区别；有限负零仍按浮点序列化，由支持该表达的格式保留符号。JSON 合同分别检查负零的 f32/f64 原始位。

## 有限与非有限数值

`NumericValue` 的 f32/f64 序列化器对 NaN、正无穷和负无穷返回 serializer error，不能静默输出 `null`、零或字符串。嵌套在 `Price`、`Quantity`、`Amount`、`Ratio` 中同样适用。它们的有限值可反序列化并保留变体/单位/来源；数量和金额同时保留显示值及 `source_value/source_unit`。

拒绝导出不表示 wire 解码已拒绝非有限数据。调用者仍可用 `wire_f32_bits()` / `wire_f64_bits()` 检查原始位，或单独定义原始位诊断 DTO。一般 Serde 格式是否接受非有限反序列化输入取决于该格式；本策略不把 `Deserialize` 当成业务有效性证明。价格与数量容器也不是会核实货币、交易规则或行情可信性的 checked identity。

## bytes 与 raw

`RawData` 的 JSON `payloads`、已保留的 `wire_frames` 是二维整数数组，每个 byte 为 0–255；不是 base64，也不是损失性 UTF-8 文本。`wire_frames: null` 表示没有保留 wire，`[]` 表示显式存在但没有帧，两者不混同。`row_ranges` 每项是 `[payload_index, offset, length]`，`xor93_rows` 指示对应行是否仍为 XOR93 源字节。

例如：`{"payloads":[[0,93,255]],"wire_frames":null,"row_ranges":[[0,1,2]],"xor93_rows":[false]}`。某个结果的 `raw: null` 与一个存在但内容为空的 `RawData` 也不同。原 payload/wire 没有因序列化而被替换，JSON 导出会分配内存；`RawPolicy::Omit` 不代表全流程零拷贝。`RawData` 只提供 `Serialize`，不能靠反序列化构造一份被误认为 SDK 来源认证的结果。

## 日期、时间与未知值

`TradingDate` 序列化为 `{"year":2024,"month":2,"day":29}`，不是 epoch 或带时区字符串。`EventTime` 使用显式变体：`{"TradingDate":{...}}`、`{"Shanghai":{"date":{...},"hour":9,"minute":30,"second":1}}`、`Local`、`TimeOfDay` 和 `{"UnknownRaw":123}`。`Local` 不擅自附加 UTC/上海时区；`TimeOfDay` 不补日期；`UnknownRaw` 不猜测单位、日期或时区。

单位或 provenance 中明确的 `Unknown` 序列化为字符串 `"Unknown"`，可按该已知变体反序列化。它不意味着接受任意未来变体文本；例如 `"FutureUnit"` 仍报错。业务层也不能由 `Unknown` 推出零值、默认单位或权威规则。

## checked 类型不提供无校验反序列化

`SecurityId`、`TradingDate`、`EventTime` 只导出 `Serialize`，当前未实现 `Deserialize`。外部输入须先读入应用 DTO，再调用 checked 构造器或 `FromStr`。例如 `TradingDate::new(2023, 2, 29)` 必须失败，不能通过派生反序列化注入该日期。下面三段是分别约束 trait 缺失的 compile-fail 合同，由开启 `serde` 的 doctest 后续实际编译。

```compile_fail
fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
requires_deserialize::<eltdx::types::SecurityId>();
```

```compile_fail
fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
requires_deserialize::<eltdx::types::TradingDate>();
```

```compile_fail
fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
requires_deserialize::<eltdx::types::EventTime>();
```

实际运行合同位于 `crates/eltdx/tests/serde_contract.rs`：有限值/极端整数/scale/provenance、f32/f64 负零、各三种非有限值拒绝及原始位不变、显示与来源单位、raw byte/缺失/空数组、五类时间格式、checked constructor 拒绝及未知变体边界。测试的 `serde_json` 是 dev-dependency；`serde` 单独 feature 不因此增加 JSON 生产依赖。编写了合同不等于其已通过，最终须运行对应 feature 的 integration tests 与 doctests。
