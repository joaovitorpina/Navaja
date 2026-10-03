/// Running as root (effective uid 0). Navaja never needs it.
pub fn is_elevated() -> bool {
    rustix::process::geteuid().is_root()
}
