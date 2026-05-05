# Claude Usage Tracker

> macOS menu bar app theo dõi usage quota của Claude AI — session 5h và weekly 7d — hỗ trợ **nhiều tài khoản cùng lúc**, hiển thị trực tiếp trên menu bar.

![macOS](https://img.shields.io/badge/macOS-12%2B-blue) ![Rust](https://img.shields.io/badge/Rust-2024-orange) ![License](https://img.shields.io/badge/license-MIT-green)

---

## Vấn đề đang giải quyết

Claude Code giới hạn usage theo 2 chu kỳ:
- **Session (5h)**: reset mỗi 5 tiếng kể từ request đầu tiên trong session
- **Weekly (7d)**: reset vào một ngày/giờ cố định mỗi tuần

Không có cách nào xem % còn lại ngoài việc vào Settings → Usage trên web. App này giải quyết điều đó — hiển thị luôn ngoài menu bar, cập nhật tự động, cảnh báo khi gần hết quota. **Hỗ trợ theo dõi nhiều tài khoản Claude đồng thời.**

---

## Demo

```
Work: S:65% W:74% 4h 36m
Personal: S:12% W:8% 2h 15m
```

**S** = Session (5h) · **W** = Weekly (7d)  
Màu **xanh** = bình thường · Màu **cam** = đã vượt ngưỡng cảnh báo

Click vào menu bar icon:

```
── Accounts ──
  Work — S:65% W:74% ─ 4h 36m [OAuth]
  Personal — S:12% W:8% ─ 2h 15m [Cookie]
────────────────────
Add Account…
Remove Account…
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

### Menu bar — Nhiều tài khoản

Sau khi chạy, icon xuất hiện ở góc phải menu bar hiển thị usage **tất cả tài khoản**. Click để mở dropdown:

| Menu item | Chức năng |
|---|---|
| `── Accounts ──` | Header, liệt kê từng account với S%, W%, source |
| `Add Account…` | Thêm tài khoản mới (nhập tên + chọn OAuth/Cookie + credential) |
| `Remove Account…` | Xoá tài khoản (chọn từ danh sách) |
| `Refresh Now` | Poll dữ liệu ngay lập tức |
| `Quit` | Thoát app |

### Thêm tài khoản

Click **Add Account…** → nhập lần lượt:
1. **Tên gợi nhớ** (ví dụ: "Work", "Personal", "Team")
2. **Phương thức xác thực**: Session Cookie hoặc OAuth (Keychain)
3. **Credential**: paste sessionKey cookie, hoặc tên keychain service (bỏ trống = mặc định)

Mỗi tài khoản hiển thị 1 dòng riêng trên menu bar.

### Cảnh báo notification

Khi usage vượt threshold (mặc định 80%), app gửi macOS notification **kèm tên tài khoản**. Cứ mỗi **5% tiếp theo** lại gửi thêm (85%, 90%, 95%...). Tự reset sau khi session/weekly reset.

---

## Cấu hình

File tự sinh tại `~/.claude/usage-tracker-config.json`:

```json
{
  "interval_secs": 900,
  "alert_pct_session": 80.0,
  "alert_pct_weekly": 80.0,
  "webhook_url": null,
  "output_path": "~/.claude/usage-tracker.json",
  "accounts": [
    {
      "id": "uuid-auto-generated",
      "name": "Work",
      "source": "OAuth",
      "credential": null
    },
    {
      "id": "uuid-auto-generated",
      "name": "Personal",
      "source": "WebCookie",
      "credential": "sk-ant-session-..."
    }
  ]
}
```

| Field | Mặc định | Mô tả |
|---|---|---|
| `interval_secs` | `900` | Tần suất poll (giây). 900 = 15 phút |
| `alert_pct_session` | `80.0` | Ngưỡng cảnh báo session (%) |
| `alert_pct_weekly` | `80.0` | Ngưỡng cảnh báo weekly (%) |
| `webhook_url` | `null` | URL Slack/Discord webhook |
| `output_path` | `~/.claude/usage-tracker.json` | File lưu lịch sử snapshot |
| `accounts` | 1 account OAuth | Danh sách tài khoản theo dõi |

### Account fields

| Field | Mô tả |
|---|---|
| `id` | UUID tự động, không cần sửa |
| `name` | Tên hiển thị trên menu bar |
| `source` | `"OAuth"` hoặc `"WebCookie"` |
| `credential` | Session cookie (WebCookie) hoặc keychain service override (OAuth, null = mặc định) |

Sửa trực tiếp JSON → bấm **Refresh Now** trong menu → có hiệu lực ngay, không cần restart.

> **Migration tự động**: Nếu đang dùng config cũ (có `session_cookie`), app tự chuyển sang format mới khi khởi động.

---

## Nguồn dữ liệu

App hỗ trợ 2 nguồn **per-account**, poll song song tất cả accounts:

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

> Lần đầu chạy, macOS hỏi quyền truy cập Keychain → chọn **Always Allow**.

### 2. Web Cookie (cho web user)

Dành cho người không cài Claude Code. Thêm account với source `WebCookie` và paste `sessionKey` cookie từ claude.ai. Nếu Web API lỗi, log error nhưng không ảnh hưởng các account khác.

---

## Kiến trúc & Modules

```
src/
├── main.rs           # Entry point, CLI parser, multi-account scheduler loop
├── config.rs         # Config JSON với accounts array + migration logic
├── aggregator.rs     # Poll tất cả accounts song song, trả Vec<Snapshot>
├── menubar.rs        # Native macOS menu bar UI, dynamic account list
├── alert.rs          # Per-account alert state, notification, webhook
├── output.rs         # In terminal + ghi JSON
└── sources/
    ├── oauth_api.rs  # Anthropic OAuth usage API (parameterized keychain)
    └── claude_web.rs # claude.ai Web API (session cookie)
```

### Threading model

```
Main thread (AppKit)           Background thread (Tokio async)
        |                               |
        | <-- Arc<Mutex<MenuBarData>> --| cập nhật tất cả accounts mỗi N giây
        | <-- mpsc::channel<Notif> -----| yêu cầu gửi notification
        | ---> tokio::Notify ---------->| Refresh Now / account thay đổi
```

### Multi-account flow

1. Daemon reload config mỗi tick → lấy danh sách accounts
2. `aggregator::snapshot_all()` poll **tất cả accounts song song** (futures::join_all)
3. Mỗi account thành công → update `MenuBarData.accounts[i]`, write history, check alert
4. Account lỗi → log error, hiển thị "?" trên menu bar
5. Menu bar render 1 dòng per account trên status bar title

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
| `uuid` | Unique account IDs |
| `futures` | Concurrent account polling |
