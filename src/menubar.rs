use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy,
    NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName,
    NSStatusBar, NSStatusBarButton, NSStatusItem,
};
use objc2_foundation::{
    MainThreadMarker, NSAttributedString, NSDate, NSMutableAttributedString, NSRange,
    NSRunLoop, NSString,
};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;


/// State shared between the daemon thread and the menu bar.
#[derive(Clone, Default)]
pub struct MenuBarData {
    pub session_pct: Option<f64>,
    pub weekly_pct: Option<f64>,
    pub session_over: bool,
    pub weekly_over: bool,
    /// "3h 18m", "now", or "" when unknown
    pub session_reset_str: String,
    pub weekly_reset_str: String,
}

/// Build a two-line NSAttributedString:
///   S:75% ─ 3h 18m
///   W:45% ─ 33h 18m
/// Percentage part is green or orange; rest is default color.
fn build_title(data: &MenuBarData) -> Retained<NSAttributedString> {
    let s_pct = data.session_pct.map(|p| format!("S:{:.0}%", p)).unwrap_or_else(|| "S:?".to_string());
    let w_pct = data.weekly_pct.map(|p| format!("W:{:.0}%", p)).unwrap_or_else(|| "W:?".to_string());

    let s_suffix = if data.session_reset_str.is_empty() {
        String::new()
    } else {
        format!(" ─ {}", data.session_reset_str)
    };
    let w_suffix = if data.weekly_reset_str.is_empty() {
        String::new()
    } else {
        format!(" ─ {}", data.weekly_reset_str)
    };

    let full = format!("{}{}\n{}{}", s_pct, s_suffix, w_pct, w_suffix);
    let nsstr = NSString::from_str(&full);
    let mstr = NSMutableAttributedString::from_nsstring(&nsstr);

    let s_pct_len = s_pct.encode_utf16().count();
    let line1_len = format!("{}{}", s_pct, s_suffix).encode_utf16().count();
    // +1 for the \n separator
    let w_pct_start = line1_len + 1;
    let w_pct_len = w_pct.encode_utf16().count();

    let orange: Retained<NSColor> = NSColor::systemOrangeColor();
    let green: Retained<NSColor> = NSColor::systemGreenColor();
    let orange_any: &AnyObject = &*orange;
    let green_any: &AnyObject = &*green;

    // Regular font for suffix text, bold for percentage part
    let regular: Retained<NSFont> = NSFont::systemFontOfSize(9.0);
    let bold: Retained<NSFont> = NSFont::boldSystemFontOfSize(9.0);
    let regular_any: &AnyObject = &*regular;
    let bold_any: &AnyObject = &*bold;
    let total_len = full.encode_utf16().count();

    // Paragraph style applied to the FULL string (must be consistent within each paragraph).
    // maximumLineHeight=10 + paragraphSpacingBefore=1 → 2*(1+10) = 22pt = menu bar height → centered.
    let para = NSMutableParagraphStyle::new();
    para.setParagraphSpacingBefore(1.0);
    para.setMaximumLineHeight(10.0);
    para.setMinimumLineHeight(10.0);
    let para_any: &AnyObject = &*para;

    unsafe {
        // Paragraph style across entire string (safe: consistent per paragraph)
        mstr.addAttribute_value_range(
            NSParagraphStyleAttributeName,
            para_any,
            NSRange { location: 0, length: total_len },
        );
        // Regular font across entire string
        mstr.addAttribute_value_range(
            NSFontAttributeName,
            regular_any,
            NSRange { location: 0, length: total_len },
        );
        // Bold for S:xx%
        mstr.addAttribute_value_range(
            NSFontAttributeName,
            bold_any,
            NSRange { location: 0, length: s_pct_len },
        );
        // Bold for W:xx%
        mstr.addAttribute_value_range(
            NSFontAttributeName,
            bold_any,
            NSRange { location: w_pct_start, length: w_pct_len },
        );
        // Colors
        mstr.addAttribute_value_range(
            NSForegroundColorAttributeName,
            if data.session_over { orange_any } else { green_any },
            NSRange { location: 0, length: s_pct_len },
        );
        mstr.addAttribute_value_range(
            NSForegroundColorAttributeName,
            if data.weekly_over { orange_any } else { green_any },
            NSRange { location: w_pct_start, length: w_pct_len },
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

    // Set the notification app bundle BEFORE any notification is delivered.
    // This prevents mac-notification-sys from showing a "Choose Application" dialog.
    let _ = mac_notification_sys::set_application("com.apple.Terminal");

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
