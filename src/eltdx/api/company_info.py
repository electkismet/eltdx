"""F10 company-information API.

The 7709 channel exposes a small catalog (0x02CF) that lists the company-info
files available for one security together with their byte offsets, and a reader
(0x02D0) that returns one GBK-decoded text section at a time.
"""

from __future__ import annotations

from .base import ApiBase


class CompanyInfoApi(ApiBase):
    def category(self, code: str, market: str):
        """List the 0x02CF company-info files (name, filename, start, length)."""

        return self._execute("company_info_category", code=code, market=market)

    def content(self, code: str, market: str, filename: str, offset: int, length: int):
        """Read one 0x02D0 company-info text section as a GBK-decoded string."""

        return self._execute(
            "company_info_content",
            code=code,
            market=market,
            filename=filename,
            offset=offset,
            length=length,
        )
