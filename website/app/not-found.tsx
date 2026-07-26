import Link from "next/link";
import { release } from "./release";

export default function NotFound() {
  return (
    <main className="not-found-page">
      <header className="site-header">
        <Link className="brand" href="/" aria-label="RetrievalKit documentation home">
          <span className="brand-mark">RK</span>
          <span>RetrievalKit</span>
        </Link>
        <nav aria-label="Recovery navigation">
          <Link href="/#install">Install</Link>
          <Link href="/#languages">Languages</Link>
          <Link href="/#platform-matrix">Platforms</Link>
        </nav>
        <a className="header-cta" href={release.archiveUrl}>
          Download preview
        </a>
      </header>

      <section className="not-found" aria-labelledby="not-found-title">
        <div className="status-pill">404 · Page not found</div>
        <p className="kicker">The local index came up empty</p>
        <h1 id="not-found-title">This route is not in the corpus.</h1>
        <p>
          The page may have moved, or the address may be incomplete. Return to
          the documentation home, jump directly to the SDK guides, or use the
          platform matrix to check what is qualified today.
        </p>
        <div className="hero-actions">
          <Link className="primary-button" href="/">
            Back to documentation
          </Link>
          <Link className="secondary-button" href="/#languages">
            Browse SDK guides
          </Link>
        </div>
      </section>

      <footer>
        <div>
          <span className="brand-mark">RK</span>
          <p>RetrievalKit v0.1.0 source preview</p>
        </div>
        <p>Apache-2.0 · Local retrieval for fewer than 50K chunks</p>
      </footer>
    </main>
  );
}
