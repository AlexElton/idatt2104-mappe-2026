import type { ClientOp } from "./types";

export function computeOps(oldText: string, newText: string): ClientOp[] {
  if (oldText === newText) return [];

  const ops: ClientOp[] = [];
  let prefix = 0;

  while (prefix < oldText.length && prefix < newText.length && oldText[prefix] === newText[prefix]) {
    prefix++;
  }

  let oldEnd = oldText.length;
  let newEnd = newText.length;

  while (oldEnd > prefix && newEnd > prefix && oldText[oldEnd - 1] === newText[newEnd - 1]) {
    oldEnd--;
    newEnd--;
  }

  for (let i = oldEnd - 1; i >= prefix; i--) {
    ops.push({ op: "delete", pos: i });
  }

  for (let i = prefix; i < newEnd; i++) {
    ops.push({ op: "insert", pos: i, char: newText[i] });
  }

  return ops;
}
