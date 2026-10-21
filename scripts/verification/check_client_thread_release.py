"""验证 TdxClient 不再因引用环而延迟回收底层 worker 线程。

用法（需要真实主站，会发起少量请求）：

    set PYTHONPATH=src
    python scripts/verification/check_client_thread_release.py --count 5

判定标准：**不调用 gc.collect()** 时，逐个 ``del`` 未 ``close()`` 的客户端后，
OS 线程数不应随客户端数量线性增长（改造前 5 个客户端 ≈ 30-55 条线程）。
"""

from __future__ import annotations

import argparse
import gc
import os
import sys
from pathlib import Path

ROOT = next(
    parent for parent in Path(__file__).resolve().parents if (parent / "pyproject.toml").exists()
)
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

import psutil  # noqa: E402

from eltdx import TdxClient  # noqa: E402

DEFAULT_HOST = "116.205.183.150:7709"


def _threads() -> int:
    return psutil.Process(os.getpid()).num_threads()


def _warmup(host: str) -> None:
    """先建一个客户端并正常 close，排除首次导入/初始化带来的线程噪声。"""

    client = TdxClient(host=host, timeout=6.0, probe_hosts=False, heartbeat_interval=None)
    client.quotes.get_snapshots("sz000001")
    client.close()
    del client
    gc.collect()


def _churn(host: str, count: int) -> None:
    """建 count 个客户端，各查一次，**不 close** 直接丢弃。"""

    for _ in range(count):
        client = TdxClient(host=host, timeout=6.0, probe_hosts=False, heartbeat_interval=None)
        client.quotes.get_snapshots("sz000001")
        del client


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default=DEFAULT_HOST)
    parser.add_argument("--count", type=int, default=5)
    args = parser.parse_args()

    gc.disable()
    try:
        _warmup(args.host)
        baseline = _threads()

        _churn(args.host, args.count)
        after_del = _threads()

        print(f"host={args.host} count={args.count} gc_enabled={gc.isenabled()}")
        print(f"threads baseline (warmup closed)      : {baseline}")
        print(f"threads after {args.count}x del (no close, no gc): {after_del}")
        print(f"growth per client                     : {(after_del - baseline) / args.count:.1f}")

        gc.collect()
        after_gc = _threads()
        print(f"threads after explicit gc.collect()   : {after_gc}")
    finally:
        gc.enable()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
