# Rust SDK 实施进度

> 本文件是跨会话恢复点。每个阶段和提交后更新状态、当前任务、下一步及未执行验证；重开任务时先读本文件，再读对应实现计划和分域规格。

## 当前状态

2026-11-17第四十三段恢复点：历史资金流检查点`da1953f`。本段补独立字面量F10请求清单（各域默认、报告/基金/market分区、频道/cache/valuation分支），修正TopicCompare必填ID/首个可用主题及FundReport分区切换的日期窗口校验，新增相关route/result源码。所有用例未编译/运行，不能据清单宣称157入口全分支验收完成。19完整源码/5部分/6未开始，继续T20 typed结果/分页及分支收尾，再T22–T30；active无额度限制，全部执行验证延期。

2026-11-17第四十二段恢复点：字段字典已提交`e8392a5`。本段新增F10独立历史资金流service/query/window/raw-backed行及结果，history/range一次HTTP请求且共用原owner/期限/预算。按COMP-050仅reverse源行再start/count截取，修正开发清单“升序”误写；保留业务码、附加表、真实raw、未知列/单位及非法日期/数值诊断。五组route/result用例源码已写（含正常/乱序、空/越界、days7零请求、共享关闭），均未执行。19完整源码/5部分/6未开始不变；T20还需157入口全部分支逐项核对、typed结果收尾，随后T22/T23及全部后续任务。目标active、无额度限制；所有测试/构建/依赖/网络/执行验证继续延期，无代理/发布。

2026-11-17第四十一段恢复点：梯队/预算已提交`2a9e05d`，恢复记录`fba8a77`。T20完整字段源的首版Rust接入已写：fields/data.rs以PowerShell只读字面量转写两个Python字段文件，不导入/执行Python，不运行验证；fields.rs保留所有财报模板继承覆盖、MG/HK scope索引/通配、两类现金流notes、显式nhytype/reporttype上下文、未知字段与原始重复列。银行现金流T088仅note，投影后缀有独立入口；未把label字典升级为已确认单位。新增三组字段语义用例源码，全部未执行。仍19完整源码/5部分/6未开始；下一步历史资金流、157入口/全部分支route与typed结果收尾，再继续T22–T30。目标active无额度上限，全部执行验证延期。

2026-11-17状态核对：第四十段源码已提交`2a9e05d`，首批endpoint为`5a4185c`。19项完整源码/5项部分/6项未开始；尚未进入统一执行验证。接续先做T20完整fields registry，已阅读Python两个字段源文件全部内容；`fields.py`的四类财报模板、两个现金流notes及`fields_mg_hk.py`的完整scope路由必须保留继承/覆盖次序。银行现金流T088没有统一label，不能从资产负债表的同名T088移植。随后历史资金流、157具名入口全分支route/result证据、T22/T23和后续扩展/发行阶段。目标active无额度上限，禁止任何提前执行验证。

2026-11-17第四十段恢复点：T20首批入口已提交`5a4185c`。继续补F10操作级64MiB逻辑字节限额，复用原OperationSponsor预算，在解码/保留raw前累计；分页投影留在原期限内，prerequisite合并全部来源/raw，业务/基金报告期只取首行并验证日期。新增两个涨跌停具名入口、原始字段模型、按日板位统计、0..1晋级率及注入式本机日期clock；chrono0.4.42只声明为暂定依赖，未解析验证。新增f10_routes/f10_results及HTTP多请求/限额源码用例，全部未执行。T20仍需完整fields registry、历史资金流、157入口全分支用例和模型细节收尾；19源码完成/5部分/6未开始，目标active无额度限制，全部执行验证延期。

2026-11-17第三十九段恢复点：最新已提交为`a0893db`。T20已开始：新增独立身份/分页/日期值类型、具名query/endpoint层，美股/港股/三板、债券、行业/期货、公司/基金报告、新闻频道/缓存、估值/主题及市场分页首批源码。157入口尚未全部完成；涨跌停梯队、完整fields registry、历史资金流和完整路由/结果源码用例仍待补。市场collector及prerequisite的原始数据/取消/总期限边界正在收尾。当前19项完整源码/5项部分（T19/T20/T22/T24/T25）/6项未开始；此统计仅为源码进度，所有执行验收未开始。目标active无额度限制；继续禁止测试、collection、build/check/clippy、依赖解析/下载、网络/AST/格式/diff检查，全部T01–T30源码完成后统一执行。恢复优先读task-20-report.md。

2026-11-17第三十八段恢复点：HTTP/DNS/TLS已提交`6196df2`。继续T19源码边界复查，补zlib完整StreamEnd/尾随字节拒绝、共享connect串行门及未确认清理时保留门租约、Params整文档node/depth/byte上限、响应/请求保留副本的可取消遍历，以及HitCache/未知顶层字段保真。补并发connect、等待者取消、gzip拼接/截断、redirect和保留副本源码用例，全部未执行。T19仍部分，19源码完成/4部分/7未开始；下一步完成T19源码交接，进入T20全部157入口。目标active无额度限制，测试/构建/依赖解析/网络验证全部延期。

2026-11-17第三十七段恢复点：F10 owner已提交`545dd8b`。HTTP/DNS/TLS/worker及connect/call/params已接入源码；共用原预算、4 worker上限、取消/关闭确认、IPv4/IPv6、显式CONNECT、header/压缩/解压限额与raw已写，公共HTTP/TLS/故障用例源码已补。依赖全部暂定，未resolve/build/test；T19仍部分，需继续源码边界复查，再做T20完整157入口。19/4/7不变。读task-19-report第三段，目标active无额度限制。

