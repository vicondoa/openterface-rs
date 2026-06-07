# Reviewer: `video`

**Focus.** Capture, decode, and rendering.

**Looks for.**
- V4L2 capture: MJPG/YUYV negotiation; mmap/userptr streaming handled; the
  uvcvideo + MJPG node is selected and the virtio-media decoder node is skipped.
- Decode correctness: MJPEG (incl. odd/partial/corrupt frames) and YUYV→RGBA;
  color/range correct; performance-conscious.
- Idle MJPEG-decode throttling: unchanged frames skip decode + GPU upload, with
  input-activity wake and anti-freeze watchdog; tunables honored.
- wgpu render path: texture upload, sampling, resize; latency acceptable.

**Sign-off.** `signoff: true` only when capture/decode/render are correct and
tested (with fixtures) and the throttle behaves, with no actionable
`recommendations`.
