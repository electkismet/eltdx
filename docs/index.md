---
title: 接口目录
hide:
  - navigation
  - toc
---

<section class="catalog-app" data-interface-catalog aria-label="eltdx 接口目录">
  <aside class="catalog-sidebar" aria-label="接口目录导航">
    <div class="catalog-sidebar-heading">
      <span class="catalog-sidebar-icon" aria-hidden="true">&lt;/&gt;</span>
      <strong>接口目录</strong>
    </div>
    <nav class="catalog-view-switch catalog-language-switch" aria-label="SDK 语言">
      <a href="./" data-catalog-language="python" aria-current="page" lang="en">Python</a>
      <a href="./?lang=rust" data-catalog-language="rust" lang="en">Rust</a>
    </nav>
    <div class="catalog-view-switch" role="group" aria-label="目录分类方式">
      <button type="button" data-catalog-view="function" aria-pressed="true">按功能</button>
      <button type="button" data-catalog-view="interface" aria-pressed="false">按接口</button>
    </div>
    <nav class="catalog-tree" data-interface-tree aria-label="接口层级"></nav>
    <nav class="catalog-reference-links" aria-label="参考文档" data-catalog-language-content="python">
      <span>参考文档</span>
      <a href="rust-sdk/">Rust SDK 与原生工具</a>
      <a href="EXTENDED_MARKET/">扩展行情 / MAC / 基金</a>
      <a href="OFFLINE/">本地行情文件</a>
      <a href="FINANCIAL/">专业财报</a>
      <a href="CLI/">命令行</a>
      <a href="METHOD_REFERENCE/">调用方法</a>
      <a href="FIELD_REFERENCE/">字段手册</a>
      <a href="COMMANDS_7709/">命令映射</a>
    </nav>
    <nav class="catalog-reference-links" aria-label="参考文档" data-catalog-language-content="rust" hidden>
      <span>参考文档</span>
      <a href="rust-sdk/">Rust SDK 安装与示例</a>
      <a href="https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/">完整 Rust API 参考</a>
      <a href="rust-sdk/local-files-and-reports/">离线文件与专业财报</a>
    </nav>
  </aside>

  <div class="catalog-main">
    <header class="catalog-heading">
      <p class="catalog-kicker">接口目录</p>
      <h1 data-interface-heading>接口文档</h1>
      <p data-interface-lead>默认按功能浏览，涵盖 7709 行情、7615 F10、Helpers、扩展行情（7727）、MAC 和 MAC-EX 基金与外盘。也可以切换到按接口浏览。</p>
    </header>

    <div class="interface-stats" data-interface-stats aria-label="目录分类统计"></div>

    <div class="interface-controls">
      <div class="interface-control-field interface-search-field">
        <label for="interface-search-input">搜索</label>
        <div class="interface-search">
          <input id="interface-search-input" type="search" data-interface-search autocomplete="off" placeholder="搜索接口、方法、命令号或 Entry">
        </div>
      </div>
      <label class="interface-control-field interface-scope-select">
        <span>目录</span>
        <select data-interface-scope-select aria-label="选择接口目录"></select>
      </label>
    </div>

    <p class="interface-result-meta"><strong data-interface-result-count aria-live="polite">0</strong><span> 项结果</span></p>

    <div class="interface-table" role="table" aria-label="eltdx 接口目录">
      <div class="interface-table-head" role="row">
        <span role="columnheader">接口 / 调用</span>
        <span role="columnheader">来源 / 目录</span>
        <span role="columnheader">协议 / 类型</span>
        <span role="columnheader">说明</span>
        <span role="columnheader">返回 / 文档</span>
      </div>
      <div class="interface-table-body" data-interface-rows role="rowgroup"></div>
      <p class="interface-empty" data-interface-empty hidden>没有匹配的接口。</p>
    </div>

    <noscript><p class="interface-empty">接口目录需要浏览器启用 JavaScript；其余文档仍可直接阅读。</p></noscript>

    <section class="catalog-scope" aria-labelledby="catalog-scope-title" data-catalog-language-content="python">
      <h2 id="catalog-scope-title">统计口径</h2>
      <p><code>7709 原生协议接口</code> 收录 26 项业务能力，按业务分类展示。</p>
      <p><code>7615 原生 Entry 接口</code> 将通用 Entry、23 个功能调用和 1 个静态资讯频道枚举平铺为 25 项；网络调用使用 HTTP POST 与 JSON，静态枚举不发网络请求。</p>
      <p><code>Helpers 封装</code> 收录组合协议调用、分页、拆批、内容解析、本地整理和计算；短线指标可返回开盘量比、流通股本、封单和连板字段。</p>
      <p><code>扩展行情 / MAC / MAC-EX</code> 提供独立客户端，覆盖扩展标的、基金净值、外盘、报价、K 线、分时和成交；服务端支持范围见各接口文档。</p>
      <p><code>MCP</code> 是面向 Agent 的工具服务，保留独立文档，但不计入接口目录分类。</p>
      <p>v3.3.2 新增市场资讯方法及批量缺失处理，详见 <a href="releases/v3.3.2/">发布说明</a>和 <a href="BATCH_MISSING/">批量缺失处理</a>。</p>
      <p>目录数据随静态页面发布，打开后不会连接行情主站、F10 网关或任何 eltdx 后台服务。</p>
    </section>
    <section class="catalog-scope" aria-labelledby="rust-catalog-scope-title" data-catalog-language-content="rust" hidden>
      <h2 id="rust-catalog-scope-title">Rust API 收录范围</h2>
      <p>Rust <code>3.4.0-beta.4</code> 与 Python 目录使用相同的业务项。同一项下可能有多个方法，或与其他项共用一个方法。完整类型、构造器和其他方法见 <a href="https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/">Rust API 参考</a>。</p>
      <p>默认支持 7709 行情。各项所需的 Cargo feature 显示在「协议 / 类型」列。安装方式和客户端示例见 <a href="rust-sdk/">Rust SDK 指南</a>。</p>
      <p><a href="https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/offline/">离线文件接口</a>和<a href="https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/financial/">专业财报服务</a>可在 API 参考中单独浏览。</p>
    </section>
  </div>
</section>
