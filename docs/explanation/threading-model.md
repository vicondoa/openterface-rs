# Threading & shutdown model

openterface-rs uses **std threads + channels**, not an async runtime. This
keeps builds fast and makes the data flow explicit. The rule is: every thread
has a defined owner, a way to be told to stop, and a join on shutdown — so
stopping never deadlocks, even when the device disappears.

## Threads in a session

| Thread | Owns | Reads | Writes |
|--------|------|-------|--------|
| **Capture** | the `VideoSource` | frames from V4L2 | decoded frames → a bounded `SyncSender<Frame>` |
| **Serial writer** | the `SerialTransport` | `InputEvent`s from a channel | paced CH9329 bytes to the device |
| **GUI event loop** | the window | Wayland input + frame channel | `InputEvent`s into the writer's channel |

The GUI loop runs on the main thread (a winit requirement); capture and the
serial writer are spawned.

## The pacing scheduler

The serial writer is a **queue-based scheduler**, not a passive sink, because the
CH9329 misbehaves if you write to it too fast (see
[env vars](../reference/env-vars.md)). It:

- **paces** mouse moves to one per `OPENTERFACE_MOUSE_INTERVAL_MS` (~30 Hz),
- **coalesces** consecutive moves (only the latest position matters), and
- gives **key/button releases priority** so they jump ahead of a move backlog
  and never arrive late.

It is built around an injectable clock so timing behavior is tested
deterministically with a fake clock — no `sleep`s in tests.

## Shutdown / cancellation

Stopping is driven by **two** signals so no thread can hang:

- a shared `running: AtomicBool` flag (the capture thread checks it between
  blocking reads, which use a timeout), and
- **channel disconnection** — dropping the input sender wakes the serial
  writer's blocking `recv()` with a disconnect.

`Session::shutdown` (also run on drop) sets `running = false`, drops the sender,
and **joins** both threads. The writer drains any already-queued events —
including pending **releases** — before exiting, so a stop in the middle of a
drag or keypress still releases on the target. Blocking serial/video reads use
timeouts so a wedged device cannot block the join forever.

## Why not tokio

The pipeline is a handful of long-lived threads with simple channels; async would
add build cost and a runtime without buying anything here. If a future feature
(e.g. a TCP control server) needs many concurrent I/O tasks, that part can adopt
async in isolation without rewriting the core.
