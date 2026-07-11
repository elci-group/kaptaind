import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/lib/auth";
import { getEntitlements } from "@/lib/entitlements";
import { listAnalysisArtifacts } from "@/lib/kaptaind/analysis";
import { readTelemetry } from "@/lib/kaptaind/telemetry";
import ProGate from "@/components/dashboard/ProGate";
import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import { Table, Thead, Th, Td } from "@/components/ui/Table";
import { prisma } from "@/lib/prisma";

const REPO_PATH = process.env.KAPTAIND_REPO_PATH || "/home/adminx/kaptaind";

export default async function TeamPage() {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) redirect("/auth/signin");

  const entitlements = await getEntitlements({ userId: session.user.id });
  if (entitlements.maxUsers <= 1) return <ProGate feature="Team Dashboard" />;

  const [members, artifacts, telemetry] = await Promise.all([
    prisma.teamMembership.findMany({
      where: { projectId: "default" },
      include: { user: { select: { name: true, email: true, image: true } } },
    }),
    listAnalysisArtifacts(REPO_PATH, 50),
    readTelemetry(REPO_PATH),
  ]);

  // Simple daily commit velocity from artifacts
  const dailyCounts: Record<string, number> = {};
  for (const a of artifacts) {
    const day = new Date(a.ended_at).toISOString().slice(0, 10);
    dailyCounts[day] = (dailyCounts[day] || 0) + 1;
  }
  const velocityDays = Object.entries(dailyCounts)
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(-14);

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
          Team Dashboard
        </h1>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          Team analytics and commit velocity
        </p>
      </div>

      <div className="mb-8 grid gap-6 sm:grid-cols-3">
        <Card>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">Members</p>
          <p className="text-3xl font-bold text-zinc-900 dark:text-zinc-100">
            {members.length}
          </p>
        </Card>
        <Card>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">
            Total Commits
          </p>
          <p className="text-3xl font-bold text-violet-600">
            {artifacts.length}
          </p>
        </Card>
        <Card>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">
            Total Token Cost
          </p>
          <p className="text-3xl font-bold text-zinc-900 dark:text-zinc-100">
            ${telemetry.aggregate_cost.toFixed(4)}
          </p>
        </Card>
      </div>

      {/* Velocity chart (simple ASCII-style bar chart) */}
      <Card className="mb-8">
        <CardHeader>
          <CardTitle>Commit Velocity (last 14 days)</CardTitle>
        </CardHeader>
        {velocityDays.length === 0 ? (
          <p className="text-sm text-zinc-500">No data yet.</p>
        ) : (
          <div className="space-y-1">
            {velocityDays.map(([day, count]) => (
              <div key={day} className="flex items-center gap-3">
                <span className="w-24 text-xs font-mono text-zinc-400">
                  {day}
                </span>
                <div className="flex-1">
                  <div
                    className="h-4 rounded bg-violet-500"
                    style={{
                      width: `${Math.min(
                        (count /
                          Math.max(
                            ...velocityDays.map(([, c]) => c),
                            1
                          )) *
                          100,
                        100
                      )}%`,
                    }}
                  />
                </div>
                <span className="w-6 text-right text-xs font-mono text-zinc-600 dark:text-zinc-400">
                  {count}
                </span>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* Members table */}
      <Card className="p-0">
        <div className="p-6 pb-0">
          <CardTitle>Team Members</CardTitle>
        </div>
        <div className="mt-4">
          <Table>
            <Thead>
              <tr>
                <Th>Name</Th>
                <Th>Email</Th>
                <Th>Role</Th>
                <Th>Joined</Th>
              </tr>
            </Thead>
            <tbody className="divide-y divide-zinc-200 dark:divide-zinc-800">
              {members.map((m) => (
                <tr key={m.id}>
                  <Td>{m.user.name || "Unknown"}</Td>
                  <Td>{m.user.email}</Td>
                  <Td>
                    <span className="capitalize">{m.role}</span>
                  </Td>
                  <Td>{new Date(m.joinedAt).toLocaleDateString()}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </div>
      </Card>
    </div>
  );
}
