export interface ReplaySelectionModifiers {
  toggle: boolean;
  range: boolean;
}

export interface ReplaySelectionUpdate {
  paths: Set<string>;
  anchor: string;
}

/// Apply desktop file-manager selection semantics to the visible replay order.
export function updateReplaySelection(
  visiblePaths: string[],
  current: Set<string>,
  clickedPath: string,
  anchorPath: string | null,
  modifiers: ReplaySelectionModifiers,
): ReplaySelectionUpdate {
  if (modifiers.range) {
    const anchor = anchorPath && visiblePaths.includes(anchorPath) ? anchorPath : clickedPath;
    const anchorIndex = visiblePaths.indexOf(anchor);
    const clickedIndex = visiblePaths.indexOf(clickedPath);
    const start = Math.min(anchorIndex, clickedIndex);
    const end = Math.max(anchorIndex, clickedIndex);
    const paths = modifiers.toggle ? new Set(current) : new Set<string>();
    for (const path of visiblePaths.slice(start, end + 1)) {
      paths.add(path);
    }
    return { paths, anchor };
  }

  if (modifiers.toggle) {
    const paths = new Set(current);
    if (paths.has(clickedPath)) {
      paths.delete(clickedPath);
    } else {
      paths.add(clickedPath);
    }
    return { paths, anchor: clickedPath };
  }

  return { paths: new Set([clickedPath]), anchor: clickedPath };
}
