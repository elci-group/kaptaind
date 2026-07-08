"use client";

import { useEffect, useState } from "react";
import Badge from "@/components/ui/Badge";
import TaskProgress from "@/components/dashboard/TaskProgress";
import type { StatusReport, DaemonState } from "@/types/kaptaind";

const stateConfig: Record<
  DaemonState,
  { label: string; variant: "success" | "warning" | "danger" | "info" }
> = {
  Idle: { label: "Idle", variant: "success" },
  Clustering: { label: "Clustering", variant: "info" },
  Testing: { label: "Testing", variant: "warning" },
  Committing: { label: "Committing", variant: "info" },
  Failed: { label: "Failed", variant: "danger" },
};

function skyClass(status: DaemonState): string {
  switch (status) {
    case "Clustering":
    case "Testing":
      return "bg-gradient-to-br from-slate-300 to-slate-500";
    case "Committing":
      return "bg-gradient-to-b from-sky-300 via-blue-400 to-blue-600";
    case "Failed":
      return "bg-gradient-to-br from-slate-700 to-red-900";
    default:
      return "";
  }
}

export default function DaemonStatusBadge({
  projectId,
  initialStatus,
}: {
  projectId: string;
  initialStatus: StatusReport;
}) {
  const [status, setStatus] = useState<StatusReport>(initialStatus);

  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const res = await fetch(
          `/api/kaptaind/status?projectId=${projectId}`
        );
        if (res.ok) {
          setStatus(await res.json());
        }
      } catch {
        // silent retry
      }
    }, 5000);

    return () => clearInterval(interval);
  }, [projectId]);

  const config = stateConfig[status.status] ?? stateConfig.Idle;
  const ambience = skyClass(status.status);
  const isDark = status.status === "Failed";

  return (
    <div
      className={`rounded-xl border border-white/10 p-4 shadow-sm transition-colors duration-700 ${
        ambience || "bg-white dark:bg-zinc-900"
      }`}
    >
      <div className="flex items-center gap-3">
        <Badge variant={config.variant}>{config.label}</Badge>
        {status.last_error && (
          <span className={`text-xs ${isDark ? "text-red-300" : "text-red-500"}`}>
            {status.last_error}
          </span>
        )}
      </div>
      <TaskProgress status={status} />
    </div>
  );
}
