# 财报服务与本地文件示例

需要 `features = ["offline"]`。本页示例当前未编译或运行；后续 rustdoc 只编译这些 `no_run` 代码，不访问用户文件或真实远端。

## FinancialService：下载并解码

FinancialService 由 HQ client 提供，共用其 owner、原始操作期限和下载预算。先取得明确档案名，完整下载后才解码 ZIP/DAT；达到 byte cap、截断 ZIP 或不完整 DAT 不能返回完整 records 成功。服务端文件清单中的 hash 是原始元数据，不表示已验证整个文件完整性。

```no_run
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::financial::{FinancialArchiveName, FinancialRecordsQuery};
fn download(client: &TdxClient, options: &OperationOptions) -> Result<()> {
    let name = FinancialArchiveName::new("gpcw20240331.zip")?;
    let query = FinancialRecordsQuery::new(name);
    let records = client.financial().download_records_detailed(&query, options)?;
    println!("rows={} date_source={:?}", records.rows().len(), records.date_source());
    Ok(())
}
```

调用者配置明确 endpoint，并在错误或成功后 close client；`financial_download` example 展示了保留业务主错误并另报清理错误的完整模式。`open_calc_client` 只构造惰性 client，不自动探测生产主机。

## 显式本地财报读取

没有隐式用户目录扫描。路径由调用者明确传入，ZIP 条目选择、压缩/解压限额和日期来源保留在 detailed 结果中；源日期、文件名日期和未验证字段不能混为一谈。

```no_run
use std::path::Path;
use eltdx::Result;
use eltdx::offline::financial::{read_detailed, FinancialReadOptions};
fn read_report(path: &Path) -> Result<()> {
    let result = read_detailed(path, &FinancialReadOptions::new())?;
    println!("rows={} completion={:?}", result.rows().len(), result.completion());
    Ok(())
}
```

本地 day/minute/EX、股本、行业和板块的逐函数归属见 `offline-migration-index.md`。本例只说明财报读取，不把一次 DAT 成功推导为所有文件格式已通过。
