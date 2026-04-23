use objc2::define_class;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{msg_send, sel, ClassType, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBaselineOffsetAttributeName, NSColor,
    NSEventMask, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSMenu, NSMenuItem,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSStatusBar, NSStatusBarButton,
    NSStatusItem,
};
use objc2_foundation::{
    MainThreadMarker, NSAttributedString, NSDate, NSDefaultRunLoopMode, NSMutableAttributedString,
    NSNumber, NSRange, NSString,
};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};

use crate::aggregator::DataSource;

/// State shared between the daemon thread and the menu bar.
#[derive(Clone, Default)]
pub struct MenuBarData {
    pub session_pct: Option<f64>,
    pub weekly_pct: Option<f64>,
    pub session_over: bool,
    pub weekly_over: bool,
    pub session_reset_str: String,
    pub weekly_reset_str: String,
    pub source: DataSource,
    pub has_cookie: bool,
}

/// Notification request sent from the daemon thread to be delivered on the main thread.
pub struct NotifRequest {
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
}

// ── ObjC action bridge ────────────────────────────────────────────────────────

#[allow(dead_code)]
enum MenuAction {
    SetCookie,
    ClearCookie,
    RefreshNow,
    Quit,
}

static ACTION_TX: OnceLock<Mutex<std::sync::mpsc::Sender<MenuAction>>> = OnceLock::new();

fn send_action(action: MenuAction) {
    if let Some(tx) = ACTION_TX.get() {
        let _ = tx.lock().unwrap().send(action);
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    struct MenuHandler;

    impl MenuHandler {
        #[unsafe(method(setCookieAction:))]
        fn set_cookie_action(&self, _sender: &AnyObject) {
            send_action(MenuAction::SetCookie);
        }

        #[unsafe(method(clearCookieAction:))]
        fn clear_cookie_action(&self, _sender: &AnyObject) {
            send_action(MenuAction::ClearCookie);
        }

        #[unsafe(method(refreshNowAction:))]
        fn refresh_now_action(&self, _sender: &AnyObject) {
            send_action(MenuAction::RefreshNow);
        }

        #[unsafe(method(quitAction:))]
        fn quit_action(&self, _sender: &AnyObject) {
            std::process::exit(0);
        }
    }
);

// ── Menu helpers ──────────────────────────────────────────────────────────────

fn make_item(title: &str, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            None,
            &NSString::from_str(""),
        )
    }
}

fn make_action_item(
    title: &str,
    handler: &Retained<MenuHandler>,
    sel: objc2::runtime::Sel,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(sel),
            &NSString::from_str(""),
        )
    };
    // MenuHandler → NSObject → AnyObject via deref coercions
    let target: &AnyObject = &**handler;
    unsafe { item.setTarget(Some(target)); }
    item
}

