// The one place the front end spells the repository URL, frozen in
// docs/adr/0001-stack.md. The page never holds it as a link: openRepository
// in ipc.ts asks Rust's open_url to open it. Rust opens only the exact URLs
// in its own list (ALLOWED_URLS in opener.rs), today just this one, so a new
// link here needs its URL added there too.
export const REPOSITORY_URL = 'https://github.com/joaovitorpina/Navaja';