2026-11-17第三十六段恢复点：T19值层已提交`8a339c0`。新增F10独立惰性owner/client/builder/diagnostics，完整with_*配置、header覆盖顺序与共享关闭；现有RequestPermit新增内部授权转移，供HTTP复用同一预算。新增相关源码用例均未执行。HTTP/DNS/TLS和connect/call/params尚待接线，T19仍部分，19/4/7。恢复读task-19-report第二段继续，active无额度限制。

2026-11-17第三十五段恢复点：T18 HQ源码收尾已提交`515a13c`。T19已开始，新增F10 config/json/request/response、feature声明及未执行用例源码；精确数字/重复键/多表模型和路由默认值已写。HTTP/TLS/DNS传输、builder/client及完整故障fixture仍待实现；不把值层当作T19完成。当前19源码完成/4部分（T19/T22/T24/T25）/7未开始。恢复读task-19-report.md继续HTTP foundation；全部测试/构建/依赖解析/网络/执行检查仍延期，active无额度限制。

2026-11-17第三十四段源码恢复点：订阅编排已提交`8051295`。installed-wheel consumer、block-meta响应/错误及helper forwarding用例源码已写，六项manifest源映射、M93–M95补充证据和M01–M100实现索引已补齐。T18当前规格的源码范围完成，所有验收仍延期；总数更新为19源码完成/3部分/8未开始。下一步T19 HTTP/F10基础。旧P2标签中的theme-strength/topics/topic-stocks依赖F10，明确归T23，不计入本次HQ源码完成。下方旧18/4/8记录仅为历史。目标active无额度限制，无任何测试/构建/检查/网络执行。

2026-11-17 第三十三段源码恢复点：批量编排已提交`d49089b`。订阅pin分组/激活合并/游标记忆/续期调度/派发/单轮及持续轮询循环已接入Rust binding；Python保留except/finally、生命周期和custom hook，保证on_error中的活动异常、refresh lookup失败顺序、清状态后关pin，复用T15唯一push消费。新增19合同函数源码（含参数化），全未执行。下一步manifest全量证据/installed-wheel consumer，然后T19–T30/Stage B；18/4/8不变，active无额度限制，禁止执行验证不变。

2026-11-17 第三十二段源码恢复点：板块阶段已提交`1b49adc`。资本、复权、资金流、普通/退市K线、分钟线和逐笔的批量切片/遍历/结果拼装已接入Rust binding。保留Python executor context和自定义worker/函数覆盖、短zip/重复key语义；分钟线一次提交全部，其他既有域保持逐worker分组。资金流自定义结果在with内返回，正常batch在with退出后构造。新增15个合同函数源码（含参数化），均未执行；更新3项source-only manifest。下一步subscription编排/完整manifest/installed-wheel consumer及全部后续任务；18/4/8不变，active无额度限制，所有执行验证延期。

2026-11-17 第三十一段源码恢复点：竞价/市场组合已提交`43028a4`。板块全部业务入口/准备/缓存/目录/文本与DBF解析/证券列表/成员分类/快照批次/报价构造已接入Rust binding；保留Python Path/codec/regex/JSON/hash调用、原缓存先发布顺序、末尾header计数规则、自定义批次和错误范围。新增17 Python合同函数源码，未执行。T18剩余批量/订阅编排、manifest全量证据与installed-wheel consumer；T19–T30尚按原范围继续，18/4/8不变，active无额度限制，全部验证延期。

2026-11-17 第三十段源码恢复点：每日涨跌停已提交`eaecc51`。竞价汇总/当前日期/首行情/80分批、全市场统计、当日资金流取数和Helper停牌列表已迁入Rust绑定，共用百分比与乘积计算。保留历史首正式撮合/price_base、显式前收盘优先、原对象/round/异常、Python市场统计旧positional语义；新增13 Python/1 Rust合同函数源码，均未运行。T18仍部分18/4/8；剩余板块、批量、订阅、manifest/consumer和T19–T30全部继续，目标active无额度限制。

2026-11-17 第二十九段源码恢复点：短线limit/日期/天梯已提交`0725189`。每日涨跌停高层、旧市场比例/规则、新股五日判断、前收盘价行选择、当日股本事件筛选和最终行构造已接入Rust绑定，逆向除权算术与strict共用；保留Python末条合格K线、查询/解析异常范围、事件顺序/两次c4读取和旧10/20/30比例，查询线程调度仍为Python adapter。新增13 Python/1 Rust合同函数源码，均未运行。T18仍部分18/4/8；继续auction/market/boards/批量/订阅/consumer，全部执行验证延期。

2026-11-17 第二十八段源码恢复点：短线source/指标已提交`9f66521`。本段完成短线limit/连板/天梯与握手日期、row date和by-code绑定，strict/legacy共用lazy分类、日期等级和auction readiness；保留ST优先旧规则、NaN/round/tolerance、原对象与排序/参数校验顺序、精确日期错误和custom hook。新增10 Python/1 Rust合同函数源码，全未执行；短线业务入口已接通，T18其余daily limits/auction/market/boards/批量/订阅/consumer仍未完成。任务计数保持18完整源码/4部分/8未开始。目标active，无额度限制，所有测试/构建/检查继续延期。

