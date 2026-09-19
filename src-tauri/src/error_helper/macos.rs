//! AppKit preview, independent of WKWebView.
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSAlert, NSScrollView, NSTextView};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

pub(super) fn preview_report(preview: &str) -> bool {
    let Some(main) = MainThreadMarker::new() else {
        return false;
    };
    let alert = NSAlert::new(main);
    alert.setMessageText(&NSString::from_str("Error report"));
    alert.setInformativeText(&NSString::from_str(
        "This is the exact JSON that will be exported. Nothing is uploaded automatically.",
    ));
    alert.addButtonWithTitle(&NSString::from_str("Back"));
    alert.addButtonWithTitle(&NSString::from_str("Export report…"));
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(640.0, 360.0));
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(main), frame);
    scroll.setHasVerticalScroller(true);
    scroll.setHasHorizontalScroller(true);
    let text = NSTextView::initWithFrame(NSTextView::alloc(main), frame);
    text.setEditable(false);
    text.setString(&NSString::from_str(preview));
    scroll.setDocumentView(Some(&text));
    alert.setAccessoryView(Some(&scroll));
    alert.runModal() == 1001
}
