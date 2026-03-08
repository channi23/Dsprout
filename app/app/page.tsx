import Link from "next/link";
import { satelliteBaseUrl } from "@/lib/satellite";

export const dynamic = "force-dynamic";

export default function Home() {
  const base = satelliteBaseUrl();

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">DSprout Contributor Dashboard</h1>
      <p className="mt-1 text-sm text-gray-600">Satellite: {base}</p>

      <section className="mt-8 space-y-3 text-sm">
        <p>Milestone 12 views:</p>
        <ul className="list-disc pl-5">
          <li>
            <Link className="underline" href="/workers">
              Worker List
            </Link>
          </li>
          <li>
            <Link className="underline" href="/contributors">
              Contributor Registration
            </Link>
          </li>
          <li>
            <Link className="underline" href="/files">
              File Lookup
            </Link>
          </li>
          <li>
            <Link className="underline" href="/files/upload">
              Upload Form
            </Link>
          </li>
          <li>
            <Link className="underline" href="/files/download">
              Download Form
            </Link>
          </li>
        </ul>
      </section>
    </main>
  );
}
