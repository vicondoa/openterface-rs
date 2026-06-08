# W1.2 — V4L2 capture spike

**Status: approach validated on paper; real-device fixtures deferred to the
work-ssd VM (W6).** No Openterface device is attached to this host — it lives on
the `work-ssd` nixling VM over USB/IP (`nixling usb attach`), where the real
V4L2 node and the closed-loop harness run. This note fixes the capture design so
W2.3 (decode) and W2.5 (video source) can proceed against synthetic fixtures.

## Known hardware facts (from the C++ implementation + docs)

- The capture endpoint is an **MS2109** UVC device: driver **`uvcvideo`**,
  advertising **MJPG** and **YUYV**, USB id `345f:2109`.
- In a VM there are several `/dev/video*` nodes; only the `uvcvideo`+MJPG one is
  the KVM. The virtio-media decoder adapter (often `/dev/video0`) must be
  **skipped** (see `cpp-cli-behavior.md` discovery section).
- Resolution / frame-rate matrix (from `FEATURES.md`): 640×480 … 1920×1080,
  with per-resolution Hz ranges; default capture is **1920×1080 @ 30, MJPG**.

## Crate + capture design (W2.5)

- Use the **`v4l`** crate (Video4Linux2 bindings) behind the
  `video::VideoSource` trait already defined in `openterface-core`.
- **Negotiation:** enumerate `v4l` formats → match the requested
  `CaptureConfig` (MJPG/YUYV, w×h, fps); if exact match fails, pick the closest
  supported and surface it via `active_config()` (the C++ silently re-selects;
  we make it observable).
- **Streaming:** MMAP buffers; hand each captured buffer to the pipeline as a
  `Frame { format, width, height, timestamp, data }` without copying the encoded
  payload.
- **Node selection (W2.6 discovery):** iterate `/dev/video*`, keep only
  `uvcvideo` devices whose `VIDIOC_ENUM_FMT` includes `MJPG`; ignore others.

## Fixtures & tests (hardware-free)

- **Unit/decode tests (W2.3):** a small set of **golden MJPEG and YUYV frames**.
  Synthetic minimal frames are generated in `openterface-test-support`
  (`SimulatedVideoSource`); a handful of *real* captured frames will be added as
  binary fixtures once available from work-ssd.
- **Real-frame capture (W6, in work-ssd):** `ffmpeg -f v4l2 -input_format mjpeg
  -i <node> -frames:v N` (the `tools/kvm-debug.sh capture` path) writes JPEGs to
  use as decode goldens and to record the device's real capability set into a
  fixture for `FixtureScanner`.

## Fault cases the simulator must model (W2.5)

Delayed frames, **corrupt/partial MJPEG** frames, format re-selection, and
**mid-stream device disappearance** (USB/IP detach) — these are the failures the
real device exhibits and what the decode/session layers must survive.

## Deferred / to verify on hardware (W6)

- Exact advertised format/fps matrix of the specific unit.
- MMAP vs USERPTR behavior and any `uvcvideo` quirks for this chip.
- Unplug/replug (USB/IP) recovery semantics.
