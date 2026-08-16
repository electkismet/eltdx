"""Import-free contracts for the native runtime's resident memory bounds."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).parents[2]
ENGINE = (ROOT / "crates" / "eltdx-runtime" / "src" / "engine.rs").read_text(
    encoding="utf-8"
)
REQUEST = (ROOT / "crates" / "eltdx-runtime" / "src" / "request.rs").read_text(
    encoding="utf-8"
)
SLOT = (ROOT / "crates" / "eltdx-runtime" / "src" / "slot.rs").read_text(
    encoding="utf-8"
)
PUSH = (ROOT / "crates" / "eltdx-runtime" / "src" / "push.rs").read_text(
    encoding="utf-8"
)
LIMITS = (ROOT / "crates" / "eltdx-protocol" / "src" / "limits.rs").read_text(
    encoding="utf-8"
)
MEMORY = (ROOT / "crates" / "eltdx-runtime" / "src" / "memory.rs").read_text(
    encoding="utf-8"
)


def test_post_ingress_waiter_collections_have_hard_caps() -> None:
    assert "self.push_waiters.len() >= self.config.max_pending_requests" in ENGINE
    assert "self.pin_close_waiter_count()?" in ENGINE
    assert "waiter_count >= self.config.max_pending_requests" in ENGINE
    assert 'message: "7709 push waiter queue is full"' in ENGINE
    assert 'message: "7709 pin close waiter queue is full"' in ENGINE


def test_channels_admission_decoders_and_push_buffers_are_bounded() -> None:
    assert "mpsc::channel(capacity)" in ENGINE
    assert "mpsc::channel(event_capacity.max(1))" in ENGINE
    assert "mpsc::channel(config.push_queue_size)" in ENGINE
    assert "mpsc::channel(1)" in ENGINE
    assert "self.waiting_count() > self.max_pending_requests" in REQUEST
    assert "self.active_count() > self.pool_size" in REQUEST
    assert "MAX_DECODED_QUEUE_FRAMES" in SLOT
    assert "MAX_DECODED_QUEUE_BYTES" in SLOT
    assert "self.frames.len() > self.max_frames" in PUSH
    assert "self.bytes > self.max_bytes" in PUSH
    for literal in (
        "SLOT_WIRE_BUDGET_BYTES: usize = 256 * 1024",
        "SLOT_FRAME_BUDGET: usize = 64",
        "SLOT_DECODED_BUDGET_BYTES: usize = 4 * 1024 * 1024",
        "MAX_DECODED_QUEUE_FRAMES: usize = 1_024",
        "MAX_DECODED_QUEUE_BYTES: usize = 8 * 1024 * 1024",
    ):
        assert literal in LIMITS


def test_runtime_workers_ingress_and_engine_memory_are_explicitly_bounded() -> None:
    assert "runtime_workers == 1" in ENGINE
    assert "Builder::new_current_thread()" in ENGINE
    assert "Builder::new_multi_thread()" in ENGINE
    assert ".worker_threads(runtime_workers)" in ENGINE
    assert "DIAGNOSTICS_REFRESH_INTERVAL: Duration = Duration::from_millis(25)" in ENGINE
    assert ".blocking_send(" not in ENGINE
    assert "MemoryBudget::new(" in ENGINE
    assert "self.memory_budget.check_empty()?" in ENGINE
    assert "self.connection_limiter.check_idle(&self.config)?" in ENGINE
    assert "try_reserve_raw" in SLOT
    assert "try_reserve_decoded" in SLOT
    assert "AtomicUsize" in MEMORY
    assert "notify_waiters" in MEMORY
    assert "value.checked_sub(bytes)" in MEMORY
