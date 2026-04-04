import { readStatus } from "@/lib/kaptaind/status";
import { listAnalysisArtifacts } from "@/lib/kaptaind/analysis";
import { readTelemetry } from "@/lib/kaptaind/telemetry";
import DaemonStatusBadge from "@/components/dashboard/DaemonStatusBadge";
import VersionDisplay from "@/components/dashboard/VersionDisplay";
import TelemetryPanel from "@/components/dashboard/TelemetryPanel";
import RecentCommitsList from "@/components/dashboard/RecentCommitsList";

const PROJECT_ID = "default";
const REPO_PATH = process.env.KAPTAIND_REPO_PATH || "/home/adminx/kaptaind";

export default async function DashboardOverview() {
  const [status, artifacts, telemetry] = await Promise.all([
    readStatus(REPO_PATH),
    listAnalysisArtifacts(REPO_PATH, 10),
    readTelemetry(REPO_PATH),
  ]);

  return (
    <div className="p-8">
      <div className="mb-8 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
            Dashboard
          </h1>
          <p className="text-sm text-zinc-500 dark:text-zinc-400">
            Overview of your Kaptaind daemon
          </p>
        </div>
        <DaemonStatusBadge
          projectId={PROJECT_ID}
          initialStatus={status}
        />
      </div>

      <div className="mb-8 grid gap-6 sm:grid-cols-2">
        <VersionDisplay version={status.last_version} />
        <TelemetryPanel telemetry={telemetry} />
      </div>

      <RecentCommitsList artifacts={artifacts} />
    </div>
  );
}
