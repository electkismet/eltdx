(function () {
  "use strict";

  function promoteDetailBackLink() {
    var link = document.querySelector(".interface-detail-back");
    document.documentElement.classList.toggle("interface-detail-page", Boolean(link));
    if (!link) {
      return;
    }

    var header = document.querySelector(".md-header__inner");
    var logo = header && header.querySelector('[data-md-component="logo"]');
    if (!header || !logo) {
      return;
    }

    var oldParent = link.parentElement;
    var searchBackIcon = document.querySelector('.md-search__icon[for="__search"] svg:last-of-type');
    link.classList.add("md-header__button", "md-icon", "interface-header-back");
    link.setAttribute("aria-label", "返回上一页");
    link.setAttribute("title", "返回上一页");
    if (searchBackIcon) {
      var icon = searchBackIcon.cloneNode(true);
      icon.setAttribute("aria-hidden", "true");
      link.replaceChildren(icon);
    } else {
      link.textContent = "←";
    }

    link.addEventListener("click", function (event) {
      if (document.referrer && window.history.length > 1) {
        event.preventDefault();
        window.history.back();
      }
    });
    header.insertBefore(link, logo);
    if (oldParent && !oldParent.textContent.trim()) {
      oldParent.remove();
    }
  }

  promoteDetailBackLink();

  var root = document.querySelector("[data-interface-catalog]");
  document.documentElement.classList.toggle("interface-catalog-page", Boolean(root));
  if (!root) {
    return;
  }

  var catalog = window.ELTDX_CATALOG;
  if (!catalog || !Array.isArray(catalog.items) || !catalog.taxonomy || !Array.isArray(catalog.taxonomy.layers)) {
    root.textContent = "接口目录数据加载失败。";
    return;
  }

  var items = catalog.items;
  var layers = catalog.taxonomy.layers;
  var functionalGroups = catalog.taxonomy.functional_groups || [];
  var sourceLabels = {
    "7709": "7709 原生协议接口",
    "F10": "7615 原生 Entry 接口",
    "Helper": "Helpers 封装"
  };
  var searchInput = root.querySelector("[data-interface-search]");
  var scopeSelect = root.querySelector("[data-interface-scope-select]");
  var tree = root.querySelector("[data-interface-tree]");
  var stats = root.querySelector("[data-interface-stats]");
  var viewButtons = root.querySelectorAll("[data-catalog-view]");
  var rows = root.querySelector("[data-interface-rows]");
  var resultCount = root.querySelector("[data-interface-result-count]");
  var empty = root.querySelector("[data-interface-empty]");
  var heading = root.querySelector("[data-interface-heading]");
  var lead = root.querySelector("[data-interface-lead]");
  var itemById = Object.create(null);
  var itemMeta = Object.create(null);
  var functionalMeta = Object.create(null);
  var scopes = Object.create(null);
  var functionScopes = Object.create(null);
  var taxonomyErrors = [];
  var scopeAliases = {
    "binary": "7709",
    "7709/commands": "7709",
    "7709/convenience": "helpers",
    "7615/entry": "7615",
    "7615/features": "7615",
    "wrapper/tdx-wrappers": "helpers",
    "wrapper/f10": "7615",
    "wrapper/helpers": "helpers"
  };

  function createElement(tag, className, text) {
    var element = document.createElement(tag);
    if (className) {
      element.className = className;
    }
    if (text !== undefined) {
      element.textContent = text;
    }
    return element;
  }

  function normalize(value) {
    return String(value || "")
      .normalize("NFKC")
      .toLocaleLowerCase()
      .replace(/\s+/g, " ")
      .trim();
  }

  function localDocUrl(item) {
    var relative = item.doc.replace(/\\/g, "/").replace(/\.md$/i, "/");
    return new URL(relative, window.location.href).href + (item.doc_anchor ? "#" + encodeURIComponent(item.doc_anchor) : "");
  }

  function registerItem(itemId, layer, group) {
    if (!itemById[itemId]) {
      taxonomyErrors.push("目录引用了不存在的接口：" + itemId);
      return;
    }
    if (itemMeta[itemId]) {
      taxonomyErrors.push("接口被重复归类：" + itemId);
      return;
    }
    itemMeta[itemId] = {layer: layer, group: group};
  }

  items.forEach(function (item) {
    itemById[item.id] = item;
  });

  function registerFunctionalItem(itemId, group) {
    if (!itemById[itemId]) {
      taxonomyErrors.push("功能目录引用了不存在的接口：" + itemId);
      return;
    }
    if (functionalMeta[itemId]) {
      taxonomyErrors.push("接口被重复归入功能分类：" + itemId);
      return;
    }
    functionalMeta[itemId] = group;
  }

  functionalGroups.forEach(function (group) {
    (group.item_ids || []).forEach(function (itemId) {
      registerFunctionalItem(itemId, group);
    });
  });
  functionalGroups.forEach(function (group) {
    (group.categories || []).forEach(function (category) {
      items.forEach(function (item) {
        if (!functionalMeta[item.id] && item.category === category) {
          registerFunctionalItem(item.id, group);
        }
      });
    });
  });
  var fallbackFunctionalGroups = functionalGroups.filter(function (group) {
    return group.fallback;
  });
  if (fallbackFunctionalGroups.length > 1) {
    taxonomyErrors.push("功能目录只能设置一个兜底分类");
  }
  if (fallbackFunctionalGroups.length === 1) {
    items.forEach(function (item) {
      if (!functionalMeta[item.id]) {
        registerFunctionalItem(item.id, fallbackFunctionalGroups[0]);
      }
    });
  }
  items.forEach(function (item) {
    if (!functionalMeta[item.id]) {
      taxonomyErrors.push("接口尚未归入功能分类：" + item.id);
    }
  });

  layers.forEach(function (layer) {
    (layer.item_ids || []).forEach(function (itemId) {
      registerItem(itemId, layer, null);
    });
    (layer.groups || []).forEach(function (group) {
      (group.item_ids || []).forEach(function (itemId) {
        registerItem(itemId, layer, group);
      });
    });
  });

  layers.forEach(function (layer) {
    if (!layer.source) {
      return;
    }
    items.forEach(function (item) {
      if (!itemMeta[item.id] && item.source === layer.source) {
        registerItem(item.id, layer, null);
      }
    });
  });

  layers.forEach(function (layer) {
    (layer.groups || []).forEach(function (group) {
      if (!group.source) {
        return;
      }
      items.forEach(function (item) {
        if (!itemMeta[item.id] && item.source === group.source) {
          registerItem(item.id, layer, group);
        }
      });
    });
  });

  items.forEach(function (item) {
    if (!itemMeta[item.id]) {
      taxonomyErrors.push("接口尚未归类：" + item.id);
    }
  });

  if (taxonomyErrors.length) {
    root.textContent = "接口目录分类失败：" + taxonomyErrors.join("；");
    return;
  }

  function countScope(layerId, groupId) {
    return items.filter(function (item) {
      var meta = itemMeta[item.id];
      return meta.layer.id === layerId && (!groupId || (meta.group && meta.group.id === groupId));
    }).length;
  }

  function buildScopes() {
    scopes.all = {
      id: "all",
      label: "接口文档",
      description: "共 " + items.length + " 项公开能力，按 7709 原生协议接口、7615 原生 Entry 接口和 Helpers 封装组织。",
      count: items.length
    };
    layers.forEach(function (layer) {
      var layerCount = countScope(layer.id);
      scopes[layer.id] = {
        id: layer.id,
        layerId: layer.id,
        label: layer.label,
        description: layer.description,
        count: layerCount
      };
      (layer.groups || []).forEach(function (group) {
        var scopeId = layer.id + "/" + group.id;
        var groupCount = countScope(layer.id, group.id);
        scopes[scopeId] = {
          id: scopeId,
          layerId: layer.id,
          groupId: group.id,
          label: group.label,
          description: layer.label + " / " + group.label + "，共 " + groupCount + " 项。",
          count: groupCount
        };
      });
    });
  }

  function buildFunctionScopes() {
    functionScopes["function/all"] = {
      id: "function/all",
      label: "全部功能",
      description: "共 " + items.length + " 项公开能力，默认按业务功能组织。",
      count: items.length
    };
    functionalGroups.forEach(function (group) {
      var scopeId = "function/" + group.id;
      functionScopes[scopeId] = {
        id: scopeId,
        groupId: group.id,
        label: group.label,
        description: group.description,
        count: items.filter(function (item) {
          return functionalMeta[item.id] === group;
        }).length
      };
    });
  }

  function scopeLink(scopeId, label, count, className) {
    var link = createElement("a", className || "catalog-tree-link");
    link.href = "#" + scopeId;
    link.dataset.scopeLink = scopeId;
    link.appendChild(createElement("span", "", label));
    link.appendChild(createElement("em", "", String(count)));
    return link;
  }

  function renderTree(view) {
    tree.textContent = "";
    if (view === "function") {
      tree.appendChild(scopeLink("function/all", "全部功能", items.length, "catalog-tree-all"));
      functionalGroups.forEach(function (group) {
        var scopeId = "function/" + group.id;
        tree.appendChild(scopeLink(scopeId, group.label, functionScopes[scopeId].count, "catalog-tree-leaf"));
      });
      return;
    }
    tree.appendChild(scopeLink("all", "全部接口", items.length, "catalog-tree-all"));
    layers.forEach(function (layer) {
      if (!(layer.groups || []).length) {
        tree.appendChild(scopeLink(layer.id, layer.label, scopes[layer.id].count, "catalog-tree-leaf"));
        return;
      }
      var details = createElement("details", "catalog-tree-layer");
      details.open = true;
      var summary = createElement("summary", "catalog-tree-summary");
      summary.appendChild(createElement("span", "catalog-tree-chevron"));
      summary.appendChild(createElement("span", "", layer.label));
      summary.appendChild(createElement("em", "", String(scopes[layer.id].count)));
      details.appendChild(summary);

      var children = createElement("div", "catalog-tree-children");
      children.appendChild(scopeLink(layer.id, "全部", scopes[layer.id].count));
      (layer.groups || []).forEach(function (group) {
        var scopeId = layer.id + "/" + group.id;
        children.appendChild(scopeLink(scopeId, group.label, scopes[scopeId].count));
      });
      details.appendChild(children);
      tree.appendChild(details);
    });
  }

  function renderScopeSelect(view) {
    scopeSelect.textContent = "";
    var allOption = createElement("option", "", (view === "function" ? "全部功能" : "全部接口") + " (" + items.length + ")");
    allOption.value = view === "function" ? "function/all" : "all";
    scopeSelect.appendChild(allOption);

    if (view === "function") {
      functionalGroups.forEach(function (group) {
        var scopeId = "function/" + group.id;
        var option = createElement("option", "", group.label + " (" + functionScopes[scopeId].count + ")");
        option.value = scopeId;
        scopeSelect.appendChild(option);
      });
      return;
    }

    layers.forEach(function (layer) {
      if (!(layer.groups || []).length) {
        var directOption = createElement("option", "", layer.label + " (" + scopes[layer.id].count + ")");
        directOption.value = layer.id;
        scopeSelect.appendChild(directOption);
        return;
      }
      var optionGroup = createElement("optgroup");
      optionGroup.label = layer.label;
      var layerOption = createElement("option", "", "全部 " + layer.label + " (" + scopes[layer.id].count + ")");
      layerOption.value = layer.id;
      optionGroup.appendChild(layerOption);
      (layer.groups || []).forEach(function (group) {
        var scopeId = layer.id + "/" + group.id;
        var option = createElement("option", "", group.label + " (" + scopes[scopeId].count + ")");
        option.value = scopeId;
        optionGroup.appendChild(option);
      });
      scopeSelect.appendChild(optionGroup);
    });
  }

  function renderStats(view) {
    stats.textContent = "";
    if (view === "function") {
      functionalGroups.forEach(function (group) {
        var scopeId = "function/" + group.id;
        var stat = scopeLink(scopeId, group.label, functionScopes[scopeId].count, "interface-stat");
        var count = stat.querySelector("em");
        var label = stat.querySelector("span");
        stat.textContent = "";
        stat.appendChild(createElement("strong", "", count.textContent));
        stat.appendChild(createElement("span", "", label.textContent));
        stats.appendChild(stat);
      });
      return;
    }
    layers.forEach(function (layer) {
      var stat = scopeLink(layer.id, layer.stat_label || layer.label, scopes[layer.id].count, "interface-stat");
      var count = stat.querySelector("em");
      var label = stat.querySelector("span");
      stat.textContent = "";
      stat.appendChild(createElement("strong", "", count.textContent));
      stat.appendChild(createElement("span", "", label.textContent));
      stats.appendChild(stat);
    });
  }

  function searchText(item, meta) {
    var calls = (item.calls || []).map(function (call) {
      return call.label + " " + call.api;
    }).join(" ");
    return normalize([
      item.title,
      item.source,
      sourceLabels[item.source],
      item.category,
      item.api,
      calls,
      (item.aliases || []).join(" "),
      item.protocol,
      item.kind,
      item.summary,
      item.return_model,
      meta.layer.label,
      meta.group ? meta.group.label : ""
    ].join(" "));
  }

  function renderRows() {
    items.forEach(function (item) {
      var meta = itemMeta[item.id];
      var row = createElement("article", "interface-row");
      row.setAttribute("role", "row");
      row.dataset.interfaceItem = "";
      row.dataset.layer = meta.layer.id;
      row.dataset.group = meta.group ? meta.group.id : "";
      row.dataset.source = item.source;
      row.dataset.functionalGroup = functionalMeta[item.id].id;
      row.dataset.search = searchText(item, meta);

      var name = createElement("div", "interface-cell interface-name");
      name.setAttribute("role", "cell");
      var titleLink = createElement("a", "", item.title);
      titleLink.href = localDocUrl(item);
      name.appendChild(titleLink);
      if (Array.isArray(item.calls) && item.calls.length) {
        var callList = createElement("div", "interface-call-list");
        item.calls.forEach(function (call) {
          var callRow = createElement("div", "interface-call");
          callRow.appendChild(createElement("span", "interface-call-label", call.label));
          callRow.appendChild(createElement("code", "", call.api));
          callList.appendChild(callRow);
        });
        name.appendChild(callList);
      } else {
        name.appendChild(createElement("code", "", item.api));
      }

      var directory = createElement("div", "interface-cell interface-source");
      directory.setAttribute("role", "cell");
      var layerTag = createElement("span", "interface-layer-tag", meta.layer.tag_label || meta.layer.label);
      layerTag.dataset.layer = meta.layer.id;
      directory.appendChild(layerTag);
      var directoryDetail = createElement("small", "", item.category);
      directoryDetail.dataset.directoryDetail = item.id;
      directory.appendChild(directoryDetail);

      var protocol = createElement("div", "interface-cell interface-protocol");
      protocol.setAttribute("role", "cell");
      protocol.appendChild(createElement("code", "", item.protocol));
      protocol.appendChild(createElement("small", "", item.kind));

      var description = createElement("div", "interface-cell interface-description", item.summary);
      description.setAttribute("role", "cell");

      var returned = createElement("div", "interface-cell interface-return");
      returned.setAttribute("role", "cell");
      returned.appendChild(createElement("code", "", item.return_model));
      var docLink = createElement("a", "", "文档");
      docLink.href = localDocUrl(item);
      docLink.setAttribute("aria-label", "查看 " + item.title + " 文档");
      returned.appendChild(docLink);

      row.appendChild(name);
      row.appendChild(directory);
      row.appendChild(protocol);
      row.appendChild(description);
      row.appendChild(returned);
      rows.appendChild(row);
    });
  }

  function currentScopeId() {
    var raw = window.location.hash.replace(/^#/, "");
    try {
      raw = decodeURIComponent(raw);
    } catch (error) {
      raw = "";
    }
    if (functionScopes[raw]) {
      return raw;
    }
    if (scopes[raw]) {
      return raw;
    }
    if (scopeAliases[raw]) {
      return scopeAliases[raw];
    }
    if (raw.indexOf("binary/") === 0) {
      return "7709";
    }
    return currentView() === "function" ? "function/all" : "all";
  }

  function currentView() {
    var raw = window.location.hash.replace(/^#/, "");
    if (functionScopes[raw]) {
      return "function";
    }
    if (scopes[raw] || scopeAliases[raw]) {
      return "interface";
    }
    return "function";
  }

  function rowMatchesScope(row, scope) {
    if (scope.id.indexOf("function/") === 0) {
      return scope.id === "function/all" || row.dataset.functionalGroup === scope.groupId;
    }
    if (!scope.layerId) {
      return true;
    }
    if (row.dataset.layer !== scope.layerId) {
      return false;
    }
    return !scope.groupId || row.dataset.group === scope.groupId;
  }

  function updateNavigation(activeScopeId) {
    Array.prototype.forEach.call(root.querySelectorAll("[data-scope-link]"), function (link) {
      if (link.dataset.scopeLink === activeScopeId) {
        link.setAttribute("aria-current", "page");
        var details = link.closest("details");
        if (details) {
          details.open = true;
        }
      } else {
        link.removeAttribute("aria-current");
      }
    });
    scopeSelect.value = activeScopeId;
  }

  function renderView(view) {
    root.dataset.catalogView = view;
    Array.prototype.forEach.call(viewButtons, function (button) {
      button.setAttribute("aria-pressed", button.dataset.catalogView === view ? "true" : "false");
    });
    renderTree(view);
    renderScopeSelect(view);
    renderStats(view);
    Array.prototype.forEach.call(rows.querySelectorAll("[data-directory-detail]"), function (detail) {
      var item = itemById[detail.dataset.directoryDetail];
      detail.textContent = view === "function" ? functionalMeta[item.id].label : item.category;
    });
  }

  function applyFilters() {
    var view = currentView();
    var activeScopeId = currentScopeId();
    var activeScopes = view === "function" ? functionScopes : scopes;
    var activeScope = activeScopes[activeScopeId] || activeScopes[view === "function" ? "function/all" : "all"];
    var terms = normalize(searchInput.value).split(" ").filter(Boolean);
    var visible = 0;

    Array.prototype.forEach.call(rows.children, function (row) {
      var matchesScope = rowMatchesScope(row, activeScope);
      var matchesQuery = terms.every(function (term) {
        return row.dataset.search.indexOf(term) >= 0;
      });
      row.hidden = !(matchesScope && matchesQuery);
      if (!row.hidden) {
        visible += 1;
      }
    });

    resultCount.textContent = String(visible);
    empty.hidden = visible !== 0;
    heading.textContent = activeScope.label;
    lead.textContent = activeScope.description;
    renderView(view);
    activeScopeId = currentScopeId();
    activeScope = activeScopes[activeScopeId] || activeScopes[view === "function" ? "function/all" : "all"];
    updateNavigation(activeScopeId);
    root.dataset.catalogReady = "true";
  }

  buildScopes();
  buildFunctionScopes();
  renderRows();

  searchInput.addEventListener("input", applyFilters);
  scopeSelect.addEventListener("change", function () {
    window.location.hash = scopeSelect.value;
  });
  Array.prototype.forEach.call(viewButtons, function (button) {
    button.addEventListener("click", function () {
      window.location.hash = button.dataset.catalogView === "function" ? "function/all" : "all";
    });
  });
  window.addEventListener("hashchange", applyFilters);
  applyFilters();
})();
