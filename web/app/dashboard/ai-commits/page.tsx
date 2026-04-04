import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/lib/auth";
import { getUserTier } from "@/lib/subscription";
import { listAnalysisArtifacts } from "@/lib/kaptaind/analysis";
import ProGate from "@/components/dashboard/ProGate";
import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import Badge from "@/components/ui/Badge";
import { AiGenerateButton } from "@/components/dashboard/AiGenerateButton";

const REPO_PATH = process.env.KAPTAIND_REPO_PATH || "/home/adminx/kaptaind";

export default async function AiCommitsPage() {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) redirect("/auth/signin");

  const tier = await getUserTier(session.user.id);
  if (tier !== "pro") return <ProGate feature="AI Commit Messages" />;

  const artifacts = await listAnalysisArtifacts(REPO_PATH, 20);

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
          AI Commit Messages
        </h1>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          LLM-enhanced narrative descriptions for each automated commit
        </p>
      </div>

      {artifacts.length === 0 ? (
        <Card>
          <p className="text-sm text-zinc-500">No commits to enhance yet.</p>
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
                <span className="text-xs text-zinc-400">
                  {a.cluster_id.slice(0, 8)}
                </span>
              </CardHeader>
              <div className="mb-3 rounded-lg bg-zinc-100 p-3 font-mono text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">
                kaptaind: {a.bump} -&gt; v{a.version} [paths=
                {a.diff.touched_paths}; score={a.weight.score.toFixed(3)};
                cluster={a.cluster_id.slice(0, 8)}]
              </div>
              <div className="rounded-lg border border-violet-200 bg-violet-50 p-3 text-sm text-zinc-700 dark:border-violet-800 dark:bg-violet-900/10 dark:text-zinc-300">
                <p className="italic text-zinc-400">
                  AI narrative will be generated on demand via the API.
                  Connect the &quot;Generate&quot; button to POST /api/ai/commit-message.
                </p>
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
