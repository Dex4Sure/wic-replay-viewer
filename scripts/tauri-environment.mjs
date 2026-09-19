const SHARED_MEMORY_SETTING = 'WEBKIT_DMABUF_RENDERER_FORCE_SHM';
const VIEWER_BACKEND_SETTING = 'WIC_REPLAY_VIEWER_GDK_BACKEND';

export function viewerEnvironment(platform, source) {
  const environment = { ...source };
  if (platform !== 'linux') {
    return environment;
  }

  // Match the native entry point: default to Wayland before spawning Tauri and
  // WebKit, with an explicit viewer-specific override for X11.
  environment.GDK_BACKEND = environment[VIEWER_BACKEND_SETTING] ?? 'wayland';
  if (environment[SHARED_MEMORY_SETTING] === undefined) {
    environment[SHARED_MEMORY_SETTING] = '1';
  }
  return environment;
}
