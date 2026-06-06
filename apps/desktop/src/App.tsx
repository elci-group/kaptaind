import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";

interface DaemonStatus {
  running: boolean;
  version: string;
  uptime_seconds: number;
  watched_repos_count: number;
  active_session_id: string | null;
  schema_version: string;
}

interface VersionBump {
  version: string;
  bump_type: string;
  timestamp: string;
}

function App() {
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [bumps, setBumps] = useState<VersionBump[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      invoke<DaemonStatus>("get_daemon_status"),
      invoke<VersionBump[]>("get_recent_bumps"),
    ])
      .then(([s, b]) => {
        setStatus(s);
        setBumps(b);
      })
      .catch((err) => {
        console.error(err);
      })
      .finally(() => setLoading(false));
  }, []);

  const openDashboard = () => {
    open("http://localhost:3000").catch(console.error);
  };

  if (loading) {
    return <div style={{ padding: 24 }}>Loading...</div>;
  }

  return (
    <div style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
      <h1>Kaptaind Control Plane</h1>

      <section style={{ marginBottom: 24 }}>
        <h2>Daemon Status</h2>
        {status ? (
          <div>
            <p>
              <strong>Connection:</strong>{" "}
              <span style={{ color: status.running ? "green" : "red" }}>
                {status.running ? "Connected" : "Disconnected"}
              </span>
            </p>
            <p>
              <strong>Version:</strong> {status.version}
            </p>
            <p>
              <strong>Uptime:</strong> {status.uptime_seconds}s
            </p>
            <p>
              <strong>Watched Repos:</strong> {status.watched_repos_count}
            </p>
          </div>
        ) : (
          <p>Unable to read daemon status.</p>
        )}
      </section>

      <section style={{ marginBottom: 24 }}>
        <h2>Recent Version Bumps</h2>
        {bumps.length === 0 ? (
          <p>No recent bumps.</p>
        ) : (
          <ul>
            {bumps.map((b, i) => (
              <li key={i}>
                <strong>{b.version}</strong> ({b.bump_type}) — {b.timestamp}
              </li>
            ))}
          </ul>
        )}
      </section>

      <button
        onClick={openDashboard}
        style={{
          padding: "12px 24px",
          fontSize: 16,
          cursor: "pointer",
        }}
      >
        Open Web Dashboard
      </button>
    </div>
  );
}

export default App;
