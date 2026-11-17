# MAC / MAC-EX 实现与恢复索引

当前T25/T26部分实现，本地工具已提交`0ec8907`，QFQ组合已提交`95721fc`，板块组合源码检查点为stage83。
**MAC模块/两类公开客户端已在源码登记；没有编译、测试、联网或安装证据，尚未验收可用。**
全部T01–T30实现写完后才统一验证；目标保持active且不设置额度。

## 文件与已写源码

| 文件 | 范围 |
|---|---|
| `crates/eltdx/src/mac/models.rs` | MAC22B代码/u16市场、独立70B远程文件名、wire float/slot/bar/transaction基础类型 |
| `mac/bitmap.rs` | 全部具名field/preset、20B响应位图、合法未知位占槽、reserved151..159、16B请求拒高位 |
| `mac/fields.rs` | 16B/20B具名字段视图、上下文别名及唯一bitmap编码；未知位不从wire消失 |
| `mac/util.rs` | 纯本地逐事件f64前复权、逐事件证据/跳过原因、原始bits保留、parent边界；8个内部用例源码 |
| `mac/services.rs` | QFQ异常/None重取/显式provider/strict/降级及共享meta，owner worker与parent预算/取消/清理；其余组合待实现 |
| `mac/boards.rs` | 全成员汇总、六指标稳定排行/None全候选、完整N日/向旧分页/重复日期证据；共享父保护和全部原始来源；私有字段返回模型 |
| `advanced/mac.rs` | 受限请求/精确16B头/同一frame解压及DEBUG位图，无socket入口 |
| `mac/commands.rs` | 单一请求布局、quote/bar/transaction/kline-offset及login原始请求/仅首字节rewrite |
| `mac/responses.rs` | 私有字段/只读getter、源identity/float/未知单位、图/快照/板块/商品/文件/JSON响应 |
| `mac/decode.rs` | 下表简单响应的严格单帧payload解析，无I/O或分页/缓存 |
| `mac/routing.rs` | 27/31/48/49/71/98港股逐笔路由和七字段MAC-EX默认 |
| `mac/enums.rs` | 完整源枚举、OR filter、checked/source-preserving BoardSymbol、SourceOrder |
| `mac/query.rs` | 全部标准MAC查询构造/默认值，u32总目标、窄字段/bitmap/下载范围校验 |
| `mac/ex_query.rs` | canonical EX身份、全部MAC-EX查询、1800/1000页、50/90000 cap、macro默认 |
| `mac/pagination.rs` | 单一checked分页进度、实际wire行推进、短/空证据和满保护上限拒绝；列表追加/K线前插 |
| `mac/handshake.rs` | 明确optional回复/fence身份、终态tracker和正/负login证据；不虚构生产ACK；5个内部用例源码 |
| `mac/config.rs` | 独立MAC/MAC-EX配置、数字endpoint默认值及共同受控传输限额；纯build |
| `mac/profile.rs` | 三步setup/严格optional fence及独立login，复用现有frame/write/read实现 |
| `mac/session.rs` | 独立owner/session、现有门与worker/授权/取消清理、meta/raw及共享探测调度 |
| `mac/client.rs` | 两类builder/管理/探测/关闭；MAC22/23、MAC-EX6/13入口源码已接入；剩余组合仍待实现 |
| `mac/quote.rs` | checked MAC身份、名称源字节、bit字段/未知位和逐响应meta的公开Quote |
| `mac/market_data.rs` | 富Bar/Transaction、精确秒级/跨日/原始时间、七个float bits、独立f64有效OHLC/逐事件证据、HK原价/count presence/来源和共享meta |
| `mac/collect.rs` | 80/700/1000/1800分页、整页前插、raw行排列、复用aggregate_meta/HK现有EX解码和all严格终止 |
| `crates/eltdx/tests/mac_wire.rs` | 7个原Stage A函数源码，未执行 |
| `crates/eltdx/tests/mac_responses.rs` | 14个简单响应/边界函数源码，未执行 |
| `crates/eltdx/tests/mac_queries.rs` | 10个查询/默认/编码/身份边界函数源码；另pagination内部6个函数，全部未执行 |
| `crates/eltdx/tests/mac_session.rs` | 19个公开入口/非空分页/秒级/raw/HK/lifecycle/取消/半包/换源/超量用例函数源码，全部未执行 |
| `crates/eltdx/tests/mac_helpers.rs` | 6个字段视图/别名/unknown/debug/请求/头/有界zlib/板块转换函数源码，全部未执行 |
| `crates/eltdx/tests/extended_services.rs` | 13个QFQ修复/降级/strict/来源/字节/额外次数/原始行映射公开函数源码，全部未执行 |
| `crates/eltdx/tests/extended_boards.rs` | 16个全成员/全候选/六指标/原始行/缺失/非有限/目标历史/重复日期/共享限额/取消关闭公开函数源码，全部未执行 |
| `crates/eltdx/tests/extended_loopback.rs` | 7个MAC/MAC-EX真实owner入口、取消/关闭/期限/阻塞回调门保留/换源公开函数源码，全部未执行 |
| `crates/eltdx/tests/support/extension_peer.rs` | std-only手写请求/响应及明确合成setup辅助，不作生产登录证据 |

MAC模块已按feature mac在crate root登记；mac依赖ex，JSON使用现有optional
serde_json，未引入第二网络/解压栈。三组原path包含用例已改为公开eltdx::mac
导入。登记和用例源码不等于编译/运行通过，不能据此宣称独立入口已可用。

