export function parentDirectory(path: string): string | null {
  const separator = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (separator < 0) {
    return null;
  }
  if (separator === 0) {
    return path.slice(0, 1);
  }
  if (separator === 2 && path[1] === ':') {
    return path.slice(0, 3);
  }
  return path.slice(0, separator);
}

export function exportDefaultPath(sourcePath: string, directory: string): string {
  const fileSeparator = Math.max(sourcePath.lastIndexOf('/'), sourcePath.lastIndexOf('\\'));
  const fileName = sourcePath.slice(fileSeparator + 1);
  const separator = directory.includes('\\') && !directory.includes('/') ? '\\' : '/';
  return `${directory}${directory.endsWith('/') || directory.endsWith('\\') ? '' : separator}${fileName}`;
}
