# scrybe-gdocs

Google Docs integration for Scrybe: export a Markdown document to a native
Google Doc in Drive (Mermaid diagrams become PNGs that carry their source in
iTXt metadata), and import a Google Doc back to Markdown (those PNGs become
```` ```mermaid ```` fences again).

- OAuth 2.0 installed-app flow with PKCE and a loopback redirect; tokens are
  cached under the platform config dir with 0600 permissions.
- Scope `drive.file` only — Scrybe sees just the files it created, which it
  keeps in a **Scrybe** folder in My Drive.
- All HTTP goes through the `HttpClient` trait, so every test runs offline
  against `testing::FakeHttp`.

See `docs/google-docs.md` in the repository for setup (you bring your own
Google Cloud OAuth client) and the agent/CLI surface (`gdoc_auth`,
`gdoc_list`, `gdoc_export`, `gdoc_import`).
