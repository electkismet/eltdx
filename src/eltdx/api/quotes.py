"""Quote and quote-list API."""

from __future__ import annotations

from collections.abc import Sequence

from .base import ApiBase


class QuoteApi(ApiBase):
    def get_snapshots(self, codes: str | Sequence[str]):
        code_list = [codes] if isinstance(codes, str) else list(codes)
        return self._execute("snapshots", codes=code_list)

    def legacy(self, codes: str | Sequence[str]):
        """Query the old-style batch quote interface (0x053E)."""

        code_list = [codes] if isinstance(codes, str) else list(codes)
        return self._execute("legacy_quotes", codes=code_list)

    def get_depth(self, codes: str | Sequence[str]):
        """Refresh five-level quote depth through native 0x0547."""

        return self.refresh(codes, cursors={})

    def list_by_category(
        self,
        category: str | int,
        *,
        sort_by: str | int | None = None,
        start: int = 0,
        count: int = 80,
        ascending: bool = False,
    ):
        return self._execute(
            "category_quotes",
            category=category,
            sort_by=sort_by,
            start=start,
            count=count,
            ascending=ascending,
        )

    def refresh(self, codes: str | Sequence[str] | None = None, cursors: dict[str, int] | None = None):
        """Refresh up to 100 codes in one 0x0547 request."""

        code_list = None if codes is None else ([codes] if isinstance(codes, str) else list(codes))
        return self._execute("refresh_stream", codes=code_list or [], cursors=dict(cursors or {}))

    def poll_push(self, *, timeout: float | None = 0.0, parse: bool = False):
        poll = getattr(self._transport, "poll_push", None)
        if poll is None:
            return None
        return poll(timeout=timeout, parse=parse)

    def drain_pushes(self, *, parse: bool = False):
        drain = getattr(self._transport, "drain_pushes", None)
        if drain is None:
            return []
        return drain(parse=parse)
