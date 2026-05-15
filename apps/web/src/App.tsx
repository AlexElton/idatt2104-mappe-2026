import { useEffect } from "react";
import { collaborationController } from "./collaboration/collaborationController";
import { ControlBar } from "./components/ControlBar";
import { EditorPane } from "./components/EditorPane";
import { PeersPanel } from "./components/PeersPanel";
import { StatusStrip } from "./components/StatusStrip";

export function App() {
  useEffect(() => {
    collaborationController.start();
    return () => collaborationController.stop();
  }, []);

  return (
    <main className="grid min-h-screen grid-cols-1 gap-4 p-3 font-mono md:grid-cols-[minmax(0,1fr)_260px] md:p-6">
      <section
        className="grid min-w-0 grid-rows-[auto_minmax(320px,1fr)_auto] border"
        aria-label="Collaborative editor"
      >
        <header className="flex flex-col gap-4 border-b p-5 md:flex-row md:items-center md:justify-between">
          <h1 className="text-xl font-bold">Collaborative Editor</h1>
          <StatusStrip />
        </header>

        <EditorPane />

        <ControlBar />
      </section>

      <PeersPanel />
    </main>
  );
}
