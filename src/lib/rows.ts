export function nextRow(key: string, index: number, count: number): number | null {
  if (count === 0) return null;

  const target =
    key === "ArrowDown"
      ? index + 1
      : key === "ArrowUp"
        ? index - 1
        : key === "Home"
          ? 0
          : key === "End"
            ? count - 1
            : index;

  if (target === index || target < 0 || target >= count) return null;
  return target;
}
