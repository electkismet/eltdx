"""Shared API helpers."""

from __future__ import annotations

import weakref
from typing import Any

from eltdx.protocol.commands import command_code
from eltdx.transport import Transport


def _weak_callback(callback: Any) -> Any:
    """把回调存成弱引用，避免子服务反过来把 ``TdxClient`` 钉住形成引用环。

    ``TdxClient`` 与它的子服务互持会构成引用环：子服务持有 client 的 bound
    method，client 又持有子服务，引用计数永远到不了 0，底层 worker 线程只能
    等分代 GC 才回收。这里统一改成弱引用：

    * bound method 必须用 :class:`weakref.WeakMethod` —— ``weakref.ref`` 拿到
      的是临时创建的 method 对象，下一个字节码就失效了；
    * 普通可调用对象（函数、测试里的 factory 实例等）退回 ``weakref.ref``；
    * 解析结果统一是「可调用对象或 ``None``」，调用点语义不变；
    * 两种弱引用都建不出来时（``__self__`` / 对象本身不可弱引用）保留强引用，
      宁可退化也不要抛 ``TypeError`` 改变既有构造行为。
    """

    if callback is None:
        return None
    if getattr(callback, "__self__", None) is not None:
        try:
            return weakref.WeakMethod(callback)
        except TypeError:
            return callback
    try:
        return weakref.ref(callback)
    except TypeError:
        return callback


class ApiBase:
    """Base class for capability-specific APIs."""

    def __init__(self, transport: Transport, *, price_resolver=None, metadata_sink=None) -> None:
        self._transport = transport
        self._price_resolver = _weak_callback(price_resolver)
        self._metadata_sink = _weak_callback(metadata_sink)

    def _execute(self, command_name: str, **payload: Any) -> Any:
        return self._transport.execute(command_code(command_name), payload)

    def _execute_priced(self, command_name: str, *, price_codes=None, **payload: Any) -> Any:
        # 只在本次调用开始时解析一次：回调缺失（未提供或已随 client 回收）
        # 时行为与改造前完全一致 —— 直接返回原始结果，不额外抛错。
        resolver = self._price_resolver() if self._price_resolver is not None else None
        if resolver is not None:
            resolver(command_name, None, price_codes)
        result = self._execute(command_name, **payload)
        if resolver is None:
            return result
        return resolver(command_name, result, price_codes)
