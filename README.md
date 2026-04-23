# claude-usage-tracker

Daemon Rust theo dõi Claude Code session + weekly usage trên macOS.

**Nguồn dữ liệu chính**: Claude OAuth endpoint `/api/oauth/usage` — cùng nguồn Claude hiển thị trong Settings → Usage. Trả về chính xác % utilization + reset time.

**Fallback**: Parse `~/.claude/projects/**/*.jsonl` khi OAuth không dùng được (token hết hạn, mất mạng, v.v.).

## Features

- **OAuth source (primary)**: đọc access token từ macOS Keychain (`Claude Code-credentials`), gọi `/api/oauth/usage` → kết quả khớp 100% với Claude UI
- **Local JSONL (fallback + context)**: gap-detection 5h cho session, fixed day-of-week reset cho weekly; breakdown input/output/cache_create/cache_read
- % utilization + thời gian còn lại đến reset
- macOS notification khi vượt threshold %
- Optional webhook (Slack/Discord)
- Lưu lịch sử snapshot ra JSON
- Chạy như launchd daemon

## Cài đặt

### Homebrew (khuyên dùng)

```bash
brew install phieulong/tap/claude-usage-tracker
brew services start claude-usage-tracker   # tự chạy khi login, không terminal
```

### Tải .app bundle

Tải file `claude-usage-tracker-universal-apple-darwin.zip` từ [Releases](https://github.com/phieulong/claude-usage-tracker/releases), giải nén và kéo vào `/Applications/`. Double-click để chạy — app sẽ hiện trên menu bar, không mở Terminal.

### Build từ source

```bash
cargo build --release
# Build .app bundle (arm64 | x86_64 | universal)
./scripts/build_app.sh universal
cp -r "dist/Claude Usage Tracker.app" /Applications/
```

## Usage

```bash
# Xem usage ngay lập tức
claude-usage-tracker status

# Xem config hiện tại
claude-usage-tracker config

# Chạy daemon (mặc định, poll mỗi 15 phút)
claude-usage-tracker daemon

# Override interval (giây)
claude-usage-tracker --interval 300 daemon
```

## Output ví dụ

```
=== Claude Usage  2026-04-23 00:23:30 +07:00  [source: oauth]  ===
Session (5h)    65.0% (Claude)   resets in 4h 36m (Thu 05:00 +07:00)
               local tokens: total=580,267  in=425  out=172,896  cache_create=406,946  cache_read=15,309,036  window_start=2026-04-23 00:00 +07:00
Weekly  (7d)    74.0% (Claude)   resets in 34h 36m (Fri 11:00 +07:00)
               local tokens: total=3,890,393  in=30,927  out=939,401  cache_create=2,920,065  cache_read=68,556,488  window_start=2026-04-17 11:00 +07:00
```

`source: oauth` nghĩa là đang dùng Claude API chính thức. Nếu là `local` thì đang fallback sang parse JSONL.

## Config

File config tự sinh tại `~/.claude/usage-tracker-config.json`:

```json
{
  "interval_secs": 900,
  "alert_pct_session": 80.0,
  "alert_pct_weekly": 80.0,
  "session_token_alert": 500000,
  "weekly_token_alert": 5000000,
  "weekly_reset_weekday": "Fri",
  "weekly_reset_time": "11:00",
  "webhook_url": null,
  "output_path": "/Users/<you>/.claude/usage-tracker.json"
}
```

| Field | Mô tả |
|---|---|
| `interval_secs` | Tần suất poll (giây, 900 = 15 phút) |
| `alert_pct_session` | Alert khi session utilization ≥ giá trị này (OAuth source dùng giá trị Claude trả về) |
| `alert_pct_weekly` | Alert khi weekly utilization ≥ giá trị này |
| `session_token_alert` | Token threshold để tính % khi fallback sang JSONL (OAuth fail) |
| `weekly_token_alert` | Token threshold weekly cho fallback |
| `weekly_reset_weekday` | Day of week weekly reset cho fallback (`Mon`..`Sun`) |
| `weekly_reset_time` | Local time reset cho fallback (HH:MM, 24h) |
| `webhook_url` | URL Slack/Discord webhook (optional) |
| `output_path` | File JSON lưu lịch sử snapshot |

### OAuth source — cơ chế

Tool đọc OAuth access token từ macOS Keychain (entry `Claude Code-credentials`) rồi gọi:

```
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer <token>
anthropic-beta: oauth-2025-04-20
```

Response:
```json
{
  "five_hour": { "utilization": 65.0, "resets_at": "2026-04-22T22:00:00Z" },
  "seven_day": { "utilization": 74.0, "resets_at": "2026-04-24T04:00:00Z" },
  ...
}
```

Lần chạy đầu macOS sẽ hỏi quyền truy cập Keychain — chọn **Always Allow** cho tiện.

Nếu Claude Code logout hoặc token hết hạn → fallback sang JSONL.

## Setup launchd (tự chạy khi login)

```bash
cp com.user.claude-usage-tracker.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.user.claude-usage-tracker.plist

# Xem log
tail -f /tmp/claude-usage-tracker.log

# Dừng
launchctl unload ~/Library/LaunchAgents/com.user.claude-usage-tracker.plist
```

## Cấu trúc

```
src/
├── main.rs              # CLI entry + scheduler loop
├── config.rs            # Plan limits, thresholds, weekly reset config
├── sources/
│   ├── mod.rs
│   └── claude_code.rs   # Parse JSONL + gap-based session detection
├── aggregator.rs        # Session (gap 5h) + weekly (fixed day/time reset)
├── alert.rs             # macOS notification + webhook
└── output.rs            # JSON writer + terminal print
```

## Cơ chế

**Session (5h)**: Walk các entry trong 24h gần nhất theo thứ tự thời gian. Session start = entry đầu; nếu có gap > 5h giữa 2 entry liên tiếp, session start nhảy sang entry sau. Session active nếu `now - session_start ≤ 5h`.

**Weekly**: `window_start = next_reset - 7d`, trong đó `next_reset` là lần sau kế tiếp của (weekday, time) theo local time. Sum tất cả token từ `window_start` đến `now`.

**Token cho quota**: `input + output + cache_creation`. `cache_read` rất rẻ (0.1× input) nên không count vào limit.
