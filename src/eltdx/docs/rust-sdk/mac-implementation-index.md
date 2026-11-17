# MAC / MAC-EX 实现与恢复索引

当前为T25部分实现，基线`80de370`后的Stage B查询与分页源码检查点stage77。
**没有编译、测试、联网或安装证据；没有注册可用的公开MAC客户端。**
全部T01–T30实现写完后才统一验证；目标保持active且不设置额度。

## 文件与已写源码

| 文件 | 范围 |
|---|---|
| `crates/eltdx/src/mac/models.rs` | MAC22B代码/u16市场、独立70B远程文件名、wire float/slot/bar/transaction基础类型 |
| `mac/bitmap.rs` | 全部具名field/preset、20B响应位图、合法未知位占槽、reserved151..159、16B请求拒高位 |
| `mac/commands.rs` | 单一请求布局、quote/bar/transaction/kline-offset及login原始请求/仅首字节rewrite |
| `mac/responses.rs` | 私有字段/只读getter、源identity/float/未知单位、图/快照/板块/商品/文件/JSON响应 |
| `mac/decode.rs` | 下表简单响应的严格单帧payload解析，无I/O或分页/缓存 |
| `mac/routing.rs` | 27/31/48/49/71/98港股逐笔路由和七字段MAC-EX默认 |
| `mac/enums.rs` | 完整源枚举、OR filter、checked/source-preserving BoardSymbol、SourceOrder |
| `mac/query.rs` | 全部标准MAC查询构造/默认值，u32总目标、窄字段/bitmap/下载范围校验 |
| `mac/ex_query.rs` | canonical EX身份、全部MAC-EX查询、1800/1000页、50/90000 cap、macro默认 |
| `mac/pagination.rs` | 单一checked分页进度、实际wire行推进、短/空证据和满保护上限拒绝；列表追加/K线前插 |
| `crates/eltdx/tests/mac_wire.rs` | 7个原Stage A函数源码，未执行 |
| `crates/eltdx/tests/mac_responses.rs` | 14个简单响应/边界函数源码，未执行 |
| `crates/eltdx/tests/mac_queries.rs` | 10个查询/默认/编码/身份边界函数源码；另pagination内部6个函数，全部未执行 |

MAC模块仍未在crate root注册。feature mac已依赖ex；JSON使用现有optional
serde_json，未引入第二网络/解压栈。当前源码路径包含用例只用于最终验证，
不能据此宣称独立入口已可用。

## MAC全部23个方法的归属

全部query已写；下列每个方法仍待独立client/profile和响应meta接线。
“已有”仅表示纯codec/checked query源码，不表示公开入口完成。

| 方法 | 已有基础 | 尚待工作 |
|---|---|---|
| quotes | 0x122B bitmap/rows | 1..80 query、owner、meta/raw |
| quote_list | 0x122C rows | Category/SortType/FilterMask、80分页 |
| bars | 0x122E四flag/36B记录 | u32目标/700分页、时间投影；T26复权修复 |
| tick_chart | 0x122D18B tick +120B tail | checked id/date及返回meta |
| tick_charts | 0x123E14B tick/5日期/短首日 | checked days/date及返回meta |
| chart_sampling | 0x254D count/float bits | owner/meta |
| transactions | 0x122F18B记录 | u32目标/1000分页、PageResult |
| symbol_info | 0x122A194B已知字段及tail | checked id、时间/单位/meta |
| boards | 0x1231配对160B记录 | checked BoardType、分页 |
| board_members | 0x122C bitmap/rows | BoardSymbol、80分页 |
| belonging_boards | 0x1218 flag1/27B prefix/GBK JSON | <=8B专用请求代码验证、meta |
| board_summary | 基础members/fields | T26全成员聚合 |
| board_ranking | 基础boards/members | T26全候选聚合再稳定排序/topN |
| board_change_ranking | 基础boards/bars | T26完整历史、缺历史诊断和稳定排序 |
| capital_flow | 0x1218 flag2独立双表/Presence | checked请求/meta，不能套Python默认0 |
| auction | 0x123D16B有符号unmatched | checked id/固定请求500/meta |
| unusual | 0x1237记录/类型/文本suffix | checked market/页目标/meta |
| server_info | 0x120F日期/window/参数/tail | 数据探活有效日期要求/diagnostic |
| kline_offsets | 0x124A混合端序 | query/owner/meta；returned不是EOF |
| file_meta | 0x1215/70B名称/41B结果 | query/owner/meta；hash算法未知 |
| download_chunk | 0x1217/8B prefix/size上界 | checked index>=1/size/query/来源 |
| download_file | 基础meta/chunk | T26固定同源/完整性/上限/EOF组合 |
| goods | 0x2562/48B记录 | checked count<=1000/PageResult |