## MAC全部23个方法的归属

全部query已写。22/23方法已有独立client/profile接线；download_file组合仍待实现。
Quote/Bar/Transaction/BoardInfo/Unusual带逐响应共享meta；PageResult带聚合meta；
其他返回的最近响应meta可通过diagnostics.latest_meta读取。全部未验证。

| 方法 | 已有基础 | 尚待工作 |
|---|---|---|
| quotes | 0x122B bitmap/rows及1..80 checked query/owner/meta/raw | 最终统一验证 |
| quote_list | 0x122C rows、Category/SortType/FilterMask、80分页、board alias | 最终统一验证 |
| bars | 0x122E四flag/36B记录、u32目标/700分页、秒级时间/整页前插/raw映射及QFQ修复 | 最终统一验证 |
| tick_chart | 0x122D18B tick +120B tail、checked id/date/owner | 最终统一验证 |
| tick_charts | 0x123E14B tick/5日期/短首日、checked days/date/owner | 最终统一验证 |
| chart_sampling | 0x254D count/float bits、owner/meta | 最终统一验证 |
| transactions | 0x122F18B记录、u32目标/1000分页、富Transaction/PageResult | 最终统一验证 |
| symbol_info | 0x122A194B已知字段及tail、checked id/时间/单位/owner | 最终统一验证 |
| boards | 0x1231配对160B记录、BoardType/分页；只以空页证明结束 | 最终统一验证 |
| board_members | 0x122C bitmap/rows、BoardSymbol/80分页；Instrument alias | 最终统一验证 |
| belonging_boards | 0x1218 flag1/27B prefix/GBK JSON、<=8B代码/owner | 最终统一验证 |
| board_summary | 基础members/fields及All目标/全成员聚合、缺失覆盖/未知槽/单位 | 最终统一验证 |
| board_ranking | 完整候选目录、所有金额/成交量汇总后稳定排序；None全候选/未知排后 | 最终统一验证 |
| board_change_ranking | 完整N个不同日期间隔、向旧分页、重复/缺历史证据及稳定排序 | 最终统一验证；T27保留旧短窗口 |
| capital_flow | 0x1218 flag2独立双表/Presence、checked请求/owner | 最终统一验证，不能套Python默认0 |
| auction | 0x123D16B有符号unmatched、checked id/固定500请求 | 最终统一验证 |
| unusual | 0x1237记录/文本suffix、单页min目标/页限额/600，未满目标Unknown | 最终统一验证，无隐式EOF |
| server_info | 0x120F日期/window/参数/tail、有效today探活/diagnostic | 最终统一验证 |
| kline_offsets | 0x124A混合端序、query/owner/meta | 最终统一验证；returned不是EOF |
| file_meta | 0x1215/70B名称/41B结果、query/owner | 最终统一验证；hash算法未知 |
| download_chunk | 0x1217/8B prefix、index>=1/size/query/owner | 最终统一验证 |
| download_file | 基础meta/chunk | T26固定同源/完整性/上限/EOF组合 |
| goods | 0x2562/48B记录、checked count<=1000/PageResult/未知全局总数 | 最终统一验证 |

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
| bars | 共享0x122E/700页collector/富Bar已接通；T26复权处理 |
| tick_chart | 共享0x122D |
| chart_sampling | 共享0x254D |
| transactions | 已接通港股EX23FC/2406既有解码及富Transaction，其余MAC122F；新session必须login |
| all_transactions | 已接通港股1800分页、50页/90000或更小显式上限仍满报LimitExceeded |

MAC-EX的quotes/tick_chart/chart_sampling/bars/transactions/all_transactions已接入；
其余七项仍是待实现组合。bars明确按700页collect，旧Python单请求由T27保留。
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
   纯证据策略/tracker已写：默认要求全部setup回复；显式optional只在已声明的
   有序fence确认后结束。缺生产login规则时Protocol/Handshake拒绝，而不是
   误报成功。这仍不是可交付的默认登录方案，最终允许的证据阶段必须补齐。
3. 独立MacClient/MacExClient、lazy builder/管理/探测/关闭和首批简单方法已写；
   一个操作沿用同一授权/期限/流量预算。共享探测调度支持8个worker和TCP-only。
   MAC DataReady要求server_info的today有效；MAC-EX要求独立login及market38一行
   goods。后者仅证明该样例命令可用，不代表全部市场/业务或生产默认login已验收。
4. 分页/公开Bar和Transaction/HK路由/raw合并来源已写，19个公开client函数源码
   覆盖取消/半包/close/failover等，均未执行。fields/util/advanced辅助接口源码
   已补；T26首段已接显式provider及parent上下文的网络QFQ编排，尚未执行验收。
   继续余下简单入口覆盖与源码收尾；生产握手缺口仍须在最终证据阶段解决。
5. T26继续目录/fallback/缓存、聚合/排行、同源下载及余下公开入口；T27兼容迁移。
6. T28–T30消费者、打包、文档全部源码完成后统一验证，并解决实际编译/平台问题。

详细恢复见`.superpowers/sdd/development-plan/task-25-report.md` Stage B checkpoint6，
以及`.superpowers/sdd/development-plan/task-26-report.md`第一段。
实现源码完整不等于执行验收通过，当前任务总数23完成源码/3部分/4未开始。
