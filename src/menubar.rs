use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy,
    NSColor, NSForegroundColorAttributeName, NSStatusBar, NSStatusBarButton, NSStatusItem,
};
use objc2_foundation::{
    MainThreadMarker, NSAttributedString, NSDate, NSMutableAttributedString, NSRange, NSRunLoop,
    NSString,
};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;


/// State shared between the daemon thread and the menu bar.
#[derive(Clone, Default)]
pub struct MenuBarData {
    pub session_pct: Option<f64>,
    pub weekly_pct: Option<f64>,
    /// true = value exceeded the configured alert threshold
    pub session_over: bool,
    pub weekly_over: bool,
}

/// Build an NSAttributedString like  "S:72% ─ W:45%"
/// with orange color applied to parts that exceeded threshold.
fn build_title(data: &MenuBarData) -> Retained<NSAttributedString> {
    let s_str = data
        .session_pct
        .map(|p| format!("S:{:.0}%", p))
        .unwrap_or_else(|| "S:?".to_string());
    let sep = " ─ ";
    let w_str = data
        .weekly_pct
        .map(|p| format!("W:{:.0}%", p))
        .unwrap_or_else(|| "W:?".to_string());

    let full = format!("{}{}{}", s_str, sep, w_str);
    let nsstr = NSString::from_str(&full);
    let mstr = NSMutableAttributedString::from_nsstring(&nsstr);

    // Ranges (NSRange uses UTF-16 code unit counts)
    let s_len = s_str.encode_utf16().count();
    let sep_len = sep.encode_utf16().count();
    let w_len = w_str.encode_utf16().count();
    let w_start = s_len + sep_len;

    let orange: Retained<NSColor> = NSColor::systemOrangeColor();
    let green: Retained<NSColor> = NSColor::systemGreenColor();
    let orange_any: &AnyObject = &*orange;
    let green_any: &AnyObject = &*green;

    unsafe {
        mstr.addAttribute_value_range(
            NSForegroundColorAttributeName,
            if data.session_over { orange_any } else { green_any },
            NSRange { location: 0, length: s_len },
        );
        mstr.addAttribute_value_range(
            NSForegroundColorAttributeName,
            if data.weekly_over { orange_any } else { green_any },
            NSRange { location: w_start, length: w_len },
        );
    }

    // Cast NSMutableAttributedString → NSAttributedString via superclass
    Retained::into_super(mstr)
}

/// Notification request sent from the daemon thread to be delivered on the main thread.
pub struct NotifRequest {
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
}

/// Run the macOS menu bar status item on the **main thread**.
/// Blocks forever — call this as the final step of `main()`.
/// `notif_rx` receives notification requests from the background daemon thread.
pub fn run(data: Arc<Mutex<MenuBarData>>, notif_rx: Receiver<NotifRequest>) -> ! {
    let mtm = MainThreadMarker::new().expect("menubar::run must be called from the main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let status_bar = NSStatusBar::systemStatusBar();
    let item: Retained<NSStatusItem> = status_bar.statusItemWithLength(-1.0_f64);

    if let Some(btn) = item.button(mtm) {
        let btn: &NSStatusBarButton = &btn;
        btn.setTitle(&NSString::from_str("Claude…"));
    }

    loop {
        // Pump the run loop for 0.5 s
        let until = NSDate::dateWithTimeIntervalSinceNow(0.5);
        NSRunLoop::mainRunLoop().runUntilDate(&until);

        // Drain all pending notification requests (sent from daemon thread)
        while let Ok(req) = notif_rx.try_recv() {
            if let Err(e) = crate::alert::notify_mac(&req.title, &req.body, req.icon.as_deref()) {
                tracing::error!("macOS notification failed: {e}");
            }
        }

        // Update menu bar label
        let current = data.lock().unwrap().clone();
        if let Some(btn) = item.button(mtm) {
            let btn: &NSStatusBarButton = &btn;
            btn.setAttributedTitle(&build_title(&current));
        }
    }
}
