import Link from "next/link";
import { satelliteBaseUrl } from "@/lib/satellite";
import styles from "./page.module.css";

export const dynamic = "force-dynamic";

export default function Home() {
  const base = satelliteBaseUrl();
  const quickLinks = [
    {
      href: "/workers",
      title: "Worker network",
      description: "Inspect active nodes, health lag, and satellite metadata.",
    },
    {
      href: "/contributors",
      title: "Contributor onboarding",
      description: "Register a machine against the shared satellite without identity drift.",
    },
    {
      href: "/agent",
      title: "Local agent control",
      description: "Start, stop, and reconfigure the worker running on this laptop.",
    },
    {
      href: "/files",
      title: "File operations",
      description: "Check manifests, shard placement, repair status, upload, and download paths.",
    },
  ];

  return (
      <main className={`marketing-shell ${styles.anchor}`}>
        <section className="marketing-hero">
          <div className="marketing-wrap">
            <header className="marketing-nav">
              <div className="marketing-nav-inner">
                <div className="marketing-brand">
                  <div className="marketing-brand-mark">
                    DS
                  </div>
                  <div className="marketing-brand-copy">
                    <p className="marketing-brand-title">DSPROUT</p>
                    <p className="marketing-brand-subtitle">Contributor cloud for local-first storage nodes</p>
                  </div>
                </div>

                <nav className="marketing-links">
                  <Link className="marketing-link" href="/workers">
                    Workers
                  </Link>
                  <Link className="marketing-link" href="/contributors">
                    Contributors
                  </Link>
                  <Link className="marketing-link" href="/agent">
                    Agent
                  </Link>
                  <Link className="marketing-link" href="/files">
                    Files
                  </Link>
                  <Link className="marketing-primary-link" href="/agent">
                    Launch control
                  </Link>
                </nav>
              </div>
            </header>

            <div className="marketing-hero-grid">
              <section className="marketing-copy">
                <div className="marketing-status-pill">
                  <span className="marketing-status-dot" />
                  Shared satellite online
                </div>

                <h1 className="marketing-headline">
                  Coordinate storage workers
                  <span className="marketing-headline-accent">
                    without the onboarding drift.
                  </span>
                </h1>

                <p className="marketing-subcopy">
                  DSprout gives contributors a clean local agent, a shared satellite, and one place to
                  verify worker health, registration, upload routing, and repair state across the LAN.
                </p>

                <div className="marketing-actions">
                  <Link className="marketing-cta" href="/agent">
                    Open local agent
                  </Link>
                  <Link className="marketing-secondary-link" href="/workers">
                    Inspect worker mesh
                  </Link>
                </div>

                <div className="marketing-meta">
                  <div className="marketing-meta-card">
                    <span className="marketing-meta-label">Satellite</span>
                    <span className="marketing-meta-value marketing-meta-mono">{base}</span>
                  </div>
                  <div className="marketing-meta-card">
                    <span className="marketing-meta-label">Topology</span>
                    <span className="marketing-meta-value">Shared satellite + local agent model</span>
                  </div>
                </div>
              </section>

              <section className="marketing-dashboard-shell">
                <div className="marketing-dashboard-frame">
                  <div className="marketing-dashboard">
                    <div className="marketing-dashboard-header">
                      <div>
                        <p className="marketing-dashboard-label">DSprout</p>
                        <p className="marketing-dashboard-title">Project Dashboard</p>
                      </div>
                      <div className="marketing-dashboard-badge">
                        live status
                      </div>
                    </div>

                    <div className="marketing-dashboard-grid">
                      <article className="marketing-stat-card">
                        <p className="marketing-stat-label">Active workers</p>
                        <p className="marketing-stat-value">01</p>
                        <p className="marketing-stat-copy">One verified worker registered after cleanup.</p>
                      </article>
                      <article className="marketing-stat-card">
                        <p className="marketing-stat-label">Onboarding model</p>
                        <p className="marketing-stat-value-small">Satellite-first</p>
                        <p className="marketing-stat-copy">Contributors connect local agents to a shared registry.</p>
                      </article>
                      <article className="marketing-wide-card">
                        <div className="marketing-wide-card-inner">
                          <div>
                            <p className="marketing-stat-label">Routing checks</p>
                            <p className="marketing-wide-title">
                              Registration, heartbeat, and shard metadata stay aligned.
                            </p>
                          </div>
                          <Link className="marketing-mini-cta" href="/files">
                            Review file health
                          </Link>
                        </div>
                      </article>
                    </div>
                  </div>
                </div>
              </section>
            </div>

            <section className="marketing-link-grid">
              {quickLinks.map((link) => (
                <Link key={link.href} href={link.href} className="marketing-link-card">
                  <p className="marketing-link-card-label">Workspace</p>
                  <h2 className="marketing-link-card-title">
                    {link.title}
                  </h2>
                  <p className="marketing-link-card-copy">{link.description}</p>
                </Link>
              ))}
            </section>
          </div>
        </section>
      </main>
  );
}
