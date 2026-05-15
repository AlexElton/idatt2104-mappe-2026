import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { greet } from "rga-core";

const PEER_COLORS = ["#4ec9b0", "#ce9178", "#dcdcaa", "#9cdcfe", "#c586c0", "#f44747"];
const SENTINEL = "\uFEFF";

type ClientOp = {
  op: "insert" | "delete";
  pos: number;
  char?: string;
};

type InitMsg = {
  type: "init";
  site_id: number;
  text: string;
  cursors: Record<string, number>;
};

type StateMsg = {
  type: "state";
  text: string;
  cursors: Record<string, number>;
  clients: number;
};

type ServerMsg = InitMsg | StateMsg;

type Peer = {
  sid: number;
  pos: number;
};

function computeOps(oldText: string, newText: string): ClientOp[] {
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

function countContent(node: Node): number {
  if (node.nodeName === "BR") return 1;
  if (node.nodeType === Node.TEXT_NODE) return node.textContent?.length ?? 0;

  let count = 0;
  for (const child of node.childNodes) {
    count += countContent(child);
  }
  return count;
}

function getPlainText(el: HTMLElement): string {
  let text = "";
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
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

  let node = walker.nextNode();
  while (node) {
    text += node.nodeName === "BR" ? "\n" : node.textContent ?? "";
    node = walker.nextNode();
  }

  return text;
}

function getCaretOffset(el: HTMLElement): number {
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

function setCaretOffset(el: HTMLElement, offset: number) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
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

function buildEditorHtml(text: string, peers: Peer[]): string {
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

export function App() {
  const editorRef = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const syncTimerRef = useRef<number | null>(null);
  const reconnectTimerRef = useRef<number | null>(null);
  const prevTextRef = useRef("");
  const pendingOpsRef = useRef<ClientOp[]>([]);
  const siteIdRef = useRef<number | null>(null);

  const [siteId, setSiteId] = useState<number | null>(null);
  const [clientCount, setClientCount] = useState(0);
  const [cursors, setCursors] = useState<Record<string, number>>({});
  const [intervalMs, setIntervalMs] = useState(1000);
  const [countdown, setCountdown] = useState(1000);
  const [connection, setConnection] = useState<"connecting" | "connected" | "disconnected" | "error">("connecting");

  const wasmGreeting = useMemo(() => greet("world"), []);

  const applyState = useCallback(
    (newText: string, nextCursors: Record<string, number>) => {
      const editor = editorRef.current;
      if (!editor) return;

      const focused = document.activeElement === editor;
      const caretPos = focused ? getCaretOffset(editor) : null;
      const peers = Object.entries(nextCursors)
        .filter(([sid]) => Number(sid) !== siteIdRef.current)
        .map(([sid, pos]) => ({ sid: Number(sid), pos: Math.min(Number(pos), newText.length) }));

      if (newText !== prevTextRef.current) {
        editor.innerHTML = buildEditorHtml(newText, peers);
        if (focused && caretPos !== null) {
          setCaretOffset(editor, Math.min(caretPos, newText.length));
        }
        prevTextRef.current = newText;
        pendingOpsRef.current = [];
      } else {
        const localText = getPlainText(editor);
        editor.innerHTML = buildEditorHtml(localText, peers);
        if (focused && caretPos !== null) {
          setCaretOffset(editor, Math.min(caretPos, localText.length));
        }
      }

      setCursors(nextCursors);
    },
    [],
  );

  const doSync = useCallback(() => {
    const ws = wsRef.current;
    const editor = editorRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN || !editor) return;

    const currentText = getPlainText(editor).replaceAll(SENTINEL, "");
    const newOps = computeOps(prevTextRef.current, currentText);
    pendingOpsRef.current.push(...newOps);
    prevTextRef.current = currentText;

    const selection = window.getSelection();
    const hasFocus =
      document.activeElement === editor &&
      Boolean(selection?.rangeCount) &&
      selection !== null &&
      editor.contains(selection.getRangeAt(0).endContainer);

    const payload: { type: "ops"; ops: ClientOp[]; cursor?: number } = {
      type: "ops",
      ops: pendingOpsRef.current,
    };

    if (hasFocus) {
      payload.cursor = getCaretOffset(editor);
    }

    ws.send(JSON.stringify(payload));
    pendingOpsRef.current = [];
    setCountdown(intervalMs);
  }, [intervalMs]);

  useEffect(() => {
    if (syncTimerRef.current !== null) {
      window.clearInterval(syncTimerRef.current);
    }

    syncTimerRef.current = window.setInterval(doSync, intervalMs);
    setCountdown(intervalMs);

    return () => {
      if (syncTimerRef.current !== null) {
        window.clearInterval(syncTimerRef.current);
      }
    };
  }, [doSync, intervalMs]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (wsRef.current?.readyState === WebSocket.OPEN) {
        setCountdown((current) => Math.max(0, current - 100));
      }
    }, 100);

    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    let closedByEffect = false;

    function connect() {
      setConnection("connecting");
      const proto = location.protocol === "https:" ? "wss" : "ws";
      const ws = new WebSocket(`${proto}://${location.host}/ws`);
      wsRef.current = ws;

      ws.onopen = () => {
        setConnection("connected");
      };

      ws.onclose = () => {
        setConnection("disconnected");
        if (!closedByEffect) {
          reconnectTimerRef.current = window.setTimeout(connect, 2000);
        }
      };

      ws.onerror = () => {
        setConnection("error");
      };

      ws.onmessage = (event: MessageEvent<string>) => {
        const msg = JSON.parse(event.data) as ServerMsg;
        if (msg.type === "init") {
          siteIdRef.current = msg.site_id;
          setSiteId(msg.site_id);
          applyState(msg.text, msg.cursors || {});
          return;
        }

        if (msg.type === "state") {
          setClientCount(msg.clients);
          applyState(msg.text, msg.cursors || {});
        }
      };
    }

    connect();

    return () => {
      closedByEffect = true;
      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current);
      }
      wsRef.current?.close();
    };
  }, [applyState]);

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "Enter") return;

    event.preventDefault();
    const editor = editorRef.current;
    const selection = window.getSelection();
    if (!editor || !selection || !selection.rangeCount) return;

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

  const peerEntries = Object.entries(cursors).filter(([sid]) => Number(sid) !== siteId);

  return (
    <main className="shell">
      <section className="workspace" aria-label="Collaborative editor">
        <header className="topbar">
          <div>
            <p className="eyebrow">RGA over WebSocket</p>
            <h1>Collaborative Editor</h1>
          </div>
          <div className="status-strip" aria-label="Connection status">
            <span className={`status-dot ${connection}`} />
            <span>{connection}</span>
            <span>Site #{siteId ?? "?"}</span>
            <span>{clientCount || (siteId ? 1 : 0)} clients</span>
          </div>
        </header>

        <div
          ref={editorRef}
          className="editor"
          contentEditable
          suppressContentEditableWarning
          spellCheck={false}
          role="textbox"
          aria-multiline="true"
          aria-label="Shared document"
          onKeyDown={handleKeyDown}
        />

        <footer className="controlbar">
          <label className="interval-control">
            <span>Sync interval</span>
            <strong>{intervalMs}ms</strong>
            <input
              type="range"
              min="100"
              max="5000"
              step="100"
              value={intervalMs}
              onChange={(event) => setIntervalMs(Number(event.currentTarget.value))}
            />
          </label>
          <div className="sync-readout">Next sync: {(countdown / 1000).toFixed(1)}s</div>
          <div className="wasm-readout">{wasmGreeting}</div>
        </footer>
      </section>

      <aside className="peers" aria-label="Peer cursors">
        <h2>Peers</h2>
        {peerEntries.length === 0 ? (
          <p>No remote cursors</p>
        ) : (
          peerEntries.map(([sid, pos]) => (
            <div className="peer" key={sid}>
              <span
                className="peer-dot"
                style={{ backgroundColor: PEER_COLORS[Number(sid) % PEER_COLORS.length] }}
              />
              <span>Site #{sid}</span>
              <strong>@ {pos}</strong>
            </div>
          ))
        )}
      </aside>
    </main>
  );
}
