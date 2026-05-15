import { useCollaborationStore } from "../stores/collaborationStore";

export function StatusStrip() {
  const connection = useCollaborationStore((state) => state.connection);
  const dotColor =
    connection === "connected"
      ? "bg-emerald-300"
      : connection === "disconnected" || connection === "error"
        ? "bg-red-500"
        : "bg-yellow-400";

  return (
    <div
      className="flex flex-col gap-2 text-sm md:flex-row md:items-center md:gap-3 md:whitespace-nowrap"
      aria-label="Connection status"
    >
      <span className={`h-2.5 w-2.5 rounded-full ${dotColor}`} />
      <span>{connection}</span>
    </div>
  );
}
