# 发布指南

这份文档只解决两件事：

1. 把 `eltdx` 上传到 GitHub
2. 让别人可以直接执行 `pip install eltdx`

## 一次看懂整条链路

要实现 `pip install eltdx`，核心链路其实很短：

1. 本地代码整理完成
2. 上传到 GitHub 仓库
3. 在 PyPI 注册并发布 `eltdx`
4. 之后用户即可通过 `pip install eltdx` 安装

其中：

- GitHub 负责托管源码、文档、Issue、Actions 工作流
- PyPI 负责提供 `pip install eltdx`
- `pip` 安装时默认是从 PyPI 拉包，不是从 GitHub 直接安装

## 当前项目已经具备什么

当前仓库已经具备这些发布前提：

- `pyproject.toml` 已配置包名、版本、构建后端
- `README.md`、`LICENSE`、`CHANGELOG.md` 已存在
- `python -m build` 可生成 wheel / sdist
- 已提供 GitHub Actions：
  - `.github/workflows/ci.yml`
  - `.github/workflows/publish.yml`

这意味着：

- 你把仓库推到 GitHub 后，CI 就能自动跑测试与构建
- 你完成 PyPI 绑定后，打 `v0.1.0` 这类标签就能自动发布到 PyPI

## 第一步：初始化本地 Git 仓库

如果当前目录还不是 Git 仓库，在项目根目录执行：

```powershell
git init -b main
git add .
git commit -m "Initial release prep"
```

如果你还没配置 Git 用户信息，先执行：

```powershell
git config --global user.name "你的 GitHub 用户名"
git config --global user.email "你的 GitHub 邮箱"
```

## 第二步：在 GitHub 创建仓库

建议仓库名就叫：

- `eltdx`

创建完成后，把远端地址连上：

```powershell
git remote add origin https://github.com/<你的用户名>/eltdx.git
git push -u origin main
```

如果你更喜欢 SSH：

```powershell
git remote add origin git@github.com:<你的用户名>/eltdx.git
git push -u origin main
```

## 第三步：把仓库地址补回 `pyproject.toml`

创建好 GitHub 仓库后，建议把仓库地址补到 `pyproject.toml` 的 `[project.urls]`：

```toml
[project.urls]
Homepage = "https://github.com/<你的用户名>/eltdx"
Repository = "https://github.com/<你的用户名>/eltdx"
Documentation = "https://github.com/<你的用户名>/eltdx/tree/main/docs"
Issues = "https://github.com/<你的用户名>/eltdx/issues"
Changelog = "https://github.com/<你的用户名>/eltdx/blob/main/CHANGELOG.md"
```

这一步不是 `pip install` 的硬性要求，但强烈建议补上。

## 第四步：注册 PyPI 账号

先注册：

- PyPI：[https://pypi.org/account/register/](https://pypi.org/account/register/)

发布前建议也准备测试环境：

- TestPyPI：[https://test.pypi.org/account/register/](https://test.pypi.org/account/register/)

## 第五步：先做一次本地构建检查

项目根目录执行：

```powershell
python -m build
python scripts/smoke_isolated_install.py
```

这两步分别验证：

- 能否正确构建 wheel / sdist
- 生成的 wheel 能否在隔离环境中正常导入

## 第六步：配置 GitHub 到 PyPI 的自动发布

当前仓库已经准备好这个工作流：

- `.github/workflows/publish.yml`

它的行为是：

- 当你推送形如 `v0.1.0` 的 Git tag 时
- GitHub Actions 先构建分发包
- 然后通过 PyPI Trusted Publishing 自动发布

你需要在 PyPI 项目里把 GitHub 仓库配置为 Trusted Publisher。

建议流程：

1. 先在 GitHub 把仓库推上去
2. 在 PyPI 创建项目发布权限配置
3. 将 GitHub 仓库 / workflow / environment 与 PyPI 绑定
4. 之后使用标签发布

官方说明：

- PyPI Trusted Publishers：[https://docs.pypi.org/trusted-publishers/](https://docs.pypi.org/trusted-publishers/)
- GitHub Actions 发布插件：[https://github.com/pypa/gh-action-pypi-publish](https://github.com/pypa/gh-action-pypi-publish)

## 第七步：发布第一个版本

确认版本号一致：

- `pyproject.toml`
- `src/eltdx/__about__.py`

然后打标签并推送：

```powershell
git tag v0.1.0
git push origin v0.1.0
```

如果 GitHub Actions 与 PyPI Trusted Publishing 配好了，这一步会自动把包发布到 PyPI。

## 第八步：安装验证

发布成功后，别人就可以直接安装：

```powershell
pip install eltdx
```

或者更稳妥一点：

```powershell
python -m pip install eltdx
```

安装后可快速验证：

```powershell
python -c "from eltdx import TdxClient; print(TdxClient)"
```

## 推荐的首发顺序

建议按这个顺序做：

1. 本地 `pytest`
2. 本地 `python -m build`
3. 本地 `python scripts/smoke_isolated_install.py`
4. `git init` / commit
5. 推到 GitHub
6. 补 `[project.urls]`
7. 配置 PyPI Trusted Publishing
8. 打 `v0.1.0` 标签
9. 验证 `pip install eltdx`

## 常见问题

### `pip install eltdx` 是从 GitHub 安装吗？

不是。默认是从 PyPI 安装。

### 只上传到 GitHub，不上传 PyPI，可以直接 `pip install eltdx` 吗？

不可以。

只上传 GitHub 后，用户最多可以这样安装：

```powershell
pip install git+https://github.com/<你的用户名>/eltdx.git
```

但这不是你想要的标准体验。要实现标准体验，还是要发布到 PyPI。

### 为什么还要 GitHub Actions？

因为它能把“手动打包上传”变成“打 tag 自动发布”，后续维护省心很多。

### `publish.yml` 放着不配 PyPI 会怎样？

不会影响本地开发。

只是你打标签后，发布步骤会因为还没完成 PyPI 绑定而失败。

## 当前最关键的一件事

如果你准备现在就进入发布流程，优先做这件事：

- 先创建 GitHub 仓库，并拿到最终仓库地址

因为拿到仓库地址后，我就可以继续帮你：

- 把 `[project.urls]` 补完整
- 检查 README 里的 GitHub / 文档链接
- 按正式发布前状态再跑一次完整检查
