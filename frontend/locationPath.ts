/** Keep filesystem identities intact; simplify only the user-facing label. */
export function displayLocationPath(path: string): string {
  if (path.startsWith('\\\\?\\UNC\\')) {
    return '\\\\' + path.slice(8);
  }
  if (/^\\\\\?\\[a-z]:\\/i.test(path)) {
    return path.slice(4);
  }
  return path;
}
