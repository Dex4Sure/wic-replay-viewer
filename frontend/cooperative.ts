/** Yield through a task (not a microtask), allowing input and painting between chunks. */
export const yieldToUi = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));
export const LIBRARY_CHUNK_SIZE = 256;

/** Stable merge sort with bounded runs and merge steps; never sort the entire library at once. */
export async function cooperativeSort<T>(
  values: T[],
  compare: (left: T, right: T) => number,
  yieldTask = yieldToUi,
): Promise<T[]> {
  let source: T[] = [];
  for (let start = 0; start < values.length; start += LIBRARY_CHUNK_SIZE) {
    source.push(...values.slice(start, start + LIBRARY_CHUNK_SIZE).sort(compare));
    if (values.length > LIBRARY_CHUNK_SIZE) {
      await yieldTask();
    }
  }
  for (let width = LIBRARY_CHUNK_SIZE; width < source.length; width *= 2) {
    const target: T[] = [];
    for (let start = 0; start < source.length; start += width * 2) {
      const middle = Math.min(start + width, source.length);
      const end = Math.min(start + width * 2, source.length);
      let left = start;
      let right = middle;
      while (left < middle || right < end) {
        target.push(
          right >= end || (left < middle && compare(source[left]!, source[right]!) <= 0)
            ? source[left++]!
            : source[right++]!,
        );
        if (target.length % LIBRARY_CHUNK_SIZE === 0) {
          await yieldTask();
        }
      }
    }
    source = target;
  }
  return source;
}