2026-11-17 第二十七段源码恢复点：短线日期/缓存已提交`6580450`。短线get组合、证券/财务/前五日K线/开盘抢筹source循环与指标构造已迁入Rust绑定，乘积/比值共用Rust，保留原顺序/optional请求边界/小样本不扫全市场/round6/自定义hook和原对象。新增10合同函数源码，全部未执行。剩余短线限价/连板/握手日期和天梯等尚待迁移；T18部分18/4/8不变，继续全部既定范围，active无额度限制。

2026-11-17 第二十六段源码恢复点：profile/每日股本已提交`1989d3c`。短线统计主日期计数/并列取较晚日期、两表校验及缓存发布、前交易日stat2字段换位接入共用Rust/绑定，保留Python错误顺序/格式/原因、原字段对象/二次读取和锁。新增8合同/2Rust函数源码及短线部分source-only证据，全未执行。短线高层source/数值/limit算法尚未回迁完；T18仍部分18/4/8。下一步继续短线及剩余helper/批量/订阅/consumer，active无额度限制。

2026-11-17 第二十五段源码恢复点：排行已提交`73436f2`。profile、证券/财务map缓存与每日股本的Python组合循环迁到Rust绑定；共用80分批、换手率/乘积及来源标签，保留ordered tuple缓存失败/部分结果、原对象/字段/自定义hook、统计主日期和两次属性读取。新增11合同/1Rust函数源码和2项source-only证据，全部未执行。T18部分、18/4/8不变；继续短线/每日限价/market/auction/boards与订阅/批量/consumer。目标active无额度限制。

2026-11-17 第二十四段源码恢复点：full_quotes/资金流helper已提交`2c6d9f4`；排行共用分页循环与Python native投影已写，保留短页EOF/超count整页/连续rank/原记录/每页名称及custom count路径。新增5合同/2Rust源码函数及排行source-only清单，均未执行；总体仍18完整源码/4部分/8未开始，T18仍部分。继续其余helper、批量/订阅和consumer；active无额度限制。

2026-11-17 第二十三段源码恢复点：special limits已提交`9d75306`。本段把full_quotes的80分组、五档资格与Python合并循环、逐笔资金流档位/累计接入Rust；保留Python重复depth最后项、四字段替换、custom对象/回调、optional Exception降级及原非finite/负值算术。新增10合同函数源码及full_quotes source-only manifest；全部未执行。T18仍部分，18/4/8不变；继续其余helper、批量/订阅及consumer，再推进T19–T30。目标active，无额度限制。

2026-11-17 第二十二段源码恢复点：stats已提交`c2ef123`。special limits单页/扫描共用Rust请求/offset循环及native接入已写，保留每请求期限、未知identity/nonfinite/raw、空页和最后页超目标数、偏移提取错误；custom算术/回调保留adapter。新增6合同/4本机集成/2Rust函数源码，均未执行。health已核对为现有共用session heartbeat及零native-load ping adapter，补两类ping源码用例。总数18/4/8不变；下一步全部HQ helper/订阅/批量调度、manifest和installed-wheel consumer，目标active无额度限制。

2026-11-17 第二十一段源码恢复点：resources已提交`4b0d38b`。stats GBK/文本/数字/行解析接入共用Rust函数，legacy保留任意精度整数、原顺序/重复键/空表错误、Python ZIP codec与异常cause和自定义parser adapter。新增8合同/2Rust函数源码及1项source-only manifest，全未执行。下一步补齐special limits、health边界及剩余批量/helper/订阅/manifest/consumer；T18仍部分，总数18/4/8，active无额度限制。

2026-11-17 第二十段源码恢复点：calendar已提交`b7ae0d1`。resources read/block_meta及标准download_file接入共用Rust请求边界和DownloadCursor；保留原pin enter/exit（含cap0）、逐请求预算、截断/EOF、raw和换源错误。custom read/execute回旧adapter。新增8合同/2本机集成/1Rust函数源码及两项source-only manifest，全部未执行。stats解析/health和剩余T18仍待做，总数18/4/8不变；active无额度限制。

2026-11-17 第十九段源码恢复点：equity已提交`fc3088e`。calendar共享有序日期去重/区间/邻居/包含查询及Python weekday近似已写；WorkdayService保留原基准bars加载/缓存与独立_day_set、弱引用失效、无覆盖时False/None、日期越界和custom日期/容器adapter。新增9合同/2Rust函数源码，均未执行；日历不在124 capability记录内，按模块/报告记录。T18仍部分，总数18/4/8不变；下一步resources/stats/health及剩余批量/helper/订阅/manifest/consumer，active无额度限制。

2026-11-17 第十八段源码恢复点：corporate资本/财务已提交`851774a`。equity标准事件的前后复权接入共用Rust affine排序/同日原序/组合循环；strict保留有限正值检查，Python保留非finite/日期/非正乘数错误及custom记录/函数覆盖adapter。新增9合同/3Rust函数源码和1项source-only manifest，全部未执行。T18仍部分，总数18/4/8不变。下一步calendar/resources/stats/health及批量/helper/订阅/manifest/consumer；active无额度限制。

2026-11-17 第十七段源码恢复点：daily money flow已提交`6b4fc0b`。corporate资本变迁/财务共用请求边界、资本prefix cursor、财务二分恢复树和Python native接入已写；保留原32补查预算、首次服务器序/恢复后重复项队列、raw/非finite/日期/异常、字段别名及属性两次读取。新增15合同/7本机集成/3Rust函数源码和两项source-only manifest；全部未执行。外层资本chunk调度仍是adapter，equity未开始修改。T18仍部分，总计18源码完成/4部分/8未开始；下一步equity/calendar/resources/stats/health及剩余helper/订阅/manifest/consumer，active无额度限制。

