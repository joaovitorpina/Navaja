//! Reads the clipboard through Win32 directly, not through clipboard-win,
//! which arboard writes with: a bug both share could not hide a format.
#![allow(unsafe_code, reason = "Win32 FFI to read the clipboard")]

use std::io;

use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

use crate::{Contents, Marker};

/// The formats Windows documents for keeping a copy out of clipboard
/// history, cloud clipboard and clipboard monitors
/// (<https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats>).
/// The first two hold a DWORD, 0 to exclude; the third excludes with any
/// data at all.
pub const MARKERS: &[Marker] = &[
    Marker {
        name: "CanIncludeInClipboardHistory",
        value: "a DWORD 0",
        accepts: is_dword_zero,
    },
    Marker {
        name: "CanUploadToCloudClipboard",
        value: "a DWORD 0",
        accepts: is_dword_zero,
    },
    Marker {
        name: "ExcludeClipboardContentFromMonitorProcessing",
        value: "any data",
        accepts: |_| true,
    },
];

/// `GlobalSize` may report a block larger than was asked for, so only the
/// first four bytes count.
fn is_dword_zero(data: &[u8]) -> bool {
    data.get(..4) == Some(&[0; 4][..])
}

pub struct Reader;

impl Reader {
    /// Every Windows desktop session has a clipboard.
    pub fn open() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn read(&self) -> Result<Contents, String> {
        let markers = MARKERS
            .iter()
            .map(|marker| Ok((marker.name, register(marker.name)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let open = Open::new()?;
        let text = open.data(CF_UNICODETEXT.into())?.map(|bytes| utf16(&bytes));
        let mut found = Vec::new();
        for (name, format) in markers {
            if let Some(data) = open.data(format)? {
                found.push((name, data));
            }
        }
        Ok(Contents {
            text,
            offered: open.formats(),
            markers: found,
        })
    }
}

/// The format's id, the one any program that registers the name gets.
fn register(name: &str) -> Result<u32, String> {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    match unsafe { RegisterClipboardFormatW(wide.as_ptr()) } {
        0 => Err(format!(
            "RegisterClipboardFormatW({name}): {}",
            io::Error::last_os_error()
        )),
        format => Ok(format),
    }
}

/// Text from `CF_UNICODETEXT`: UTF-16, up to its NUL.
fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// The clipboard, open on this thread until dropped.
struct Open;

impl Open {
    /// Fails while another program has the clipboard open; the caller
    /// retries.
    fn new() -> Result<Self, String> {
        // SAFETY: a null owner is allowed; `Drop` closes the clipboard.
        if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
            return Err(format!("OpenClipboard: {}", io::Error::last_os_error()));
        }
        Ok(Self)
    }

    /// The data of `format`, or `None` if the clipboard has no such format.
    fn data(&self, format: u32) -> Result<Option<Vec<u8>>, String> {
        // SAFETY: the clipboard is open on this thread (`self`).
        if unsafe { IsClipboardFormatAvailable(format) } == 0 {
            return Ok(None);
        }
        // SAFETY: as above. The handle belongs to the clipboard and stays
        // valid while it is open.
        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            return Err(format!(
                "GetClipboardData({format}): {}",
                io::Error::last_os_error()
            ));
        }
        // SAFETY: `handle` is the clipboard's global memory block for
        // `format`. It is read only between GlobalLock and GlobalUnlock, and
        // GlobalSize bounds the read.
        unsafe {
            let size = GlobalSize(handle);
            let bytes = GlobalLock(handle);
            if bytes.is_null() {
                return Err(format!(
                    "GlobalLock({format}): {}",
                    io::Error::last_os_error()
                ));
            }
            let data = std::slice::from_raw_parts(bytes.cast::<u8>(), size).to_vec();
            GlobalUnlock(handle);
            Ok(Some(data))
        }
    }

    /// Every format on the clipboard: registered ones by name, standard
    /// ones by number.
    fn formats(&self) -> Vec<String> {
        let mut names = Vec::new();
        // SAFETY: the clipboard is open on this thread (`self`).
        let mut format = unsafe { EnumClipboardFormats(0) };
        while format != 0 {
            let mut name = [0u16; 256];
            // SAFETY: the buffer outlives the call, which writes at most
            // its length, NUL included.
            let len =
                unsafe { GetClipboardFormatNameW(format, name.as_mut_ptr(), name.len() as i32) };
            names.push(match usize::try_from(len) {
                Ok(len) if len > 0 => String::from_utf16_lossy(&name[..len]),
                _ => format!("#{format}"),
            });
            // SAFETY: as above.
            format = unsafe { EnumClipboardFormats(format) };
        }
        names
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: this thread opened the clipboard, and closes it once.
        unsafe { CloseClipboard() };
    }
}
