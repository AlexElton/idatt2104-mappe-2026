import { useCallback } from "react";
import { collaborationController } from "../collaboration/collaborationController";
import { insertManagedNewline } from "../collaboration/editorDom";

export function EditorPane() {
  const attachEditor = useCallback((editor: HTMLDivElement | null) => {
    collaborationController.attachEditor(editor);
  }, []);

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Enter") return;

    event.preventDefault();
    insertManagedNewline(event.currentTarget);
  }, []);

  return (
    <div
      ref={attachEditor}
      className="editor min-h-80 w-full overflow-auto whitespace-pre-wrap break-words p-5 leading-relaxed outline-none focus:ring-2 focus:ring-inset"
      contentEditable
      suppressContentEditableWarning
      spellCheck={false}
      role="textbox"
      aria-multiline="true"
      aria-label="Shared document"
      onKeyDown={handleKeyDown}
    />
  );
}
