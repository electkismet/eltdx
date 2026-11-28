# 本地行情文件和专业财报

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.3", features = ["offline"] }`。本地读取只需要明确的文件或目录路径。联网财报使用 `TdxClient`，示例中使用财报服务的专用连接配置。

## 读取本地行情

```rust
use std::path::Path;
use eltdx::Result;
use eltdx::offline::{daily, ReadOptions};

fn show_daily(path: &Path) -> Result<()> {
    let data = daily::read(path, None, &ReadOptions::new())?;
    for bar in data.rows().iter().take(5) {
        println!("{:?} 收盘价 {}", bar.date(), bar.close());
    }
    Ok(())
}
```

`path` 填本地 `.day` 文件路径。第二个参数为换算系数，`None` 让 SDK 根据文件名选择。`ReadOptions` 可配置 `with_max_bytes`、`with_tail` 和 `with_raw`，分别控制读取上限、处理残尾的方式和保留原始数据的方式。`rows()` 返回记录，`ignored_tail_bytes()` 和 `diagnostics()` 可用于检查文件末尾和解析提示。

| 场景 | 接口 | 用法 |
| --- | --- | --- |
| 本地日线 | `offline::daily` | `read` 读取，`last_date` 查看最后日期，`encode` 编码一条记录，`append` 或 `write` 按日线规则写入。`security_coefficients` 按文件名取得换算配置。 |
| 本地分钟线 | `offline::minute` | `read_lc` 读取 `.lc1` 或 `.lc5`，`read_5` 读取 `.5`。编码、查末尾时间及写入方法也按 `_lc` 和 `_5` 区分。 |
| 扩展日线 | `offline::ex_daily` | `read` 读取，`last_date` 查最后日期，`encode`、`append`、`write` 分别用于编码或写入。 |
| 板块 | `offline::block` | `read` 接收板块文件，`read_custom` 接收自定义板块目录。通过 `rows()` 取得板块记录。 |
| 行业 | `offline::industry` | `read` 接收配置文件路径，`parse` 接收已有字节，返回 `IndustryMap`。 |
| 股本权息 | `offline::gbbq` | `read` 接收本地股本权息文件，通过 `rows()` 获取记录。 |
| 路径与对齐 | `offline::paths`、`offline::alignment` | `resolve_vipdoc` 定位数据目录，`find_*` 按市场和证券代码生成路径。`ensure_aligned` 按记录字节数检查文件对齐。 |

写入接口接受记录切片和 `AppendOptions`，返回 `WriteReport`。可用 `accepted()`、`skipped()` 和 `bytes_written()` 确认实际写入结果。首次操作建议使用单独的输出路径，`write` 的追加语义见对应模块。

[查看本地文件各模块的完整类型和签名](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/offline/)

## 查找并读取专业财报

```rust
use std::path::Path;
use eltdx::{OperationOptions, Result};
use eltdx::financial::{open_calc_client, CalcClientConfig};
use eltdx::offline::financial::{read_detailed, FinancialReadOptions};

fn list_reports() -> Result<()> {
    let client = open_calc_client(&CalcClientConfig::new())?;
    let result = client.financial().list_archive_files(&OperationOptions::default());
    let closed = client.close();
    for file in result?.iter().take(5) {
        println!("{} {:?} 字节", file.filename(), file.filesize());
    }
    closed
}

fn show_local_report(path: &Path) -> Result<()> {
    let data = read_detailed(path, &FinancialReadOptions::new())?;
    println!("{} 条，状态 {:?}", data.rows().len(), data.completion());
    Ok(())
}
```

- 在线下载时，先用 `list_archive_files` 取得实际文件名，再用 `FinancialArchiveName::new` 和 `FinancialRecordsQuery::new` 构造查询。`download_records` 返回记录，`download_records_detailed` 同时提供日期来源和完整性信息。需要调整读取限制时用 `download_records_with_options`。
- `list_archive_files_detailed` 保留目录来源，`download_archive` 只返回已读取的下载字节和完成状态。下载 ZIP 时检查 `completion`。
- 本地文件使用 `read` 或 `read_detailed`。已有字节时用 `parse_dat`、`parse_dat_detailed`、`parse_zip` 或 `parse_zip_detailed`，按文件格式选择。`FinancialReadOptions` 可设置档案上限、ZIP 内文件选择和报告日期。
- 用 `rows()` 获取记录，用 `date_source()` 检查报告日期依据，用 `completion()` 判断解析状态。

[查看联网财报服务](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/financial/struct.FinancialService.html) · [查看本地财报解析接口](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/offline/financial/)
