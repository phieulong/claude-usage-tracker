# Claude Usage Tracker

> macOS menu bar app theo dõi usage quota của Claude AI — session 5h và weekly 7d — hiển thị trực tiếp trên menu bar, không cần mở browser.

![macOS](https://img.shields.io/badge/macOS-12%2B-blue) ![Rust](https://img.shields.io/badge/Rust-2024-orange) ![License](https://img.shields.io/badge/license-MIT-green)

---

## Vấn đề đang giải quyết

Claude Code giới hạn usage theo 2 chu kỳ:
- **Session (5h)**: reset mỗi 5 tiếng kể từ request đầu tiên trong session
- **Weekly (7d)**: reset vào một ngày/giờ cố định mỗi tuần

Không có cách nào xem % còn lại ngoài việc vào Settings → Usage trên web. App này giải quyết điều đó — hiển thị luôn ngoài menu bar, cập nhật tự động, cảnh báo khi gần hết quota.

---

## Demo

```
S:65% — 4h 36m
W:74% — 34h 36m
```

**S** = Session (5h) · **W** = Weekly (7d)  
Màu **xanh** = bình thường · Màu **cam** = đã vượt ngưỡng cảnh báo

Click vào menu bar icon:

```
Source: OAuth ✓
────────────────────
Set Session Cookie…
────────────────────
Refresh Now
────────────────────
Quit
```

---

## Cài đặt

### One-line install (khuyên dùng — mọi macOS, không cần Xcode/CLT)

```bash
curl -fsSL https://raw.githubusercontent.com/phieulong/claude-usage-tracker/main/install.sh | bash
```

Script tự động:
- Phát hiện kiến trúc (Apple Silicon / Intel)
- Tải binary phù hợp từ GitHub Releases
- Tạo `/Applications/Claude Usage Tracker.app` → mở được qua **Spotlight** (`Cmd+Space`)
- Hỏi có muốn **tự chạy khi login** không
- Hỏi có muốn **chạy ngay** không

### Homebrew

```bash
brew install phieulong/tap/claude-usage-tracker
brew services start claude-usage-tracker
```

> Nếu gặp lỗi CLT trên macOS beta/mới: dùng one-line install ở trên.

### Tải .app bundle thủ công

Tải `claude-usage-tracker-universal-apple-darwin.zip` từ [Releases](https://github.com/phieulong/claude-usage-tracker/releases), giải nén và kéo vào `/Applications/`.

### Build từ source

```bash
cargo build --release
./scripts/build_app.sh universal
cp -r "dist/Claude Usage Tracker.app" /Applications/
```

---

## Yêu cầu

| Yêu cầu | Chi tiết |
|---|---|
| macOS | 12.0 Monterey trở lên |
| Claude Code | Đã đăng nhập (để dùng OAuth source) |
| Keychain | Cho phép truy cập lần đầu → chọn **Always Allow** |

Không cần cài thêm gì khác. Binary tự chứa mọi dependency.

---

## Hướng dẫn sử dụng

### Khởi động

```bash
# Qua Spotlight (sau khi cài .app):
# Cmd+Space → gõ "Claude Usage" → Enter

# Qua CLI:
claude-usage-tracker daemon

# Xem usage một lần rồi thoát:
claude-usage-tracker status

# Xem config hiện tại:
claude-usage-tracker config

# Đổi interval poll:
claude-usage-tracker --interval 300 daemon   # poll mỗi 5 phút
```

### Menu bar

Sau khi chạy, icon xuất hiện ở góc phải menu bar. Click để mở dropdown:

| Menu item | Chức năng |
|---|---|
| `Source: OAuth ✓` | Cho biết đang lấy data từ nguồn nào |
| `Set Session Cookie…` | Nhập cookie để dùng Web API (không cần Claude Code) |
| `Clear Session Cookie` | Xoá cookie, quay về OAuth |
| `Refresh Now` | Poll dữ liệu ngay lập tức |
| `Quit` | Thoát app |

### Cảnh báo notification

Khi usage vượt threshold (mặc định 80%), app gửi macOS notification. Cứ mỗi **5% tiếp theo** lại gửi thêm (85%, 90%, 95%...). Tự reset sau khi session/weekly reset.

---

## Cấu hình

File tự sinh tại `~/.claude/usage-tracker-config.json`:

```json
{
  "interval_secs": 900,
  "alert_pct_session": 80.0,
  "alert_pct_weekly": 80.0,
  "webhook_url": null,
  "session_cookie": null,
  "output_path": "~/.claude/usage-tracker.json"
}
```

| Field | Mặc định | Mô tả |
|---|---|---|
| `interval_secs` | `900` | Tần suất poll (giây). 900 = 15 phút |
| `alert_pct_session` | `80.0` | Ngưỡng cảnh báo session (%) |
| `alert_pct_weekly` | `80.0` | Ngưỡng cảnh báo weekly (%) |
| `webhook_url` | `null` | URL Slack/Discord webhook |
| `session_cookie` | `null` | `sessionKey` cookie của claude.ai (cho web user không dùng Claude Code) |
| `output_path` | `~/.claude/usage-tracker.json` | File lưu lịch sử snapshot |

Sửa trực tiếp JSON → bấm **Refresh Now** trong menu → có hiệu lực ngay, không cần restart.

---

## Nguồn dữ liệu

App hỗ trợ 2 nguồn, tự động ưu tiên:

### 1. OAuth API (mặc định)

Đọc access token từ macOS Keychain (`Claude Code-credentials`) → gọi endpoint chính thức:

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <token>
anthropic-beta: oauth-2025-04-20
```

Response:
```json
{
  "five_hour": { "utilization": 65.0, "resets_at": "2026-04-23T05:00:00Z" },
  "seven_day":  { "utilization": 74.0, "resets_at": "2026-04-24T04:00:00Z" }
}
```

Kết quả **khớp 100%** với con số Claude hiển thị trong Settings → Usage.

> Lần đầu chạy, macOS hỏi quyền truy cập Keychain → chọn **Always Allow**.

### 2. Web Cookie (cho web user)

Dành cho người không cài Claude Code. Nhập `sessionKey` cookie từ claude.ai qua menu **Set Session Cookie...** — app dùng Web API thay OAuth. Nếu Web API lỗi, tự fallback về OAuth.

---

## Kiến trúc & Modules

```
src/
├── main.rs           # Entry point, CLI parser, scheduler loop
├── config.rs         # Đọc/ghi config JSON
├── aggregator.rs     # Điều phối nguồn dữ liệu, tổng hợp Snapshot
├── menubar.rs        # Native macOS menu bar UI (objc2 / AppKit)
├── alert.rs          # Logic cảnh báo, notification, webhook
├── output.rs         # In terminal + ghi JSON
└── sources/
    ├── oauth_api.rs  # Anthropic OAuth usage API
    └── claude_web.rs # claude.ai Web API (session cookie)
```

### `main.rs` — Entry point & scheduler

Xử lý CLI (`status` / `config` / `daemon`). Ở chế độ daemon, app chạy **2 thread song song**:

```
Main thread (AppKit)           Background thread (Tokio async)
        |                               |
        | <-- Arc<Mutex<MenuBarData>> --| cập nhật % mỗi N giây
        | <-- mpsc::channel<Notif> -----| yêu cầu gửi notification
        | ---> tokio::Notify ---------->| Refresh Now / config thay đổi
```

AppKit bắt buộc phải chạy trên main thread — do đó Tokio runtime được đặt trên background thread riêng.

### `aggregator.rs` — Điều phối nguồn dữ liệu

Nhận config, chọn nguồn phù hợp, trả về `Snapshot`:

```
session_cookie có?
  Có  → thử Web API
          OK   → DataSource::WebCookie
          Lỗi  → fallback OAuth
  Không → OAuth API
              429 Too Many Requests → tự retry sau 3 phút
```

### `sources/oauth_api.rs` — OAuth source

1. Chạy `security find-generic-password -s "Claude Code-credentials" -w` để đọc JSON từ Keychain
2. Parse access token, kiểm tra expiry
3. `GET /api/oauth/usage` với Bearer token
4. Parse → `OauthUsageResponse { five_hour, seven_day }`

### `menubar.rs` — Menu bar UI

Dùng **objc2** (Rust bindings cho Objective-C) để tạo native macOS UI:

- `NSStatusItem` hiển thị attributed string 2 dòng (session + weekly)
- Font **bold** cho % chính, regular cho thời gian reset
- Màu xanh/cam theo threshold từng dòng độc lập
- ObjC action handler cho các menu item (Set Cookie, Refresh, Quit...)
- Event loop dùng `NSApplication.nextEventMatchingMask + sendEvent` — cách đúng để AppKit dispatch mouse events (dropdown mới hoạt động)

### `alert.rs` — Logic cảnh báo

Thuật toán step-based, không spam notification:

```
threshold = 80%
[80%–85%) → alert lần 1: "Session at 80.x% used"
[85%–90%) → alert lần 2: "Session at 85.x% used"
[90%–95%) → alert lần 3: "Session at 90.x% used"
pct < 80% → reset trạng thái, sẵn sàng alert lại
```

Gửi đồng thời macOS notification + Slack/Discord webhook (nếu `webhook_url` được cấu hình).

### `config.rs` — Config

Đọc/ghi `~/.claude/usage-tracker-config.json`. Tự sinh với giá trị mặc định nếu chưa tồn tại. Daemon **reload config mỗi lần poll** — thay đổi có hiệu lực ngay không cần restart.

### `output.rs` — Output

- In snapshot ra stdout có màu ANSI
- Append snapshot vào file JSON (`output_path`) để tích hợp với tool khác

---

## Logs & Debug

```bash
# Log realtime (khi chạy qua launchd / brew services)
tail -f /tmp/claude-usage-tracker.log
tail -f /tmp/claude-usage-tracker.err

# Verbose logging
RUST_LOG=debug claude-usage-tracker daemon
```

---

## Gỡ cài đặt

```bash
# Nếu dùng one-line installer:
sudo rm -rf "/Applications/Claude Usage Tracker.app"
rm -f /opt/homebrew/bin/claude-usage-tracker
launchctl unload ~/Library/LaunchAgents/com.phieulong.claude-usage-tracker.plist
rm ~/Library/LaunchAgents/com.phieulong.claude-usage-tracker.plist

# Nếu dùng brew:
brew uninstall claude-usage-tracker
brew services stop claude-usage-tracker
```

---

## Tech stack

| Crate | Vai trò |
|---|---|
| `tokio` | Async runtime cho HTTP |
| `reqwest` | HTTP client (rustls, không cần OpenSSL) |
| `objc2` + `objc2-app-kit` | Native macOS UI — menu bar, AppKit |
| `mac-notification-sys` | macOS push notification |
| `serde` + `serde_json` | Serialize config và API response |
| `clap` | CLI argument parsing |
| `chrono` | Timezone-aware datetime |
| `tracing` | Structured logging |