2026-11-17 第十六段源码恢复点：竞价已提交`fa7e07d`，恢复记录`353336d`。daily money flow共用Rust请求边界和备用源循环源码已写，保留原3/24循环预算（含skip扣额）、cursor规则、selected错误传播、新源TransportError降级、factory/adopt/close和custom query/execute；原无目录预取、unknown/multiple block、非finite/date/raw行为保留。新增12合同/6本机集成/3Rust函数及1项source-only manifest，全部未执行。batch调度和F10 history仍待后续任务；T18仍部分，总计18源码完成/4部分/8未开始。下一步corporate/equity/calendar/resources/stats/health及helper/订阅/manifest/consumer；active无额度限制。

2026-11-17 第十五段源码恢复点：逐笔阶段已提交`6544772`，竞价阶段已提交`fa7e07d`。竞价共用Rust请求/响应边界及Python/native接入源码已写，保留原专用连接池延迟创建/关闭重建/主机配置/弱回调、custom execute扩展、raw与旧time/count行为且不预取目录。新增5个合同/4个本机集成函数、1项source-only manifest；全部未执行。T18仍部分，总体18源码完成/4部分/8未开始。下一步daily money flow及其余HQ/helper/订阅/manifest/consumer；目标active无额度限制。

2026-11-17 第十四段源码恢复点：上一阶段行情已提交`d3c1215`。逐笔today/history单页、全量分页、历史bulk和正式开盘撮合的Python/native共用源码已写；保留原逐请求预算、无预取目录、重复/超count页、页内顺序、首base/raw/日期与上限异常；bulk保留原分块及空终页，不提前构建Python逐笔对象。自定义参数和instance/class分页替换保留原逐页adapter，today bulk继续原拒绝。新增14个合同/12个本机集成/3个Rust函数源码，四项manifest仅增source-only证据。全部未执行，T18仍部分；总体18源码完成/4部分/8未开始。下一步auctions/dailyflow及剩余HQ/helper/订阅迁移，完整manifest和installed-wheel consumer。目标active，无额度限制。

2026-11-17 第十三段源码恢复点：目录阶段已提交`6fd1e05`。旧版行情/分类排行/刷新、CodeApi停牌筛选和标准订阅group的固定连接刷新native源码已写；共用Rust域边界/停牌位规则，保留原精度registry、负值/重复/原始字节、resolver次序、无resolver不预取，以及先预取再占pin。新增9个合同/9个本机差分集成函数、三项manifest source-only证据；全部未执行。T18仍部分，总体18源码完成/4部分/8未开始；下一步逐笔/竞价/资金流等剩余HQ及helper/订阅编排回迁。目标active无额度限制。

2026-11-17 第十二段源码恢复点：session/company已提交`2cbcd53`。目录count/list、股票/A股/ETF/指数/ST筛选及统计的native接入源码已写，共用Rust域边界/纯筛选并返回原Python行；保留limit=0请求、原count当天日期、旧精度registry及后行失败前行副作用、Unicode ST、跨市场顺序、自定义字段/异常/非bool求和行为。新增11个合同/7个本机差分集成函数及1个Rust用例源码，两项manifest仅增source-only证据。全部未执行，T18仍部分；下一步其余quotes/latest_suspended和其他HQ/native域、完整manifest与installed-wheel consumer。总体18源码完成/4部分/8未开始；目标active无额度限制。

2026-11-17 第十一段源码恢复点：分钟线已提交`ac99b99`，恢复记录`93cab12`；session握手/心跳及HQ公司目录/内容的Python/native接入已写，共用Rust域请求/解码边界，保留原raw/snapshot、日期、GBK、显式市场与字节范围行为。新增7个合同/4个本机差分集成函数源码、四项manifest source-only证据；local ping保留不加载native的adapter。全部未执行，T18仍部分；下一步目录count/list和筛选统计，再推进其余HQ域。总体18源码完成/4部分/8未开始，目标active无额度限制。

2026-11-17 第十段源码恢复点：普通/退市K线已提交`b5ed0b5`；分钟线today/history/recent/aux/sparkline五命令Python/native接入源码已写，共用请求/解码边界并保留原scalar/负volume、raw hex、resolver顺序、原输入市场推断、默认日期和批量workers。补10个合同及7个本机差分/集成函数源码、五项manifest source-only证据，同时修正上一段K线record_hex恢复。全部尚未编译/collection/执行，T18仍部分，统计18源码完成/4部分/8未开始；继续其他HQ native域、manifest全量证据及installed-wheel consumer，再做T19–T30/剩余Stage B。目标active且没有额度限制。

2026-11-17 第九段源码恢复点：Rust例子/consumer已提交`39c8e3e`。普通/退市K线首批Python/native回迁源码已写：共享分页与排序循环、legacy单请求预算、不pin整段历史、首响应metadata/raw/异常/当天回绕、批量归一并发和custom对象adapter；新增合同/本机差分用例源码并在manifest保留source-only证据。尚未编译/collection/执行，不能认定迁移通过；T18仍部分，下一域分钟线及其余HQ业务/manifest总核对/installed-wheel consumer。任务总数仍18源码完成/4部分/8未开始，目标active且无额度限制。

