"""MAC / 扩展行情命令的公共基类。

本模块**刻意不导入任何 eltdx 内部模块**，因此可以被 :mod:`eltdx.mac` 与
:mod:`eltdx.ex` 同时依赖而不产生循环导入。

历史上 ``eltdx.mac.commands`` 与 ``eltdx.ex.commands.base`` 各自定义了一个
结构完全相同、仅 TypeVar 名不同的 ``BaseCommand``。两者是不同的类对象，
于是把 MAC 命令交给 ``MacExClient._execute``（其形参标注为 *ex* 的
``BaseCommand``）时会被类型检查判为不兼容。统一到本模块后不再有此问题。
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Generic, TypeVar

T = TypeVar("T")


class BaseCommand(ABC, Generic[T]):
    """行情命令基类：只含请求构造与响应解析，不含 IO。

    子类实现 :meth:`build_request`（返回含完整帧头的请求字节）与
    :meth:`parse_response`（解析解压后的响应 ``body``，返回强类型结果）。
    传输层负责发送与接收，不在此层。
    """

    @abstractmethod
    def build_request(self) -> bytes:
        """构造请求包（含完整帧头）。"""

        raise NotImplementedError

    @abstractmethod
    def parse_response(self, body: bytes) -> T:
        """解析解压后的响应 ``body``，返回强类型结果。"""

        raise NotImplementedError


__all__ = ["BaseCommand"]
