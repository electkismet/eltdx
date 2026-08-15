# GitHub Pages

eltdx 的文档站是纯静态站点。GitHub Actions 使用 MkDocs Material 将仓库内 `docs/` 构建到根目录 `site/`，再把构建产物部署到 GitHub Pages。

## 本地构建

```bash
python -m pip install "mkdocs-material==9.7.6"
python -m mkdocs build --strict
```

本地预览：

```bash
python -m mkdocs serve
```

## 发布

仓库的 Pages source 需要设置为 **GitHub Actions**。之后每次推送 `main`，或手动运行 `Pages` workflow，都会重新构建并部署文档站。

## 边界

- MkDocs 只在文档构建环境安装，不属于 eltdx 运行时依赖。
- `site/` 是生成目录，已被 Git 忽略，不提交到仓库。
- 接口目录的数据随静态 JavaScript 一起发布，打开页面不会请求行情主站、F10 网关或后台服务。
- Pages 不改变 `TdxClient`、7709、7615 或 MCP 的运行方式。
