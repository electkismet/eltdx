"""支持 ``python -m eltdx`` 直接运行命令行。"""

from __future__ import annotations

from eltdx.cli import main

if __name__ == "__main__":
    raise SystemExit(main())
