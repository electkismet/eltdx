# 专业财报下载 `eltdx.financial`

`eltdx.financial` 从通达信**计算服务器**下载专业财报文件并解析为结构化记录：先取 `tdxfin/gpcw.txt` 得到可用财报文件列表，再下载对应的 `gpcwYYYYMMDD.zip` 并解出每只股票的财务记录。

底层**复用** eltdx 既有的 `0x06b9` 文件下载能力（`ResourceApi.download_file`），不重复实现 socket / 帧逻辑。

## 入口

| 入口 | 说明 |
| --- | --- |
| `open_calc_client(host=None, *, timeout=8.0)` | 构造指向计算服务器的 `TdxClient`（`probe_hosts=False`） |
| `list_archive_files(client)` | 下载并解析 `gpcw.txt`，返回 `list[FinancialFileInfo]` |
| `download_archive(client, filename)` | 下载单个财报 zip 的原始字节 |
| `download_records(client, filename)` | 下载 zip → 解 `.dat` → 返回 `list[FinancialRecord]` |
| `CALC_HOSTS` | 默认计算服务器列表 |

## 示例

```python
from eltdx.financial import (
    CALC_HOSTS,
    download_records,
    list_archive_files,
    open_calc_client,
)

with open_calc_client() as client:              # 指向计算服务器
    files = list_archive_files(client)
    print(files[0].filename)                    # 形如 tdxfin/gpcw20260331.zip
    records = download_records(client, files[0].filename)
    for record in records[:3]:
        print(record.market_id, record.fields[:4])
```

也可以把任意具备 `client.resources.download_file(path) -> bytes` 的对象传入 `list_archive_files` / `download_records`。

## 说明

- zip 损坏或缺少 `.dat` 时抛 `eltdx.exceptions.ResourceFormatError`。
- 解析器与模型同时被 `eltdx.offline.read_history_financial` 复用，保持单一事实来源。
- 计算服务器与行情主站是不同服务器，`open_calc_client()` 默认使用 `CALC_HOSTS`。
