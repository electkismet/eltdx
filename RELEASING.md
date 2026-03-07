# 发布说明

这份文件给项目维护者使用，记录 `eltdx` 的发版流程。

当前仓库已经准备好 GitHub Actions 自动发布：

- 工作流文件：`.github/workflows/publish.yml`
- 触发方式：推送 `v*` 标签，或者手动运行 `workflow_dispatch`
- 发布目标：PyPI

## 第一次使用前要做的事

如果你希望“推送标签后自动发到 PyPI”，需要先在 PyPI 后台配置 Trusted Publisher。

按官方文档，去 PyPI 项目后台添加 GitHub 发布者，建议使用下面这些值：

- PyPI 项目名：`eltdx`
- GitHub owner：`electkismet`
- GitHub repository：`eltdx`
- Workflow name：`publish.yml`
- Environment name：`pypi`

如果这些值填错，GitHub Actions 会构建成功，但发布到 PyPI 那一步会失败。

参考：

- [PyPI Trusted Publishers](https://docs.pypi.org/trusted-publishers/)
- [Packaging guide: Publishing package distribution releases using GitHub Actions CI/CD workflows](https://packaging.python.org/en/latest/guides/publishing-package-distribution-releases-using-github-actions-ci-cd-workflows/)

## 每次发版前要检查什么

先确认下面几项已经更新：

- `pyproject.toml` 里的 `version`
- `src/eltdx/__about__.py` 里的 `__version__`
- `CHANGELOG.md` 已补新版本说明
- 如果你改了首页展示，再顺手看一下 `README.md`

建议本地先跑：

```bash
python -m build
python -m pytest -q
```

如果本地构建或测试没过，不要急着打标签。

## 标准发版流程

### 1. 提交版本改动

```bash
git add pyproject.toml src/eltdx/__about__.py CHANGELOG.md README.md
git commit -m "Prepare 0.1.4 release"
git push origin main
```

### 2. 打标签并推送

```bash
git tag v0.1.4
git push origin v0.1.4
```

推送标签后，GitHub 会自动触发：

- `build`：构建 `sdist` 和 `wheel`
- `publish`：把构建产物发布到 PyPI

### 3. 检查 GitHub Actions

去仓库的 Actions 页面，确认 `Publish` 工作流已经成功。

链接：

- [GitHub Actions](https://github.com/electkismet/eltdx/actions)

如果失败，先看报错是在：

- 构建阶段失败
- 还是发布到 PyPI 阶段失败

两者处理方式不一样。

### 4. 检查 PyPI

发布完成后，去 PyPI 页面看三个地方：

- 版本号是否已经更新
- 顶部摘要是否正常显示
- README 长说明是否正常渲染

链接：

- [PyPI - eltdx](https://pypi.org/project/eltdx/)

### 5. 本地再装一遍验证

推荐再做一次最直接的安装验证：

```bash
python -m pip install -U eltdx
python -c "import eltdx; print(eltdx.__version__)"
```

如果你想更稳一点，可以在一个新的虚拟环境里验证。

## GitHub Release 怎么写

如果你想在 GitHub 的 Releases 页面补一份更好读的中文说明，可以用下面这个模板：

````md
## eltdx 0.1.4

这版主要更新：

- 修复 PyPI 顶部摘要显示
- 补充文档导航
- 优化发布展示细节

### 安装

```bash
python -m pip install -U eltdx
```

### 适用环境

- Python 3.10+

### 说明

- PyPI 页面不会自动跟 GitHub 首页同步
- 只有发布新版本后，PyPI 才会显示最新的 README 和包元数据
````

如果你不想手写，也可以在 GitHub Release 页面点 `Generate release notes`，仓库里的 `.github/release.yml` 会参与生成规则。

参考：

- [GitHub Docs: Automatically generated release notes](https://docs.github.com/en/repositories/releasing-projects-on-github/automatically-generated-release-notes)

## 常见问题

### 为什么 GitHub 改了，PyPI 还是旧的？

因为 PyPI 不会实时读取 GitHub 页面。

PyPI 展示的是“最近一次上传到 PyPI 的发行包里附带的元数据和 README”。

### 为什么标签推上去了，但 PyPI 没发成功？

大概率是下面几种情况：

- PyPI Trusted Publisher 还没配
- PyPI 里填的仓库 / workflow / environment 名字不对
- 这个版本号已经在 PyPI 存在，不能重复上传

### 标签打错了怎么办？

如果还没真正发出去，可以删标签重来。

如果已经发到了 PyPI，通常不建议覆盖同版本，最好直接发下一个版本号。

## 手动发布兜底方案

如果 GitHub Actions 暂时不可用，也可以手动发布：

```bash
python -m pip install -U build twine
python -m build
python -m twine upload dist/*
```

但长期建议还是优先使用 Trusted Publishing，配置更干净，也不用在 GitHub Secrets 里长期保存 PyPI token。
