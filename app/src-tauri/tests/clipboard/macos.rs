//! Reads the general pasteboard through AppKit's `NSPasteboard`.

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSPasteboard;
use objc2_foundation::NSString;

use crate::{Contents, Marker};

/// The marker nspasteboard.org defines for data that clipboard-history apps
/// should not record. Its presence is the signal; its data is not read.
pub const MARKERS: &[Marker] = &[Marker {
    name: "org.nspasteboard.ConcealedType",
    value: "any data",
    accepts: |_| true,
}];

/// The value of `NSPasteboardTypeString`, which objc2-app-kit only exposes
/// as an extern static, unsafe to read.
const PLAIN_TEXT: &str = "public.utf8-plain-text";

pub struct Reader;

impl Reader {
    /// Every macOS login session has a general pasteboard.
    pub fn open() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn read(&self) -> Result<Contents, String> {
        autoreleasepool(|_| {
            let pasteboard = NSPasteboard::generalPasteboard();
            let offered: Vec<String> = pasteboard
                .types()
                .map(|types| types.to_vec().iter().map(|t| t.to_string()).collect())
                .unwrap_or_default();
            let text = pasteboard
                .stringForType(&NSString::from_str(PLAIN_TEXT))
                .map(|text| text.to_string());
            let markers = MARKERS
                .iter()
                .filter(|marker| offered.iter().any(|t| t == marker.name))
                .map(|marker| {
                    let data = pasteboard
                        .dataForType(&NSString::from_str(marker.name))
                        .map(|data| data.to_vec())
                        .unwrap_or_default();
                    (marker.name, data)
                })
                .collect();
            Ok(Contents {
                text,
                offered,
                markers,
            })
        })
    }
}
