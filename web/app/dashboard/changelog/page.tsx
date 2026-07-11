import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/lib/auth";
import { getEntitlements } from "@/lib/entitlements";
import { listAocManifests } from "@/lib/kaptaind/aoc";
import ProGate from "@/components/dashboard/ProGate";
import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import Badge from "@/components/ui/Badge";
import { AiGenerateButton } from "@/components/dashboard/AiGenerateButton";

const REPO_PATH = process.env.KAPTAIND_REPO_PATH || "/home/adminx/kaptaind";

export default async function ChangelogPage() {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) redirect("/auth/signin");

  const entitlements = await getEntitlements({ userId: session.user.id });
  if (!entitlements.canUseAi)
    return <ProGate feature="Changelog Generation" />;

  const manifests = await listAocManifests(REPO_PATH, 20);

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
          Changelog Generator
        </h1>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          Generate markdown changelogs from your Aim of Change sessions
        </p>
      </div>

      {manifests.length === 0 ? (
        <Card>
          <p className="text-sm text-zinc-500">
            No shipped AoC sessions yet. Use{" "}
            <code className="rounded bg-zinc-100 px-1.5 py-0.5 text-xs dark:bg-zinc-800">
              kaptaind-cli aoc start &lt;label&gt;
            </code>{" "}
            to begin tracking.
          </p>
        </Card>
      ) : (
        <div className="space-y-4">
          {manifests.map((m) => (
            <Card key={m.id}>
              <CardHeader className="flex flex-row items-center justify-between">
                <div className="flex items-center gap-3">
                  <CardTitle>{m.label}</CardTitle>
                  <Badge variant="info">
                    v{m.initial_version} &rarr; v{m.final_version}
                  </Badge>
                </div>
                <span className="text-xs text-zinc-400">
                  {new Date(m.shipped_at).toLocaleDateString()}
                </span>
              </CardHeader>
              <div className="grid grid-cols-3 gap-4 text-sm">
                <div>
                  <span className="text-zinc-500">Clusters:</span>{" "}
                  <span className="font-medium">{m.cluster_count}</span>
                </div>
                <div>
                  <span className="text-zinc-500">Commits:</span>{" "}
                  <span className="font-medium">{m.commit_count}</span>
                </div>
                <div>
                  <span className="text-zinc-500">Test Failures:</span>{" "}
                  <span className="font-medium">{m.test_failures}</span>
                </div>
              </div>
              <div className="mt-4">
                <AiGenerateButton
                  endpoint="/api/ai/changelog"
                  payload={{
                    projectId: "default",
                    aocId: m.id,
                  }}
                  label="Generate Changelog"
                  resultLabel="Changelog"
                />
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
