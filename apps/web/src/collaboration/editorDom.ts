import type { Peer } from "./types";
import { PEER_COLORS } from "./types";

export const SENTINEL = "\uFEFF";

function countContent(node: Node): number {
  if (node.nodeName === "BR") return 1;
  if (node.nodeType === Node.TEXT_NODE) return node.textContent?.length ?? 0;

  let count = 0;
  for (const child of node.childNodes) {
    count += countContent(child);
  }
  return count;
}

function contentWalker(el: HTMLElement): TreeWalker {
  return document.createTreeWalker(el, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
    acceptNode(node) {
      if (node.nodeType === Node.ELEMENT_NODE) {
        const element = node as HTMLElement;
        if (element.classList?.contains("peer-cursor")) return NodeFilter.FILTER_REJECT;
        if (node.nodeName === "BR") return NodeFilter.FILTER_ACCEPT;
        return NodeFilter.FILTER_SKIP;
      }

      const parent = (node as Text).parentElement;
      if (parent?.classList.contains("peer-cursor")) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
}

export function getPlainText(el: HTMLElement): string {
  let text = "";
  const walker = contentWalker(el);

  let node = walker.nextNode();
  while (node) {
    text += node.nodeName === "BR" ? "\n" : node.textContent ?? "";
    node = walker.nextNode();
  }

  return text;
}

export function getCaretOffset(el: HTMLElement): number {
  const selection = window.getSelection();
  if (!selection || !selection.rangeCount) return 0;

  const range = selection.getRangeAt(0);
  if (!el.contains(range.endContainer)) return 0;

  const offsetRange = document.createRange();
  offsetRange.setStart(el, 0);
  offsetRange.setEnd(range.endContainer, range.endOffset);

  const fragment = offsetRange.cloneContents();
  fragment.querySelectorAll(".peer-cursor").forEach((cursor) => cursor.remove());
  return countContent(fragment);
}

export function setCaretOffset(el: HTMLElement, offset: number) {
  const walker = contentWalker(el);
  let remaining = offset;
  let node = walker.nextNode();

  while (node) {
    if (node.nodeName === "BR") {
      if (remaining === 0) {
        const range = document.createRange();
        range.setStartBefore(node);
        range.collapse(true);
        const selection = window.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
        return;
      }
      remaining -= 1;
    } else {
      const length = node.textContent?.length ?? 0;
      if (remaining <= length) {
        const range = document.createRange();
        range.setStart(node, remaining);
        range.collapse(true);
        const selection = window.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
        return;
      }
      remaining -= length;
    }

    node = walker.nextNode();
  }

  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

export function buildEditorHtml(text: string, peers: Peer[]): string {
  const sorted = [...peers].sort((a, b) => a.pos - b.pos);
  let html = "";
  let peerIndex = 0;

  for (let i = 0; i <= text.length; i++) {
    while (peerIndex < sorted.length && sorted[peerIndex].pos === i) {
      const { sid } = sorted[peerIndex];
      const color = PEER_COLORS[sid % PEER_COLORS.length];
      html += `<span class="peer-cursor" data-site="${sid}" contenteditable="false" style="color:${color}" title="Site #${sid}"></span>`;
      peerIndex++;
    }

    if (i < text.length) {
      const ch = text[i];
      if (ch === "&") html += "&amp;";
      else if (ch === "<") html += "&lt;";
      else if (ch === ">") html += "&gt;";
      else if (ch === "\n") html += "&#10;";
      else html += ch;
    }
  }

  return html;
}

export function insertManagedNewline(editor: HTMLElement) {
  const selection = window.getSelection();
  if (!selection || !selection.rangeCount) return;

  const range = selection.getRangeAt(0);
  if (!range.collapsed) range.deleteContents();

  const container = range.startContainer;
  const offset = range.startOffset;

  if (container.nodeType === Node.TEXT_NODE) {
    (container as Text).insertData(offset, `\n${SENTINEL}`);
    const nextRange = document.createRange();
    nextRange.setStart(container, offset + 1);
    nextRange.collapse(true);
    selection.removeAllRanges();
    selection.addRange(nextRange);
  } else {
    const newline = document.createTextNode(`\n${SENTINEL}`);
    const refNode = container.childNodes[offset] || null;
    container.insertBefore(newline, refNode);
    const nextRange = document.createRange();
    nextRange.setStart(newline, 1);
    nextRange.collapse(true);
    selection.removeAllRanges();
    selection.addRange(nextRange);
  }

  editor.addEventListener(
    "input",
    () => {
      const walker = document.createTreeWalker(editor, NodeFilter.SHOW_TEXT);
      let node = walker.nextNode();
      while (node) {
        const index = (node as Text).data.indexOf(SENTINEL);
        if (index !== -1) {
          (node as Text).deleteData(index, 1);
          break;
        }
        node = walker.nextNode();
      }
    },
    { once: true },
  );
}