2026-11-17 最新恢复点：M90/M91已提交f6a99a8；M96/M97板块及默认HQ内部文本/DBF解析、显式来源/固定HQ下载、owner缓存和8组公开用例源码已提交1bb3c1a。M79–M97共19/19 helper方法均已有首批源码。随后新增hq_query例子和tests/consumers/rust-sdk独立manifest、仅std/eltdx的非空回环/分页/关闭consumer源码。T18仍部分，全部HQ Python/native回迁、真实manifest迁移证据及安装wheel consumer仍待完成；外部复制/依赖图/编译运行证据也尚无。当前18项源码完整、4项部分、8项未开始；没有运行任何测试、构建或执行检查。

当前实际进度：T01–T17及T21实现源码已写完；T18/T22/T24/T25为部分实现（18完整源码/4部分/8未开始）。恢复先读task-18-report第七/八段和brief，继续HQ Python/native逐域回迁、真实manifest证据和installed-wheel consumer。M79–M97及Rust首批例子/consumer源码已写，不重复实现；其源码完整度不代表T18完成或已通过编译/测试。目标保持active且不设置额度限制；全部执行验证延期。

2026-11-16 第二轮 T12 检查点已提交 `1e25507`：新增 NativeEngine loopback 成功链、协议异常 `__cause__`、KeyboardInterrupt 清理的集成测试源码；`from_sdk` 保持旧 Python native 异常无额外 cause；Rust 与 Python legacy directory 共用分页循环但分开应用严格响应校验，保留 Python 旧行集语义。实现和测试源码均未运行；T12 尚未完成。

2026-11-16 第三轮 T12 兼容审阅：普通、数量正确的首个快照响应保留服务器行序；发生恢复时，按 Python facade 的代码队列重组，仅把明确空单项列为 missing，并忽略恢复响应中未匹配的行。resolver 的结果投影先于 MissingCodesError/skip warning。新增相应合同及 loopback 测试源码；未执行测试、构建或验证。

| 项目 | 值 |
|---|---|
| 实施分支 | `codex/rust-sdk-implementation` |
| 起始基线 | `af471f1adf38757f6aa0188968db8110eb23f6ad` |
| 规格检查点 | `e36c71e` (`docs(rust-sdk): checkpoint implementation specification`) |
| 最新检查点 | F10 HTTP/DNS/TLS `6196df2`；第三十八段传输/取消/保留副本边界补强源码已写；全部执行验证延期 |
| 当前阶段 | T01–T18及T21源码完成；T19/T22/T24/T25部分；19完整源码/4部分/7未开始。 |
| 已提交进度 | T01–T10 实现与静态审阅完成；T11 检查点 `6f5fadb`、`de9e74f`、`a38dc32`、`ddb5e89`、`73c9365`、`f6ff2b3`、`c0f005d`；T17 `cd6735b`/`bcd6a8f`；T21 `b6e4beb`；T22 `9b6c8fd`/`a64a4a6`；T24 `1220bf2`/`04f613a`；T25 `c87e5b0`/`78bf121`；全部验证延期 |
| 测试约束 | 不运行 pytest 或 collection；不运行 cargo test/build/check/clippy/doctest、网络验证、AST 编译脚本或 `git diff --check` |
| 下一步 | T19 HTTP/F10基础，继续全部T20–T30及Stage B；HQ索引见hq-implementation-index.md，最终统一验证 |

## 阶段进度

| 阶段 | 任务 | 实现状态 | 测试状态 |
|---|---|---|---|
| P0 | T01–T03基线、样本、兼容决策 | T01–T03实现与契约源码完成，最终验证延期；涨跌停规则profile因权威证据缺失保持Unknown | 按用户要求暂不运行 |
| P1 | T04–T10 SDK基础、runtime、metadata | T04–T09 静态审阅完成；T10 Stage B 与 fix1 已通过限定静态复审，最终验证延期 | 按用户要求暂不运行 |
| P2 | T11–T18 HQ与Python首批迁移 | T11–T18实现源码完成；HQ桥/证据/consumer源码具备；兼容和可用版本验收全部待最终执行 | 按用户要求暂不运行 |
| P3 | T19–T23 F10、offline、financial | T21 静态审阅批准；T22 Unicode修复 `a64a4a6` 通过限定复审，ZIP/资源集成待做 | 按用户要求暂不运行 |
| P4 | T24–T27 EX、MAC、Python迁移 | T24 Stage A/fix 已批；T25 pure codec/model Stage A `c87e5b0` 已写，完整服务与响应范围待 Stage B | 按用户要求暂不运行 |
| P5 | T28–T30消费者、打包、完整交付文档 | 未开始 | 待所有实现代码完成后统一执行 |

## 依赖与冲突预审