/// Show an osascript dialog to get a session cookie from the user.
/// Returns `None` if the user cancelled or entered nothing.
fn prompt_for_cookie() -> Option<String> {
    let script = concat!(
        "tell application \"System Events\" to display dialog ",
        "\"Paste your claude.ai sessionKey cookie value:\" ",
        "default answer \"\" ",
        "buttons {\"Cancel\", \"Save\"} default button \"Save\""
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None; // user cancelled
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output: "button returned:Save, text returned:VALUE\n"
    stdout
        .split("text returned:")
        .nth(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ── Title rendering ───────────────────────────────────────────────────────────

fn build_title(data: &MenuBarData) -> Retained<NSAttributedString> {
    let s_pct = data
        .session_pct
        .map(|p| format!("S:{:.0}%", p))
        .unwrap_or_else(|| "S:?".to_string());
    let w_pct = data
        .weekly_pct
        .map(|p| format!("W:{:.0}%", p))
        .unwrap_or_else(|| "W:?".to_string());

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
    let w_pct_start = line1_len + 1;
    let w_pct_len = w_pct.encode_utf16().count();

    let orange: Retained<NSColor> = NSColor::systemOrangeColor();
    let green: Retained<NSColor> = NSColor::systemGreenColor();
    let orange_any: &AnyObject = &*orange;
    let green_any: &AnyObject = &*green;

    let regular: Retained<NSFont> = NSFont::systemFontOfSize(10.5);
    let bold: Retained<NSFont> = NSFont::boldSystemFontOfSize(10.5);
    let regular_any: &AnyObject = &*regular;
    let bold_any: &AnyObject = &*bold;
    let total_len = full.encode_utf16().count();

    let para = NSMutableParagraphStyle::new();
    para.setMaximumLineHeight(12.0);
    para.setMinimumLineHeight(12.0);
    let para_any: &AnyObject = &*para;

    let baseline_offset = NSNumber::new_f64(-7.0);
    let baseline_any: &AnyObject = &*baseline_offset;

    unsafe {
        mstr.addAttribute_value_range(
            NSParagraphStyleAttributeName,
            para_any,
            NSRange { location: 0, length: total_len },
        );
        mstr.addAttribute_value_range(
            NSFontAttributeName,
            regular_any,
            NSRange { location: 0, length: total_len },
        );
        mstr.addAttribute_value_range(
            NSBaselineOffsetAttributeName,
            baseline_any,
            NSRange { location: 0, length: total_len },
        );
        mstr.addAttribute_value_range(
            NSFontAttributeName,
            bold_any,
            NSRange { location: 0, length: s_pct_len },
        );
        mstr.addAttribute_value_range(
            NSFontAttributeName,
            bold_any,
            NSRange { location: w_pct_start, length: w_pct_len },
        );
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

    Retained::into_super(mstr)
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Run the macOS menu bar on the **main thread**. Blocks forever.
pub fn run(
    data: Arc<Mutex<MenuBarData>>,
    notif_rx: Receiver<NotifRequest>,
    refresh_notify: Arc<tokio::sync::Notify>,
) -> ! {
    let mtm = MainThreadMarker::new().expect("menubar::run must be called from the main thread");

    let _ = mac_notification_sys::set_application("com.apple.Terminal");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();

    // Action channel: ObjC callbacks → Rust handler
    let (action_tx, action_rx) = std::sync::mpsc::channel::<MenuAction>();
    ACTION_TX.set(Mutex::new(action_tx)).ok();

    // ObjC action handler object
    let handler: Retained<MenuHandler> = unsafe { msg_send![MenuHandler::class(), new] };

    // Status item
    let status_bar = NSStatusBar::systemStatusBar();
    let item: Retained<NSStatusItem> = status_bar.statusItemWithLength(-1.0_f64);
    if let Some(btn) = item.button(mtm) {
        let btn: &NSStatusBarButton = &btn;
        btn.setTitle(&NSString::from_str("Claude…"));
    }

    // Build dropdown menu
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);

    let source_item = make_item("Source: OAuth", mtm);
    source_item.setEnabled(false);
    menu.addItem(&source_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let set_item = make_action_item("Set Session Cookie…", &handler, sel!(setCookieAction:), mtm);
    menu.addItem(&set_item);

    let clear_item =
        make_action_item("Clear Session Cookie", &handler, sel!(clearCookieAction:), mtm);
    clear_item.setHidden(true);
    menu.addItem(&clear_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let refresh_item = make_action_item("Refresh Now", &handler, sel!(refreshNowAction:), mtm);
    menu.addItem(&refresh_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let quit_item = make_action_item("Quit", &handler, sel!(quitAction:), mtm);
    menu.addItem(&quit_item);

    item.setMenu(Some(&menu));

    let mut prev_has_cookie = false;

    loop {
        // Pump AppKit events for up to 0.5 s — this is the correct way to dispatch
        // mouse-click events (including status-item clicks that open the dropdown).
        // NSRunLoop::runUntilDate only processes low-level run-loop sources and does NOT
        // call NSApplication::sendEvent, so menus would never appear with that approach.
        let until = NSDate::dateWithTimeIntervalSinceNow(0.5);
        unsafe {
            // Blocking wait: returns when an event arrives or the 0.5 s timeout expires.
            if let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&until),
                NSDefaultRunLoopMode,
                true,
            ) {
                app.sendEvent(&event);
            }
            // Drain any remaining queued events without blocking further.
            let now = NSDate::dateWithTimeIntervalSinceNow(0.0);
            while let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&now),
                NSDefaultRunLoopMode,
                true,
            ) {
                app.sendEvent(&event);
            }
        }

        // Deliver pending notifications
        while let Ok(req) = notif_rx.try_recv() {
            if let Err(e) = crate::alert::notify_mac(&req.title, &req.body, req.icon.as_deref()) {
                tracing::error!("macOS notification failed: {e}");
            }
        }

        // Handle menu actions
        while let Ok(action) = action_rx.try_recv() {
            match action {
                MenuAction::SetCookie => {
                    if let Some(cookie) = prompt_for_cookie() {
                        match crate::config::load() {
                            Ok(mut cfg) => {
                                cfg.session_cookie = Some(cookie);
                                if let Err(e) = crate::config::save(&cfg) {
                                    tracing::error!("Failed to save config: {e}");
                                } else {
                                    refresh_notify.notify_one();
                                }
                            }
                            Err(e) => tracing::error!("Failed to load config: {e}"),
                        }
                    }
                }
                MenuAction::ClearCookie => {
                    match crate::config::load() {
                        Ok(mut cfg) => {
                            cfg.session_cookie = None;
                            if let Err(e) = crate::config::save(&cfg) {
                                tracing::error!("Failed to save config: {e}");
                            } else {
                                refresh_notify.notify_one();
                            }
                        }
                        Err(e) => tracing::error!("Failed to load config: {e}"),
                    }
                }
                MenuAction::RefreshNow => {
                    refresh_notify.notify_one();
                }
                MenuAction::Quit => std::process::exit(0),
            }
        }

        // Update menu bar state
        let current = data.lock().unwrap().clone();

        let source_label = match current.source {
            DataSource::WebCookie => "Source: Web Cookie ✓",
            DataSource::OAuth => "Source: OAuth",
        };
        source_item.setTitle(&NSString::from_str(source_label));

        if current.has_cookie != prev_has_cookie {
            clear_item.setHidden(!current.has_cookie);
            prev_has_cookie = current.has_cookie;
        }

        if let Some(btn) = item.button(mtm) {
            let btn: &NSStatusBarButton = &btn;
            btn.setAttributedTitle(&build_title(&current));
        }
    }
}
