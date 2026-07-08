import type { StatusReport } from "@/types/kaptaind";

export default function TaskProgress({ status }: { status: StatusReport }) {
  if (!status.current_task) return null;

  const percent =
    status.progress_percent == null
      ? null
      : Math.max(0, Math.min(100, status.progress_percent));

  return (
    <div className="mt-3 w-full max-w-md">
      <div className="mb-1 flex items-center justify-between text-xs text-zinc-500 dark:text-zinc-400">
        <span>{status.current_task}</span>
        {percent != null && <span>{percent}%</span>}
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-700">
        <div
          className="h-full rounded-full bg-sky-500 transition-all duration-500"
          style={{ width: percent == null ? "100%" : `${percent}%` }}
        />
      </div>
    </div>
  );
}