| 任务/工作线 | 共用文件或接口 | 预审结论与处理 |
|---|---|---|
| T01 ↔ T02 | 迁移manifest、fixture目录、registry覆盖 | T01先冻结ID/schema及源映射；T02只使用该manifest，不重复定义registry来源。 |
| T04 ↔ T07 ↔ T08 | `TdxClient`、ClientConfig、SessionOwner、Error、OperationOptions | 高耦合；公共类型和生命周期由同一主线先实现，业务域不可自造平行类型。 |
| T05 ↔ T06 | runtime engine/request/deadline、pending/pin交接 | 高耦合；pin终态/所有权优先，后新增deadline raw-reply路径；共享代码不得复制Engine执行循环。 |
| T09 ↔ T10 | protocol parser上下文、metadata snapshot、数值投影 | protocol层只定义语言中立解析trait/value；SDK metadata owner与缓存保持在eltdx。 |
| T11 ↔ T18 | batch/query/service、Python HQ facade | T11先定批量和缺失契约；T18按域替换facade，不提前删除未迁移实现。 |
| T12 ↔ T18 ↔ T23 ↔ T27 | native service bridge、Python公开入口 | 每阶段只迁移有Rust域实现及兼容范围的入口；保留既有外观，custom transport单独适配。 |
| T13 ↔ T14 ↔ T15 | 时间、owner、metadata、quote pin与push | 共享类型先冻结；订阅独立dispatcher唯一消费push，不能与旧poll路径双读。 |
| T16 ↔ T18 | corporate/limits/adjustment/calendar/helpers | 未取得涨跌停权威依据的路径返回Unknown；不从旧冲突实现猜定。 |
| T17 ↔ T22 ↔ T23 | HQ resources、offline DAT/ZIP、财报下载 | T17先定完整下载契约和zip实现；T22复用同一decoder；Python直接DAT旧上限保持。 |
| T19 ↔ T20 | F10 HTTP、JSON、query/response types | T19冻结传输层与byte cap；T20只实现入口/解析，不再引第二HTTP栈。 |
| T24 ↔ T26 ↔ T27 | EX session/commands/catalog/Python bridge | T24先提供有界profile；目录/组合服务等session稳定后实现。 |
| T25 ↔ T26 ↔ T27 | MAC bitmap/routing/services/Python bridge | 位图与profile为唯一解析来源；MAC-EX复用MAC模型并独立登录路由。 |
| T28 ↔ T29 ↔ T30 | features/package/consumer/evidence/docs | T28确定consumer矩阵，T29定package metadata，T30汇总实际证据；用户要求的测试延迟覆盖整个实施期。 |

**实施裁定：** 用户要求在全部实现代码完成前不运行测试；测试代码仍按计划逐任务写好，延迟到末尾统一运行。现有规格中的P0样例和默认值为新API设计输入，不表示已有生产协议已验证。联网事实不确定的域实现显式Unknown/Unsupported，不以猜测解锁。用户未授权发布、推tag或合并。

2026-11-16 恢复记录：目标模式保持 active，没有 token 额度限制。T07 已提交 `26488cf`（owner 生命周期）、`abb4285`（操作取消、pin owner 和真实 runtime 并发关闭源码）、`ca372d2`（取消 Condvar 丢失唤醒修复）。限定复审确认操作唤醒与真实 runtime 用例已处理，仍需解决 pin 无界关闭等待与 Drop 提前解除 owner 注册；第二轮修复只沿用现有 owner 列表和 runtime pending close，不增加并行资源状态机。测试/构建/检查均未执行。

第二轮 pin 修复已提交 `0656e63`：新增有界 runtime close 等待，SDK helper 同时遵守固定本地 cap 和 owner 当前期限；owner 进入关闭时唤醒等待者，Drop 不再注销，显式释放成功才注销，保留 runtime source 与结构化上下文。限定复审尚未返回，不能声称 T07 完成。T08/T09 实施者目前仅做源码阅读；T08 尝试授权会放在 worker 真正连接/发送前，T09 解析上下文会贯穿全部嵌套报价/分钟分支，并保存除法前整数。全部实现完成前仍不运行验证。

第二轮复审已返回：以上两个原问题均已处理，但 pin 超时后 Engine 退出会使下一次 pin receipt 不可创建，导致清理状态无法确认。第三轮通过精确 Engine 归属与已确认的 Engine 退出结果核销其 pin，继续返回原失败；资源已回收的事实与错误根因分开记录。未增加第二套 pin 状态机，全部验证仍延期。

第三轮提交 `98e8558` 已获限定复审批准：Engine 确认退出后可以核销其保留 pin，跨 Engine 的同编号 reservation 不会互相删除，原失败原因继续向外返回。T07 实现与静态审阅完成。用户再次要求继续目标模式，当前目标保持 active 且没有额度限制；T08/T09 已获 GO，在分开的文件范围内实施。T10 brief 已保存，T11 及后续代码尚未实施。不把静态审阅当作编译或运行成功证据。

2026-11-16 恢复记录：目标保持 active，`remainingTokens=null`。最新提交 `a64a4a6`。T10 fix round1 `151489f` 已通过限定静态复审，四项原始 Important 均已修复；controller 检查确认 `execute_raw_inner` 在 raw 请求提交前以正常 owner 注册启动 cleanup controller。T11 是下一任务。T22 Unicode 日期数字修复已通过限定复审，但 ZIP/archive 与服务注册未完成。T25 pure codec/model Stage A 已提交 `c87e5b0`、报告 `78bf121`；未注册，部分响应 schema、owner/session/transport 与 T26/T27 组合仍待实现。无测试、构建、check、网络、编译/AST、格式化或 diff 检查执行。

## 恢复规则

最新恢复点（2026-11-16）：goal模式active，`remainingTokens=null`。T17源码检查点88b9036，恢复先读task-17-report最新章节及task-18-brief，进入T18。T13 source_time及raw/日期/cursor约束继续有效；T18必须保留普通/退市raw、HQ同日回绕、native今日逐笔列拒绝和Python原计时/次数，并继续使用T15受控raw reader维持旧推送投影。财务/复权/weekday/特殊限价及资源下载的旧Python投影仍由T18实现，不能直接套新Rust默认或新增来源保护限额。ZIP依赖尚未解析/锁定/验证。没有运行测试、collection、构建、检查、格式化、diff检查或网络验证。

