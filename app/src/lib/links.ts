// The one place the front end spells the repository URL, frozen in
// docs/adr/0001-stack.md. The page never holds it as a link: openRepository
// in ipc.ts asks Rust's open_url to open it, and Rust opens only this URL
// and pages under it.
export const REPOSITORY_URL = 'https://github.com/joaovitorpina/Navaja';