## MAC-EX全部13个方法的归属

共享MAC请求和模型，复用T24门/worker/frame，不复制网络循环或MAC布局。

| 方法 | 简单基础与组合归属 |
|---|---|
| macro_indicators | goods市场38；T26完整目录/缓存 |
| macro_quotes | quotes；T26代码/名称唯一解析及目录缓存 |
| macro_bars | bars；T26代码/名称解析 |
| goods_count | 复用EX count/目录；T26指定市场计数 |
| goods | 0x2562；T26明确诊断后的EX目录fallback/边界缓存 |
| instruments | 复用EX info完整desc；T26目录范围 |
| quotes | 共享0x122B，默认恰好0/1/2/3/4/5/7 |
| quote_list | T26 goods/source order前min(count,80)；新排序不支持时构造拒绝 |
| bars | 共享0x122E布局；T26复权处理 |
| tick_chart | 共享0x122D |
| chart_sampling | 共享0x254D |
| transactions | 港股EX23FC/2406，其余MAC122F；新session必须login |
| all_transactions | T26港股1800分页、50页/90000到上限仍满报LimitExceeded |

MAC-EX查询已统一使用ExInstrumentId，再做显式无损MAC转换；市场号不推断HQ
交易所。转换拒绝u8溢出、GBK不可表示和反向超过9B UTF-8代码。Stage A的
MacExInstrumentId仅为旧纯codec groundwork，未被新公开查询采用。

## 严格模型与旧Python边界

新的响应结构用WireFloat保存每个f32 bit；未知单位保持Unknown。
GoodsItem.code是3个浮点占位，不当证券代码，category不当market。
板块与领涨证券独立身份；16B padding不拼进六字节代码。

两类JSON先跳27字节prefix，随后严格GBK和数组校验。所属板块支持9/13字段行；
资金流保存today/five-day两表，Missing/Null/Present/Derived和原JSON字段。
缺失金额不变成0；只在左右都存在时算净额，不自造日期和金额单位。
T27仍须保持旧Python宽容转换、默认0、异常/求值顺序、公开dataclass与返回类型。

分时tail120B，单日tick18B、多日tick14B。有效空图有完整header/tail，
不足字段是Protocol。多日图固定最多5日期，日期槽/页容量/完整tick数均检查，
按短首日/后续整日/空日分配。symbol HHMMSS、tick minutes和transaction seconds
分别解释，不能互换。symbol date/time全0为None。

server-info已知核心71B，两个可选u32在71/75，剩余tail保留；坏日期保留源值，
不生成TradingDate。T25探活应要求today有效。Python旧87B门槛与空日期模型投影
继续由T27兼容层处理。file hash没有算法证据，不声称完成完整性校验。

## 接下来与未验证证据

1. checked query/enums/page进度、身份转换及用例源码已写；仍未编译或执行。
2. 实现标准MAC三阶段OptionalReply及MAC-EX独立login profile；复用T24框架。
   源码只有“登录体>=2B”启发式，缺少生产成功/否定payload证据；不能编造ACK。
   可选setup的静默超时不证明响应边界，必须防晚到setup被当业务响应。
3. 接入独立MacClient/MacExClient、lazy builder/管理/探测/关闭、同一个操作预算。
4. 接入分页/meta/raw，补真实公开client回环fixture源码后登记模块。
5. T26完成目录/fallback/缓存、聚合/排行、复权、同源下载及all-pages；T27兼容迁移。
6. T28–T30消费者、打包、文档全部源码完成后统一验证，并解决实际编译/平台问题。

详细恢复见`.superpowers/sdd/development-plan/task-25-report.md` Stage B checkpoint2。
实现源码完整不等于执行验收通过，当前任务总数仍23完成源码/2部分/5未开始。
