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
    <div class="catalog-view-switch" role="group" aria-label="目录分类方式">
      <button type="button" data-catalog-view="function" aria-pressed="true">按功能</button>
      <button type="button" data-catalog-view="interface" aria-pressed="false">按接口</button>
    </div>
    <nav class="catalog-tree" data-interface-tree aria-label="接口层级"></nav>
    <nav class="catalog-reference-links" aria-label="参考文档">
      <span>参考文档</span>
      <a href="METHOD_REFERENCE/">调用方法</a>
      <a href="FIELD_REFERENCE/">字段手册</a>
      <a href="COMMANDS_7709/">命令映射</a>
    </nav>
  </aside>

  <div class="catalog-main">
    <header class="catalog-heading">
      <p class="catalog-kicker">接口目录</p>
      <h1 data-interface-heading>接口文档</h1>
      <p data-interface-lead>默认按功能浏览，也可以切换到按 7709 原生协议接口、7615 原生 Entry 接口和 Helpers 封装浏览。</p>
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

    <section class="catalog-scope" aria-labelledby="catalog-scope-title">
      <h2 id="catalog-scope-title">统计口径</h2>
      <p><code>7709 原生协议接口</code> 收录 22 项二进制业务能力，按业务分类展示。</p>
      <p><code>7615 原生 Entry 接口</code> 将通用 Entry 和 20 个功能调用平铺在同一目录；它使用 HTTP POST 与 JSON，是 7615/TQLEX 的原生调用入口。</p>
      <p><code>Helpers 封装</code> 收录组合协议调用、分页、拆批、内容解析、本地整理和计算；短线指标可返回开盘量比、流通股本、封单和连板字段。</p>
      <p><code>MCP</code> 是面向 Agent 的工具服务，保留独立文档，但不计入接口目录分类。</p>
      <p>目录数据随静态页面发布，打开后不会连接行情主站、F10 网关或任何 eltdx 后台服务。</p>
    </section>
  </div>
</section>
