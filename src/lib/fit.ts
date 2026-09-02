export interface Size {
  width: number;
  height: number;
}

export function fittedTo(natural: Size, room: Size, swapped: boolean): Size | null {
  if (!(natural.width > 0) || !(natural.height > 0)) return null;
  const across = swapped ? room.height : room.width;
  const down = swapped ? room.width : room.height;
  if (!(across > 0) || !(down > 0)) return null;
  const by = Math.min(across / natural.width, down / natural.height, 1);
  return { width: natural.width * by, height: natural.height * by };
}
