"""扩展行情命令基类：只含请求构造与响应解析，不含 IO。

基类本体在 :mod:`eltdx._command`，此处只做再导出——MAC 与扩展行情两套命令
共用同一个 ``BaseCommand`` 类对象（此前两份结构相同、仅 TypeVar 名不同的
定义会让类型检查判为不兼容）。
"""

from __future__ import annotations

from eltdx._command import BaseCommand

__all__ = ["BaseCommand"]
