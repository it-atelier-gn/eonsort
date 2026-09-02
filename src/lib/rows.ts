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

export function nextTile(
  key: string,
  index: number,
  count: number,
  perRow: number,
): number | null {
  if (count === 0) return null;
  const across = Math.max(1, perRow);

  const target =
    key === "ArrowRight"
      ? index + 1
      : key === "ArrowLeft"
        ? index - 1
        : key === "ArrowDown"
          ? index + across
          : key === "ArrowUp"
            ? index - across
            : key === "Home"
              ? 0
              : key === "End"
                ? count - 1
                : index;

  if (target === index || target < 0) return null;
  if (target >= count) {
    return key === "ArrowDown" && index < count - 1 ? count - 1 : null;
  }
  return target;
}
