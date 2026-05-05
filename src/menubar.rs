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

/// Per-account usage data shared between daemon and menu bar.
#[derive(Clone, Default)]
pub struct AccountData {
    pub id: String,
    pub name: String,
    pub session_pct: Option<f64>,
    pub weekly_pct: Option<f64>,
    pub session_over: bool,
    pub weekly_over: bool,
    pub session_reset_str: String,
    pub weekly_reset_str: String,
    pub source: DataSource,
    pub error: Option<String>,
}

/// State shared between the daemon thread and the menu bar.
#[derive(Clone, Default)]
pub struct MenuBarData {
    pub accounts: Vec<AccountData>,
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
    AddAccount,
    RemoveAccount(String),
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
        #[unsafe(method(addAccountAction:))]
        fn add_account_action(&self, _sender: &AnyObject) {
            send_action(MenuAction::AddAccount);
        }

        #[unsafe(method(refreshNowAction:))]
        fn refresh_now_action(&self, _sender: &AnyObject) {
            send_action(MenuAction::RefreshNow);
        }

        #[unsafe(method(quitAction:))]
        fn quit_action(&self, _sender: &AnyObject) {
            std::process::exit(0);
        }

        #[unsafe(method(removeAccountAction:))]
        fn remove_account_action(&self, _sender: &AnyObject) {
            // Account ID passed via represented object is not trivial in objc2,
            // so we use a dialog to let user pick which account to remove.
            send_action(MenuAction::RemoveAccount(String::new()));
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
    let target: &AnyObject = &**handler;
    unsafe { item.setTarget(Some(target)); }
    item
}

/// Show an osascript dialog to get text input from the user.
fn prompt_dialog(message: &str, default: &str) -> Option<String> {
    let script = format!(
        "tell application \"System Events\" to display dialog \"{}\" default answer \"{}\" buttons {{\"Cancel\", \"OK\"}} default button \"OK\"",
        message.replace('"', "\\\""),
        default.replace('"', "\\\""),
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split("text returned:")
        .nth(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Show a button-choice dialog. Returns the button text user clicked, or None on cancel.
fn choice_dialog(message: &str, buttons: &[&str]) -> Option<String> {
    let btns = buttons.iter().map(|b| format!("\"{}\"", b)).collect::<Vec<_>>().join(", ");
    let script = format!(
        "tell application \"System Events\" to display dialog \"{}\" buttons {{{}}} default button \"{}\"",
        message.replace('"', "\\\""),
        btns,
        buttons.last().unwrap_or(&"OK"),
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split("button returned:")
        .nth(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Show a list dialog to pick from items. Returns chosen item or None.
fn list_dialog(message: &str, items: &[String]) -> Option<String> {
    let item_list = items.iter().map(|i| format!("\"{}\"", i.replace('"', "\\\""))).collect::<Vec<_>>().join(", ");
    let script = format!(
        "tell application \"System Events\" to choose from list {{{}}} with prompt \"{}\"",
        item_list,
        message.replace('"', "\\\""),
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout == "false" {
        return None;
    }
    Some(stdout)
}

// ── Title rendering ───────────────────────────────────────────────────────────

fn build_title(data: &MenuBarData) -> Retained<NSAttributedString> {
    if data.accounts.is_empty() {
        let nsstr = NSString::from_str("Claude…");
        return NSMutableAttributedString::from_nsstring(&nsstr).into_super();
    }

    // Build multi-line title: one line per account
    let mut lines: Vec<String> = Vec::new();
    let mut line_meta: Vec<(usize, bool, usize, bool)> = Vec::new(); // (s_pct_len, s_over, w_pct_len, w_over)

    for acc in &data.accounts {
        let label = if acc.name.is_empty() { "?" } else { &acc.name };
        let s_pct = acc.session_pct
            .map(|p| format!("S:{:.0}%", p))
            .unwrap_or_else(|| "S:?".to_string());
        let w_pct = acc.weekly_pct
            .map(|p| format!("W:{:.0}%", p))
            .unwrap_or_else(|| "W:?".to_string());

        let s_suffix = if acc.session_reset_str.is_empty() {
            String::new()
        } else {
            format!(" {}", acc.session_reset_str)
        };

        let line = format!("{}: {} {}{}", label, s_pct, w_pct, s_suffix);
        let s_start = label.len() + 2; // after "Name: "
        let s_pct_len = s_pct.len();
        let w_pct_len = w_pct.len();
        line_meta.push((s_pct_len, acc.session_over, w_pct_len, acc.weekly_over));
        lines.push(line);
    }

    let full = lines.join("\n");
    let nsstr = NSString::from_str(&full);
    let mstr = NSMutableAttributedString::from_nsstring(&nsstr);

    let total_len = full.encode_utf16().count();

    let regular: Retained<NSFont> = NSFont::systemFontOfSize(10.0);
    let bold: Retained<NSFont> = NSFont::boldSystemFontOfSize(10.0);
    let regular_any: &AnyObject = &*regular;
    let bold_any: &AnyObject = &*bold;

    let orange: Retained<NSColor> = NSColor::systemOrangeColor();
    let green: Retained<NSColor> = NSColor::systemGreenColor();

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
    }

    // Apply bold + color to each account's S:XX% and W:XX%
    let mut offset: usize = 0;
    for (i, line) in lines.iter().enumerate() {
        let (s_pct_len, s_over, w_pct_len, w_over) = line_meta[i];
        let acc = &data.accounts[i];
        let label = if acc.name.is_empty() { "?" } else { &acc.name };
        let prefix_len = label.encode_utf16().count() + 2; // "Name: "

        let s_start = offset + prefix_len;
        let s_pct_utf16 = lines[i][label.len() + 2..label.len() + 2 + s_pct_len].encode_utf16().count();

        // Find W: position
        let w_start_byte = label.len() + 2 + s_pct_len + 1; // +1 for space
        let w_start_utf16 = lines[i][..w_start_byte].encode_utf16().count();

        let orange_any: &AnyObject = &*orange;
        let green_any: &AnyObject = &*green;

        unsafe {
            // Bold + color for S:XX%
            mstr.addAttribute_value_range(
                NSFontAttributeName,
                bold_any,
                NSRange { location: s_start, length: s_pct_utf16 },
            );
            mstr.addAttribute_value_range(
                NSForegroundColorAttributeName,
                if s_over { orange_any } else { green_any },
                NSRange { location: s_start, length: s_pct_utf16 },
            );

            // Bold + color for W:XX%
            let w_abs = offset + w_start_utf16;
            let w_pct_utf16 = {
                let w_str = &lines[i][w_start_byte..w_start_byte + w_pct_len];
                w_str.encode_utf16().count()
            };
            mstr.addAttribute_value_range(
                NSFontAttributeName,
                bold_any,
                NSRange { location: w_abs, length: w_pct_utf16 },
            );
            mstr.addAttribute_value_range(
                NSForegroundColorAttributeName,
                if w_over { orange_any } else { green_any },
                NSRange { location: w_abs, length: w_pct_utf16 },
            );
        }

        offset += line.encode_utf16().count() + 1; // +1 for \n
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

    let (action_tx, action_rx) = std::sync::mpsc::channel::<MenuAction>();
    ACTION_TX.set(Mutex::new(action_tx)).ok();

    let handler: Retained<MenuHandler> = unsafe { msg_send![MenuHandler::class(), new] };

    let status_bar = NSStatusBar::systemStatusBar();
    let item: Retained<NSStatusItem> = status_bar.statusItemWithLength(-1.0_f64);
    if let Some(btn) = item.button(mtm) {
        let btn: &NSStatusBarButton = &btn;
        btn.setTitle(&NSString::from_str("Claude…"));
    }

    // Build dropdown menu
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);

    // Account list section (dynamic — rebuilt each tick)
    let accounts_header = make_item("── Accounts ──", mtm);
    accounts_header.setEnabled(false);
    menu.addItem(&accounts_header);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let add_item = make_action_item("Add Account…", &handler, sel!(addAccountAction:), mtm);
    menu.addItem(&add_item);

    let remove_item = make_action_item("Remove Account…", &handler, sel!(removeAccountAction:), mtm);
    menu.addItem(&remove_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let refresh_item = make_action_item("Refresh Now", &handler, sel!(refreshNowAction:), mtm);
    menu.addItem(&refresh_item);

    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let quit_item = make_action_item("Quit", &handler, sel!(quitAction:), mtm);
    menu.addItem(&quit_item);

    item.setMenu(Some(&menu));

    // Track dynamic account menu items (inserted between header and separator)
    let mut account_items: Vec<Retained<NSMenuItem>> = Vec::new();
    let mut prev_account_count: usize = 0;

    loop {
        let until = NSDate::dateWithTimeIntervalSinceNow(0.5);
        unsafe {
            if let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&until),
                NSDefaultRunLoopMode,
                true,
            ) {
                app.sendEvent(&event);
            }
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
                MenuAction::AddAccount => {
                    handle_add_account(&refresh_notify);
                }
                MenuAction::RemoveAccount(_) => {
                    handle_remove_account(&refresh_notify);
                }
                MenuAction::RefreshNow => {
                    refresh_notify.notify_one();
                }
                MenuAction::Quit => std::process::exit(0),
            }
        }

        // Update menu bar state
        let current = data.lock().unwrap().clone();

        // Rebuild account items in menu if count changed
        if current.accounts.len() != prev_account_count {
            // Remove old dynamic items
            for old_item in &account_items {
                menu.removeItem(old_item);
            }
            account_items.clear();

            // Insert new items after the header (index 1 = after "── Accounts ──")
            for (i, acc) in current.accounts.iter().enumerate() {
                let source_tag = match acc.source {
                    DataSource::OAuth => "OAuth",
                    DataSource::WebCookie => "Cookie",
                };
                let s_str = acc.session_pct.map(|p| format!("S:{:.0}%", p)).unwrap_or("S:?".into());
                let w_str = acc.weekly_pct.map(|p| format!("W:{:.0}%", p)).unwrap_or("W:?".into());
                let label = format!("  {} — {} {} [{}]", acc.name, s_str, w_str, source_tag);
                let mi = make_item(&label, mtm);
                mi.setEnabled(false);
                menu.insertItem_atIndex(&mi, (1 + i) as isize);
                account_items.push(mi);
            }
            prev_account_count = current.accounts.len();
        } else {
            // Update existing items' titles
            for (i, acc) in current.accounts.iter().enumerate() {
                if i < account_items.len() {
                    let source_tag = match acc.source {
                        DataSource::OAuth => "OAuth",
                        DataSource::WebCookie => "Cookie",
                    };
                    let s_str = acc.session_pct.map(|p| format!("S:{:.0}%", p)).unwrap_or("S:?".into());
                    let w_str = acc.weekly_pct.map(|p| format!("W:{:.0}%", p)).unwrap_or("W:?".into());
                    let reset = if acc.session_reset_str.is_empty() { String::new() } else { format!(" ─ {}", acc.session_reset_str) };
                    let label = format!("  {} — {} {}{} [{}]", acc.name, s_str, w_str, reset, source_tag);
                    account_items[i].setTitle(&NSString::from_str(&label));
                }
            }
        }

        // Update status bar title
        if let Some(btn) = item.button(mtm) {
            let btn: &NSStatusBarButton = &btn;
            btn.setAttributedTitle(&build_title(&current));
        }
    }
}

fn handle_add_account(refresh_notify: &Arc<tokio::sync::Notify>) {
    // Step 1: Ask for account name
    let name = match prompt_dialog("Enter account name (e.g. Work, Personal):", "") {
        Some(n) => n,
        None => return,
    };

    // Step 2: Ask for source type
    let source_choice = match choice_dialog(
        "Choose authentication method:",
        &["Cancel", "Session Cookie", "OAuth (Keychain)"],
    ) {
        Some(s) => s,
        None => return,
    };

    let (source, credential) = match source_choice.as_str() {
        "Session Cookie" => {
            let cookie = match prompt_dialog("Paste your claude.ai sessionKey cookie:", "") {
                Some(c) => c,
                None => return,
            };
            (crate::config::AccountSource::WebCookie, Some(cookie))
        }
        "OAuth (Keychain)" => {
            // Optional: custom keychain service
            let service = prompt_dialog(
                "Keychain service name (leave empty for default 'Claude Code-credentials'):",
                "",
            );
            let cred = service.filter(|s| !s.is_empty());
            (crate::config::AccountSource::OAuth, cred)
        }
        _ => return,
    };

    // Save to config
    match crate::config::load() {
        Ok(mut cfg) => {
            cfg.accounts.push(crate::config::Account {
                id: uuid::Uuid::new_v4().to_string(),
                name,
                source,
                credential,
            });
            if let Err(e) = crate::config::save(&cfg) {
                tracing::error!("Failed to save config: {e}");
            } else {
                refresh_notify.notify_one();
            }
        }
        Err(e) => tracing::error!("Failed to load config: {e}"),
    }
}

fn handle_remove_account(refresh_notify: &Arc<tokio::sync::Notify>) {
    let cfg = match crate::config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to load config: {e}");
            return;
        }
    };

    if cfg.accounts.is_empty() {
        return;
    }

    let names: Vec<String> = cfg.accounts.iter().map(|a| a.name.clone()).collect();
    let chosen = match list_dialog("Select account to remove:", &names) {
        Some(c) => c,
        None => return,
    };

    // Find and remove the account
    match crate::config::load() {
        Ok(mut cfg) => {
            cfg.accounts.retain(|a| a.name != chosen);
            if let Err(e) = crate::config::save(&cfg) {
                tracing::error!("Failed to save config: {e}");
            } else {
                refresh_notify.notify_one();
            }
        }
        Err(e) => tracing::error!("Failed to load config: {e}"),
    }
}
