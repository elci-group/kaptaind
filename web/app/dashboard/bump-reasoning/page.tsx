import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/lib/auth";
import { getEntitlements } from "@/lib/entitlements";
import { listAnalysisArtifacts } from "@/lib/kaptaind/analysis";
import ProGate from "@/components/dashboard/ProGate";
import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import Badge from "@/components/ui/Badge";
import { AiGenerateButton } from "@/components/dashboard/AiGenerateButton";

const REPO_PATH = process.env.KAPTAIND_REPO_PATH || "/home/adminx/kaptaind";

export default async function BumpReasoningPage() {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) redirect("/auth/signin");

  const entitlements = await getEntitlements({ userId: session.user.id });
  if (!entitlements.canUseAi) return <ProGate feature="Bump Reasoning" />;

  const artifacts = await listAnalysisArtifacts(REPO_PATH, 20);

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
          Bump Reasoning
        </h1>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          AI-powered explanations for each semantic version decision
        </p>
      </div>

      {artifacts.length === 0 ? (
        <Card>
          <p className="text-sm text-zinc-500">No analysis artifacts yet.</p>
        </Card>
      ) : (
        <div className="space-y-4">
          {artifacts.map((a) => (
            <Card key={a.cluster_id}>
              <CardHeader className="flex flex-row items-center justify-between">
                <div className="flex items-center gap-3">
                  <CardTitle>v{a.version}</CardTitle>
                  <Badge
                    variant={
                      a.bump === "Major"
                        ? "danger"
                        : a.bump === "Minor"
                          ? "info"
                          : "success"
                    }
                  >
                    {a.bump}
                  </Badge>
                </div>
                <span className="text-xs text-zinc-400 font-mono">
                  {a.cluster_id.slice(0, 8)}
                </span>
              </CardHeader>

              {/* Score breakdown */}
              <div className="mb-4 space-y-2">
                <ScoreBar
                  label="Structural"
                  value={a.diff.structural}
                  max={1}
                  color="bg-blue-500"
                />
                <ScoreBar
                  label="API"
                  value={a.diff.api}
                  max={1}
                  color="bg-violet-500"
                />
                <ScoreBar
                  label="Dependencies"
                  value={a.diff.deps}
                  max={1}
                  color="bg-amber-500"
                />
                <ScoreBar
                  label="Runtime"
                  value={a.diff.runtime}
                  max={1}
                  color="bg-emerald-500"
                />
              </div>

              <div className="flex items-center justify-between text-sm">
                <span className="text-zinc-500">
                  Weighted score:{" "}
                  <span className="font-mono font-semibold text-zinc-900 dark:text-zinc-100">
                    {a.weight.score.toFixed(3)}
                  </span>
                </span>
                <div className="flex gap-2">
                  {a.diff.api_breaking && (
                    <Badge variant="danger">API Breaking</Badge>
                  )}
                  {a.diff.api_added && (
                    <Badge variant="info">API Added</Badge>
                  )}
                </div>
              </div>

              <div className="mt-4">
                <AiGenerateButton
                  endpoint="/api/ai/bump-reasoning"
                  payload={{
                    projectId: "default",
                    clusterId: a.cluster_id,
                  }}
                  label="Explain Bump"
                  resultLabel="Reasoning"
                />
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

function ScoreBar({
  label,
  value,
  max,
  color,
}: {
  label: string;
  value: number;
  max: number;
  color: string;
}) {
  const pct = Math.min((value / max) * 100, 100);

  return (
    <div className="flex items-center gap-3">
      <span className="w-28 text-xs text-zinc-500 dark:text-zinc-400">
        {label}
      </span>
      <div className="flex-1 rounded-full bg-zinc-200 dark:bg-zinc-700 h-2">
        <div
          className={`h-2 rounded-full ${color}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="w-12 text-right text-xs font-mono text-zinc-600 dark:text-zinc-400">
        {value.toFixed(3)}
      </span>
    </div>
  );
}
