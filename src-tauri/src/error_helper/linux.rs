//! GTK widgets follow the available desktop theme; no WebKit or application CSS.
use super::*;
use gtk::prelude::*;
use gtk::{ButtonsType, DialogFlags, MessageDialog, MessageType, ResponseType};

fn init() -> bool {
    gtk::init().is_ok()
}
fn message(title: &str, body: &str) -> MessageDialog {
    let dialog = MessageDialog::new(
        None::<&gtk::Window>,
        DialogFlags::empty(),
        MessageType::Warning,
        ButtonsType::None,
        title,
    );
    dialog.set_title("WiC Replay Viewer — Error report");
    dialog.set_secondary_text(Some(body));
    dialog.set_position(gtk::WindowPosition::Center);
    dialog.set_default_response(ResponseType::Cancel);
    dialog
}
fn finish(dialog: &impl IsA<gtk::Widget>) {
    unsafe {
        dialog.destroy();
    }
}
pub(super) fn notification(notice: &Notice, alive: bool) -> Action {
    if !init() {
        return Action::Wait;
    }
    let (title, body) = description(notice, alive);
    let dialog = message(title, &body);
    if alive {
        dialog.add_button("Close app…", ResponseType::Reject);
    }
    dialog.add_button("Generate local report…", ResponseType::Accept);
    dialog.add_button(
        if alive { "Keep waiting" } else { "Dismiss" },
        ResponseType::Cancel,
    );
    dialog.set_default_response(ResponseType::Cancel);
    capture_probe(&dialog, "notification");
    let response = dialog.run();
    finish(&dialog);
    match response {
        ResponseType::Accept => Action::Generate,
        ResponseType::Reject => Action::CloseApp,
        _ => Action::Wait,
    }
}
pub(super) fn information(title: &str, body: &str) {
    if !init() {
        return;
    }
    let dialog = message(title, body);
    dialog.add_button("Close", ResponseType::Cancel);
    dialog.run();
    finish(&dialog);
}
pub(super) fn confirm_close() -> bool {
    if !init() {
        return false;
    }
    let dialog = message(
        "Close WiC Replay Viewer?",
        "Closing the app stops any parsing still running. Previously saved replays are kept.",
    );
    let close = dialog.add_button("Close app", ResponseType::Accept);
    close.style_context().add_class("destructive-action");
    dialog.add_button("Keep waiting", ResponseType::Cancel);
    dialog.set_default_response(ResponseType::Cancel);
    let result = dialog.run() == ResponseType::Accept;
    finish(&dialog);
    result
}
pub(super) fn preview_report(preview: &str) -> bool {
    if !init() {
        return false;
    }
    let dialog = gtk::Dialog::with_buttons(
        Some("Error report — WiC Replay Viewer"),
        None::<&gtk::Window>,
        DialogFlags::empty(),
        &[
            ("Back", ResponseType::Cancel),
            ("Export report…", ResponseType::Accept),
        ],
    );
    dialog.set_default_size(720, 520);
    dialog.set_position(gtk::WindowPosition::Center);
    dialog.set_default_response(ResponseType::Cancel);
    let content = dialog.content_area();
    content.set_spacing(12);
    content.set_border_width(18);
    let label = gtk::Label::new(Some(
        "This is the exact JSON that will be exported. Nothing is uploaded automatically.",
    ));
    label.set_line_wrap(true);
    label.set_xalign(0.0);
    content.pack_start(&label, false, false, 0);
    let text = gtk::TextView::new();
    text.set_editable(false);
    text.set_monospace(true);
    text.set_wrap_mode(gtk::WrapMode::None);
    text.buffer().unwrap().set_text(preview);
    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroll.add(&text);
    content.pack_start(&scroll, true, true, 0);
    dialog.show_all();
    capture_probe(&dialog, "preview");
    let result = dialog.run() == ResponseType::Accept;
    finish(&dialog);
    result
}
pub(super) fn choose_export() -> Option<PathBuf> {
    let dialog = gtk::FileChooserDialog::new(
        Some("Export error report"),
        None::<&gtk::Window>,
        gtk::FileChooserAction::Save,
    );
    dialog.add_buttons(&[
        ("Cancel", ResponseType::Cancel),
        ("Export", ResponseType::Accept),
    ]);
    dialog.set_current_name("WiCReplayViewer-error-report.json");
    dialog.set_do_overwrite_confirmation(true);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Error report (*.json)"));
    filter.add_pattern("*.json");
    dialog.add_filter(filter);
    let result = if dialog.run() == ResponseType::Accept {
        dialog.filename()
    } else {
        None
    };
    finish(&dialog);
    result
}

// The probe captures its own GTK widget and responds automatically; absent from release builds.
#[cfg(debug_assertions)]
fn capture_probe(dialog: &impl IsA<gtk::Dialog>, name: &'static str) {
    let Ok(directory) = std::env::var("WIC_HELPER_TEST_OUTPUT") else {
        return;
    };
    let dialog = dialog.clone().upcast::<gtk::Dialog>();
    gtk::glib::timeout_add_local_once(Duration::from_millis(800), move || {
        if let Some(window) = dialog.window()
            && let Some(pixbuf) =
                window.pixbuf(0, 0, dialog.allocated_width(), dialog.allocated_height())
        {
            let _ = pixbuf.savev(
                std::path::Path::new(&directory).join(format!("{name}.png")),
                "png",
                &[],
            );
        }
        dialog.response(
            if name == "notification" && std::env::var_os("WIC_HELPER_TEST_DECLINE").is_none() {
                ResponseType::Accept
            } else {
                ResponseType::Cancel
            },
        );
    });
}
#[cfg(not(debug_assertions))]
fn capture_probe(_: &impl IsA<gtk::Dialog>, _: &'static str) {}
