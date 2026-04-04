"use client";

import { useEffect, useState } from "react";
import Badge from "@/components/ui/Badge";
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

  return (
    <div className="flex items-center gap-3">
      <Badge variant={config.variant}>{config.label}</Badge>
      {status.last_error && (
        <span className="text-xs text-red-500">{status.last_error}</span>
      )}
    </div>
  );
}
