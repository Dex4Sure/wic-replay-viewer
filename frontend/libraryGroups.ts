import type { ReplaySummary } from './types';

export interface ReplayFolderGroup {
  path: string;
  label: string;
  rows: ReplaySummary[];
}

export function parentFolder(path: string): string {
  const normalized = path.replaceAll('\\', '/').replace(/\/+$/, '');
  const separator = normalized.lastIndexOf('/');
  if (separator < 0) {
    return '.';
  }
  if (separator === 0) {
    return '/';
  }
  return normalized.slice(0, separator);
}

export function folderLabel(path: string): string {
  if (path === '/' || path === '.') {
    return path;
  }
  return path.slice(path.lastIndexOf('/') + 1);
}

export function groupReplaysByFolder(rows: ReplaySummary[]): ReplayFolderGroup[] {
  const folders = new Map<string, ReplaySummary[]>();
  for (const row of rows) {
    const folder = parentFolder(row.path);
    const groupedRows = folders.get(folder);
    if (groupedRows) {
      groupedRows.push(row);
    } else {
      folders.set(folder, [row]);
    }
  }

  return [...folders.entries()]
    .map(([path, groupedRows]) => ({ path, label: folderLabel(path), rows: groupedRows }))
    .sort((left, right) => left.path.localeCompare(right.path, undefined, { sensitivity: 'base' }));
}
