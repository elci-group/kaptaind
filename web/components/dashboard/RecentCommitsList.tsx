import Badge from "@/components/ui/Badge";
import { Table, Thead, Th, Td } from "@/components/ui/Table";
import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import type { AnalysisArtifact } from "@/types/kaptaind";

const bumpBadge: Record<string, { label: string; variant: "success" | "warning" | "danger" | "info" }> = {
  Major: { label: "Major", variant: "danger" },
  Minor: { label: "Minor", variant: "info" },
  Patch: { label: "Patch", variant: "success" },
  None: { label: "None", variant: "default" as "info" },
};

export default function RecentCommitsList({
  artifacts,
}: {
  artifacts: AnalysisArtifact[];
}) {
  if (artifacts.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Recent Commits</CardTitle>
        </CardHeader>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          No analysis history yet. The daemon will create entries when it
          processes file changes.
        </p>
      </Card>
    );
  }

  return (
    <Card className="p-0">
      <div className="p-6 pb-0">
        <CardTitle>Recent Commits</CardTitle>
      </div>
      <div className="mt-4">
        <Table>
          <Thead>
            <tr>
              <Th>Version</Th>
              <Th>Bump</Th>
              <Th>Score</Th>
              <Th>Paths</Th>
              <Th>Events</Th>
              <Th>Date</Th>
              <Th>Cluster</Th>
            </tr>
          </Thead>
          <tbody className="divide-y divide-zinc-200 dark:divide-zinc-800">
            {artifacts.map((a) => {
              const b = bumpBadge[a.bump] ?? bumpBadge.None;
              return (
                <tr key={a.cluster_id}>
                  <Td>
                    <span className="font-mono font-medium text-violet-600">
                      v{a.version}
                    </span>
                  </Td>
                  <Td>
                    <Badge variant={b.variant}>{b.label}</Badge>
                  </Td>
                  <Td>{a.weight.score.toFixed(3)}</Td>
                  <Td>{a.diff.touched_paths}</Td>
                  <Td>{a.event_count}</Td>
                  <Td>
                    {new Date(a.ended_at).toLocaleString(undefined, {
                      month: "short",
                      day: "numeric",
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </Td>
                  <Td>
                    <span className="font-mono text-xs text-zinc-400">
                      {a.cluster_id.slice(0, 8)}
                    </span>
                  </Td>
                </tr>
              );
            })}
          </tbody>
        </Table>
      </div>
    </Card>
  );
}
