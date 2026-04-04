import { redirect } from "next/navigation";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { getUserTier } from "@/lib/subscription";
import Sidebar from "@/components/layout/Sidebar";

export default async function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const session = await getServerSession(authOptions);

  if (!session?.user?.id) {
    redirect("/auth/signin");
  }

  const tier = await getUserTier(session.user.id);

  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar tier={tier} />
      <main className="flex-1 overflow-y-auto bg-zinc-100 dark:bg-zinc-900">
        {children}
      </main>
    </div>
  );
}
