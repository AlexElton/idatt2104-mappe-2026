import { useCallback } from "react";
import { collaborationController } from "../collaboration/collaborationController";

export function EditorPane() {
  const attachEditor = useCallback((editor: HTMLDivElement | null) => {
    collaborationController.attachEditor(editor);
  }, []);

  return (
    <div
      ref={attachEditor}
      className="editor min-h-80 w-full overflow-hidden leading-relaxed outline-none focus-within:ring-2 focus-within:ring-inset"
      role="textbox"
      aria-multiline="true"
      aria-label="Shared document"
    />
  );
}