T21 于 `9734db3` 完成35个非财报本地 API、模型/格式算法、GBK 静态查表数据及四组测试源码；所有验证延期。仅运行了经授权的一次 Python 3.12.14 标准库 GBK 映射数据生成，脚本、版本和来源已在实现报告记录。任务审阅包以 `230e566` 为基线，仅含 T21 自有文件，scoped source review 进行中。T10 的 `Diagnostic`/`EnrichmentStatus` 共享实现仍待完成；这不是 T21 已验证集成的证据。

T21 第一轮静态审阅指出 `read_custom` 将空 leaf 当作可跳过数据，违背 brief 的路径名边界；另一个 Minor 是用例同样期望跳过。修复限定为拒绝空 leaf 并保留空 block 名称/missing `.blk` 的独立兼容跳过。原实施者正在从 `9734db3` 进行 fix round 1；之后需对修复 diff 做 scoped re-review。没有运行测试或验证。

1. 以当前实施分支最新提交为准；不切回或直接提交 `main`。
2. 先读本文件、`development-plan.md` 对应任务、相关API规格及 `git status`；保留未提交的用户工作。
3. 只完成计划中有源文件、兼容边界和验收条目的任务；不声称未编译代码可用。
4. 实现可按阶段提交。提交消息用 `feat(rust-sdk): ...`、`fix(runtime): ...` 或 `docs(rust-sdk): ...`，提交后记录短SHA和变动任务。
5. T01–T30所有实现、绑定、打包及示例代码完成后，更新状态为“待统一测试”；再一次性运行计划要求的全套验证，修复后重跑受影响集合，并记录命令、计数、平台与结果。
6. 不实际发布crate、wheel、tag或合并分支；发布仍需独立授权。

## 提交记录

| Commit | 任务 | 内容 |
|---|---|---|
| `e36c71e` | 规格检查点 | 已审阅的架构、接口、计划与兼容规格，以及初始进度文件 |
| `b6c2ea6` | T01 首轮实现 | 修正验证入口并新增迁移清单与基线契约 |
| `5a096aa` | T01 修复 round 1 | 补齐命令注册、源码哈希、pytest 节点与目录映射证据 |
| `c0e2c1c` | T01 修复 round 1 | 区分协议支持与 SDK 迁移状态并增加运行期反射快照 |
| `02de834` | T01 修复 round 2 | 加入反射细节、状态词表与 Python 模块覆盖清单；复审仍有迁移证据和覆盖扩展问题 |
| `edacc482` | T01 修复 round 3 | 加入独立 SDK 证据、可扩展覆盖来源及路径归一化；等待限定复审 |
| `8cc1a0b` | T01 修复 round 4 | 第三轮未通过；增加 crate 公共符号、精确 pytest 节点、三类模块覆盖来源绑定及正反契约源码；所有验证仍按要求禁止 |
| `0178eae` | T01 进度 round 4 | 记录第四轮已提交、复审等待状态和验证禁令 |
| `2eb0618` | T01 修复 round 5 | 收紧 crate 根公开证据、默认 pytest 文件/节点规则、目标测试导入范围及迁移状态关联；未运行任何验证 |
| `6f7cd82` | T01 修复 round 5 | 排除嵌套测试类、构造器赋值、fixture 与禁用节点；补充对应反例契约源码；未运行任何验证 |
| `7026d5d` | T01 进度 round 5 | 记录三项复审发现的静态闭合判断、验证延期及进入 T02 的恢复点 |
| `ea4721c` | T02 样本与证据库存 | 逐例记录 37 个既有 7709 样本来源/哈希、7 个补充案例、期望值证据等级和真实抓包缺口；最终验证延期 |
| `8d11baa` | T03 策略契约 | 冻结涨跌停冲突、身份/精度预取、字段级单位和日期；添加跨域兼容边界样本与契约测试源码；未运行测试、构建、check或diff whitespace检查 |
| `1777e4f` | T04 crate foundation | 新建纯Rust `eltdx` facade、配置/错误/基础类型与惰性 builder；只添加测试源码 |
| `4cb8511` | T04 review fix 1 | 移除受校验类型的直接反序列化、增加时间/价格 getter 与 loopback side-effect 源测试 |
| `eb0935f` | T04 review checkpoint | 记录裸 `127` 歧义裁定、复审 finding 与 no-startup 证据裁定 |
| `4b6a524` | T04 review fix 2 | 修正大整数价格舍入经 `f64` 精度丢失，添加大值/边界/极端 scale 源测试 |
| `c1b8c34` | T04 completion checkpoint | 记录两轮 scoped review 关闭与待最终复审的 `i64::MIN` 覆盖观察 |
| `4bf47d7` | T06 runtime raw/deadline | 增加 RawReply、共同 typed/raw 执行路径、绝对期限传递及 pin/connect deadline；首轮 scoped review 的共享 connect waiter 问题由 `561dcf5` 修复并通过限定复审 |
| `561dcf5` | T06 fix 1 | connect waiter 仅结束自己的等待；共享 attempt 保持运行并在终态后清理；限定复审通过；未运行验证 |
| `d4e3d37` | T06/T07 recovery checkpoint | T06完成，记录T07绝对deadline扩展与cleanup预算裁定 |
| `26488cf` | T07 owner lifecycle | 共享 owner、绝对关闭期限及资源清理责任 |
| `abb4285` / `ca372d2` | T07 fix round 1 | 操作取消/唤醒、pin owner、真实 runtime 并发关闭源码及 Condvar 同步 |
| `0656e63` | T07 fix round 2 | 有界 pin close、Drop 保留注册、原 runtime source 链 |
| `98e8558` | T07 fix round 3 | Engine 确认退出后的 pin 核销及跨 Engine reservation 身份隔离；限定复审批准 |
| `6f5fadb` | T11 checkpoint | QuoteService/CodeService 初始实现与进度恢复记录 |
| `de9e74f` | T11 checkpoint | 恢复路径、服务实现与严格身份处理 |
| `a38dc32` | T11 checkpoint | Python legacy metadata 预取顺序与初始快照请求兼容 |
| `ddb5e89` | T11 tests/checkpoint | 第二 chunk 取消、恢复超时、category 非有限必需值源码用例；同步状态文档 |
| `73c9365` | T11 fix | category continuation 无法表示下一个 offset 时返回 `RequestedWindow` 并省略 cursor |
| `f6ff2b3` | T11 tests | public CodeService 保留未知证券分类为 `Unknown` |
| `c0f005d` | T11 tests | 主快照连接失败仍返回 connection error，不降级成 missing |
| `684a157` | T12 compatibility fix | 保留 Python 快照服务器顺序与恢复语义；统一验证延期 |
| `338d99b` | T13 source implementation | K线/退市K线/分钟18方法，query/DTO、日期/精度/续读/raw映射和合成测试源码；尚未执行验证 |
| `992ba98` | T14 trades | 逐笔10方法、行/列/开盘撮合与源追踪；合成测试源码，执行延期 |
| `ce46437` | T14 completion source | 竞价/日资金流3方法、专属池、owner总连接限制、原子探测预算及合成测试源码；执行延期 |
| `9bbafde` | T15 partial foundation | options/events/gap/cursor/typed queue、runtime独占push consumer及合成测试源码；活动订阅/dispatcher/owner/renewal集成未完成 |

