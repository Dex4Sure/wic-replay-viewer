# AppImage startup on Fedora

The first Ubuntu 22.04-built AppImage terminated its WebKit process during startup
on the Fedora 44 desktop. The generated report identified `webviewTerminated`,
2,440 ms after startup, with no imported replays. Its application fingerprint was
`70602943a068ba0679ffba69f8c1b046e6d0d11045f9ebacd81bdda5538c4cf6`.

The original AppImage SHA-256 was
`b0c2de0863719003f71ad5279402a7755d3686051d2ac775fda4b670dc61f8fb`.
A launch with an isolated profile reproduced this stderr message:

```text
Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...
```

Disabling WebKit compositing did not resolve it. Removing only the bundled
`usr/lib/libwayland-client.so.0` from an extracted copy eliminated the error.
Restoring that same library reproduced it; removing it again and repackaging
allowed the AppImage to render its library on Fedora. The captured window was
matched to the test process group and its executable inside the AppImage mount.
No replay import was needed for this startup check.

The [AppImage exclusion list](https://github.com/AppImage/pkg2appimage/blob/master/excludelist)
also excludes this library because of conflicts with newer Mesa drivers.
`scripts/package-appimage.sh` applies this exclusion after Tauri's dependency
collection, then packages the AppDir with Tauri's existing AppImage output tool.
Other bundled libraries and the application's rendering settings are retained.
No WebKit sandbox or compositing setting is disabled by the fix.

The earlier Ubuntu container smoke check established that the frontend was
embedded, but did not establish compatibility with Fedora's graphics stack.
Future packaging validation should include a launch on the intended host desktop.

Portable regression checks verify the library exclusion, retention of WebKit,
and rejection of a failed packager's partial output. The full portable quality
gate passed. This is a packaging-only correction: the application version and
source fingerprint remain unchanged. The startup-corrected build, before the
native-dialog theme update, had AppImage SHA-256:
`460bd51fc364d44f11ac576999f259411a9766ffe35f470f971ddad06e6106f2`.

## Native dialog theme

The generated GTK launcher inferred dark mode from the legacy GTK theme name.
Fedora reported `Adwaita` with the separate color-scheme preference `prefer-dark`,
so file pickers and the error helper appeared light. The AppImage wrapper now
defaults to `Adwaita:dark`, preserving explicit `APPIMAGE_GTK_THEME` or `GTK_THEME`
overrides. Packaging regression checks cover both overrides and repeated packaging.

The repackaged image was opened on the Fedora desktop with an isolated profile.
A controlled termination of its application child displayed the dark native error
dialog; a capture of that dialog was visually reviewed in the browser. The user
also confirmed that both the file picker and error report were dark. The full
portable quality gate passed. The image delivered to Downloads has SHA-256:
`e90f743058f59fafe71c0bfab3746887aa7d799ef0f03edfdf4dbe7afcfa5e53`.
