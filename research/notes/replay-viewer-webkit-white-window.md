# Replay viewer WebKitGTK white-window finding

## Scope

This note records the August 19, 2026 diagnosis of a blank white window from:

```bash
cd components/replay-viewer
npm run tauri dev
```

The failure was reproduced on Fedora 44 with WebKitGTK 2.52.5, GTK 3.24.52,
Mesa 26.1.6, and an AMD Navi 48 / Radeon RX 9070 XT using the `amdgpu` driver.
Those versions identify the validated machine; they are not asserted to be the
minimum or complete affected set.

## Evidence

The native process, Vite server, and WebKit helper processes remained alive. The
same development URL rendered normally in a separate browser through both
`127.0.0.1` and `localhost`, ruling out the configured host name and compiled Vue
assets.

Temporary diagnostics, removed after the investigation, established that:

- Tauri native setup completed;
- Vue mounted without a reported JavaScript error;
- the frontend invoked the `initial_state` command successfully; and
- the WebKit document contained 341 rendered text characters in a 1440 by 853
  viewport even though the native window presented as white.

The fault was therefore below the Vue/Tauri application state: WebKitGTK had a
populated render tree but its accelerated compositor did not present it visibly.
It was unrelated to timeline schema v10, replay parsing, SQLite cache invalidation,
or the new tactical-aid event variant.

## Initial resolution and recurrence

The initial workaround used `WEBKIT_DISABLE_COMPOSITING_MODE=1`, which WebKit
documents as forcing accelerated compositing off. A same-machine retest on August
19 showed that workaround, `WEBKIT_DISABLE_DMABUF_RENDERER=1`, and GTK's X11 backend
when tried separately still produced a white window. The Vue application continued
to render normally through the standalone Vite URL, keeping the fault below the
frontend.

`WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` selected WebKitGTK's valid shared-memory
transport and made the native window visible. This is distinct from
`WEBKIT_DISABLE_DMABUF_RENDERER=1`, which can leave current WebKitGTK releases
without a usable fallback transport.

Setting the replacement only inside Rust `main()` was not stable: the first
explicitly prefixed launch rendered, while a subsequent ordinary launch whose WebKit
helper inherited the late mutation returned to white. The viewer's
`components/replay-viewer/scripts/tauri.mjs` adds
the default to Tauri's initial environment before spawning the CLI and native
process. The Rust entry point retains the same conditional value only as a
direct-launch fallback. Explicit caller values remain authoritative in both paths.

On August 20 the ordinary command again produced a blank window even though both
the native process and its WebKit child inherited
`WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`. A separate browser still rendered the Vue
application normally. The active desktop has a mixed-scale Wayland layout: the Dell
display runs at 1.0x while the primary LG display runs at 1.5x. This matches a
separate WebKitGTK class of blank-WebView failures tied to monitor placement and
scale rather than the DMA-BUF transport alone.

A controlled `GDK_BACKEND=x11` launch combined with shared-memory transport rendered
the complete native 1,055-replay library. The viewer window was captured directly
through its X11 window ID, proving native presentation rather than relying on the
browser-only Vite check. Repeating the ordinary command after moving this policy
into the launcher produced the same rendered native window.

The Linux launcher now deliberately overrides a desktop-wide
`GDK_BACKEND=wayland` with `GDK_BACKEND=x11`. Future Wayland testing uses the
viewer-specific `WIC_REPLAY_VIEWER_GDK_BACKEND=wayland` override. This avoids an
unrelated shell or desktop setting silently reintroducing the failure. The Rust
entry point mirrors both defaults for direct launches, and a Node regression test
locks the launcher environment policy.

The normal command therefore needs no shell prefix:

```bash
npm run tauri dev
```

Non-Linux builds are unchanged. Packagers and developers retain control through the
viewer-specific backend override and an explicit shared-memory setting. These
compatibility settings are independent of the historical Fedora AppImage packaging
work around `linuxdeploy`, `strip`, and mixed-architecture GTK module selection.
AppImage is no longer a release target; that packaging task is retired.

## Verification

- The default development command launched the native process successfully.
- The native window remained white when the old compositing fallback, the obsolete
  DMA-BUF-disable setting, and GTK's X11 backend were each tried separately.
- The native window rendered when `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` was present
  before Tauri started.
- The ordinary npm development command placed the setting in both the native
  viewer's initial environment and the spawned WebKit renderer.
- The native X11 window rendered the full 1,055-replay library with shared-memory
  transport, both under a controlled launch and the final ordinary npm command.
- The launcher environment regression test proves Linux overrides an inherited
  Wayland backend while preserving viewer-specific and WebKit-specific overrides.
- `components/replay-viewer/scripts/quality.sh` passed frontend type checking,
  Vitest, the production Vite build, Rust formatting, 24 Rust tests, and Clippy
  with warnings denied.
- The pre-existing generated
  `components/replay-viewer/src-tauri/gen/schemas/linux-schema.json` remained
  untracked and was not staged.

## Follow-up boundary

Retest native Wayland and DMA-BUF rendering after material WebKitGTK, GTK, Mutter, or
Mesa updates. Remove either default only after the ordinary Tauri window renders on
both monitor scales with `WIC_REPLAY_VIEWER_GDK_BACKEND=wayland` and the
shared-memory fallback explicitly disabled. A successful browser-only Vite render is
necessary for frontend validation but is not sufficient to prove the native WebKit
presentation path works.

## September 18, 2026 follow-up

The released 0.5.0 AppImage was launched on the affected Fedora desktop with
`WIC_REPLAY_VIEWER_GDK_BACKEND=wayland` and the existing shared-memory transport
default. Its native window displayed the library normally, as confirmed by the
user. This establishes a successful Wayland startup, not sustained playback or
DMA-BUF rendering. The Linux backend default was changed to Wayland in the npm
launcher and native entry point; X11 remains available through the viewer-specific
override. The DMA-BUF retest above remains open.
