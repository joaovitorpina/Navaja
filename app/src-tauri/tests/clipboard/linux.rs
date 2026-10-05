//! Reads the CLIPBOARD selection over its own X11 connection, the way a
//! clipboard manager such as Klipper asks for it.

use std::thread;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, GetPropertyReply, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME, NONE};

use crate::{Contents, Marker};

/// The target KDE defines for password managers' copies. Klipper and other
/// history managers skip a copy that offers it with the value `secret`.
pub const MARKERS: &[Marker] = &[Marker {
    name: "x-kde-passwordManagerHint",
    value: "\"secret\"",
    accepts: |data| data == b"secret",
}];

/// How long the selection's owner gets to answer one request.
const ANSWER: Duration = Duration::from_secs(2);

/// How to run the check where it would otherwise skip.
const HOW: &str = "run it under X11, e.g. `env -u WAYLAND_DISPLAY xvfb-run -a cargo nextest run -p navaja --test clipboard`";

pub struct Reader {
    conn: RustConnection,
    window: Window,
    clipboard: Atom,
    targets: Atom,
    utf8: Atom,
    /// Where the owner puts each answer, on `window`.
    property: Atom,
}

fn message<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

impl Reader {
    /// An error is why this machine has no X11 clipboard to check, and the
    /// test skips.
    ///
    /// With `WAYLAND_DISPLAY` set, arboard 3.6 copies over Wayland only if
    /// the compositor offers a data-control protocol (KDE, wlroots), which
    /// it learns from wl-clipboard-rs's `is_primary_selection_supported`.
    /// Otherwise it falls back to X11, through Xwayland (GNOME). This runs
    /// the same probe, so it skips only where the copy really goes over
    /// Wayland, rather than read through Xwayland's bridge what arboard
    /// wrote there, and checks the X11 copy GNOME users get. It must follow
    /// arboard's choice when arboard is updated.
    pub fn open() -> Result<Self, String> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && wl_clipboard_rs::utils::is_primary_selection_supported().is_ok()
        {
            return Err(format!(
                "the Wayland compositor offers data-control, so arboard copies over Wayland, and this check reads X11; {HOW}"
            ));
        }
        if std::env::var_os("DISPLAY").is_none() {
            return Err(format!("no X display (DISPLAY is unset); {HOW}"));
        }
        let (conn, screen) = x11rb::connect(None)
            .map_err(|e| format!("cannot connect to the X display ({e}); {HOW}"))?;

        // From here on the display exists, so a failure fails the test.
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().expect("an X11 id");
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &CreateWindowAux::new(),
        )
        .expect("an X11 window")
        .check()
        .expect("an X11 window");
        let mut reader = Self {
            conn,
            window,
            clipboard: NONE,
            targets: NONE,
            utf8: NONE,
            property: NONE,
        };
        reader.clipboard = reader.atom("CLIPBOARD").expect("the CLIPBOARD atom");
        reader.targets = reader.atom("TARGETS").expect("the TARGETS atom");
        reader.utf8 = reader.atom("UTF8_STRING").expect("the UTF8_STRING atom");
        reader.property = reader.atom("NAVAJA_TEST_CLIPBOARD").expect("an atom");
        Ok(reader)
    }

    pub fn read(&self) -> Result<Contents, String> {
        // No owner, or one that lists nothing: nothing copied yet.
        let Some(reply) = self.convert(self.targets)? else {
            return Ok(Contents::default());
        };
        let targets: Vec<Atom> = reply
            .value32()
            .ok_or("TARGETS answered with something other than atoms")?
            .collect();
        let offered = targets
            .iter()
            .map(|&target| self.name(target))
            .collect::<Result<_, _>>()?;
        let text = self
            .convert(self.utf8)?
            .map(|reply| String::from_utf8_lossy(&reply.value).into_owned());
        let mut markers = Vec::new();
        for marker in MARKERS {
            let target = self.atom(marker.name)?;
            if targets.contains(&target) {
                let data = self.convert(target)?.map(|reply| reply.value);
                markers.push((marker.name, data.unwrap_or_default()));
            }
        }
        Ok(Contents {
            text,
            offered,
            markers,
        })
    }

    fn atom(&self, name: &str) -> Result<Atom, String> {
        Ok(self
            .conn
            .intern_atom(false, name.as_bytes())
            .map_err(message)?
            .reply()
            .map_err(message)?
            .atom)
    }

    fn name(&self, atom: Atom) -> Result<String, String> {
        let reply = self
            .conn
            .get_atom_name(atom)
            .map_err(message)?
            .reply()
            .map_err(message)?;
        Ok(String::from_utf8_lossy(&reply.name).into_owned())
    }

    /// Asks the CLIPBOARD's owner for `target`. `None` if there is no owner
    /// or it refuses.
    fn convert(&self, target: Atom) -> Result<Option<GetPropertyReply>, String> {
        self.conn
            .convert_selection(
                self.window,
                self.clipboard,
                target,
                self.property,
                CURRENT_TIME,
            )
            .map_err(message)?;
        self.conn.flush().map_err(message)?;
        let deadline = Instant::now() + ANSWER;
        loop {
            match self.conn.poll_for_event().map_err(message)? {
                Some(Event::SelectionNotify(event))
                    if event.requestor == self.window && event.target == target =>
                {
                    if event.property == NONE {
                        return Ok(None);
                    }
                    let reply = self
                        .conn
                        .get_property(
                            true,
                            self.window,
                            self.property,
                            AtomEnum::ANY,
                            0,
                            u32::MAX / 4,
                        )
                        .map_err(message)?
                        .reply()
                        .map_err(message)?;
                    return Ok(Some(reply));
                }
                Some(_) => {}
                None if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                None => {
                    return Err(format!(
                        "the clipboard's owner did not answer within {ANSWER:?}"
                    ));
                }
            }
        }
    }
}