## 阻塞与风险

- 涨跌停权威规则、EX/MAC部分协议字段和真实服务器样本仍按设计文档中的证据缺口处理；未知值不能伪造成正确结果。
- 计划的 Python 业务迁移必须保留当前公开API并按域切换。尚未有native adapter入口前，不先移除Python实现。
- 全流程测试延期意味着各提交在最终测试前均未声明可编译或可运行；代码静态编辑不能作为通过证据。
- 第三轮复审未通过：迁移证据未解析到 crates/eltdx 公共实现，兼容证据未证明可由 pytest 精确收集，模块覆盖来源未建立逐类关联。第四轮只处理这些 T01 契约缺口和状态文档；没有运行任何测试/collection、Rust 验证、网络验证、AST 编译脚本或 diff whitespace 检查。
- 第四轮限定复审未通过（3项 Important）：Rust 符号搜索未排除私有父模块/注释字符串且解析路径未约束在 crate 源目录；pytest 文件规则漏 `.py` 后缀且漏 `__new__`；Python 导入遍历整文件，模块 coverage 未要求有效迁移状态。第5轮针对这些缺口补充静态契约；当前没有 crates/eltdx 实体或 migrated 记录，修复只定义未来证据格式，不提升任何迁移状态。
- 第五轮静态复核：三项发现已在契约实现中覆盖，T01 实现状态关闭；全部测试和构建仍待 T01–T30 实现代码完成后统一执行。没有 Rust SDK crate 或 migrated 记录，尚不宣称 SDK 能力已经实现。
- T02 库存确认既有 7709 样本中 36 个来自合成输入，`delisted_bars/normal` 引用了仓库中缺失的 UI 样例记录；37 个 `expected.json` 均为 Python 回归基线，不是独立 oracle。新增 7 个案例均为合成输入；完整 `0x0FFC` 抓包、非空 category22 和 EX/MAC/F10 实测响应仍是明确缺口。
- T03 未取得有日期的权威涨跌停规则来源，因此六个 Rust 涨跌停案例均保持 `UnknownRule`，而 Python 的 daily/shortline/MAC 三套投影分别保留。T03 policy fixture 是代码审阅/兼容及算术契约，不是监管规则、线上抓包或独立业务事实的证明；全部契约测试源码待 T01–T30 实现完成后统一执行。
- T04 两轮静态 scoped review 已关闭；`Price` 的 `i64::MIN` 缩放边界没有单独 authored case，最终全分支复审需再次检查。serde schema E06 仍未解决，身份/日期/时间仅保留 Serialize，未暴露不安全 Deserialize。

2026-11-16 M23恢复点：已写TdxClient::pin、借用PinnedQuoteService::refresh、同pin冷元数据加载、空wire轮询和失败激活失效处理；depth/refresh共用投影，元数据分页共用scanner。新增pool1冷元数据/空轮询/超100无额外命令/已关闭pin的合成用例源码，均未执行。T15仍部分完成，接续M24 dispatcher/lease/owner/renewal/resync/真实内存计费/Python受控fanout。任务完成计数仍为15完整源码、5部分、10未开始；没有测试或构建成功证据。

2026-11-16 第四段恢复提交：ab5f8bf（26文件），包含M92–M94及共享上下文/源日期/单位/EOF源码用例。前段profile/cache为14f590c。仍为18项源码完整、4部分、8未开始；goal active，无额度限制，尚无任何执行验证。继续M88–M91/M96–M97及完整Python/native回迁；shortline须额外满足server_datetime/09:25/双stat日期门槛，不能把auction日期fallback当ready证明。
