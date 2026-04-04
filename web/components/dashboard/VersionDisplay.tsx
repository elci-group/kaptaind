import Card, { CardHeader, CardTitle } from "@/components/ui/Card";

export default function VersionDisplay({
  version,
}: {
  version: string | null;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Current Version</CardTitle>
      </CardHeader>
      <p className="text-3xl font-bold text-violet-600">
        {version ? `v${version}` : "N/A"}
      </p>
    </Card>
  );
}
