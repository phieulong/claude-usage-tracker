# claude-usage-tracker

Daemon Rust theo dõi token usage của Claude Code theo session (5 giờ gần nhất) và weekly (7 ngày gần nhất) trên macOS. Đọc trực tiếp từ `~/.claude/projects/**/*.jsonl`.

## Features

- Parse JSONL logs của Claude Code để tổng hợp token usage
- Hai cửa sổ thời gian: **session** (5h) và **weekly** (7d)
- macOS notification khi vượt threshold
- Optional webhook (Slack/Discord) khi alert
- Lưu lịch sử snapshot ra JSON
- Chạy như launchd daemon (tự động khởi động khi login)

## Build

```bash
cargo build --release
cp target/release/claude-usage-tracker ~/.local/bin/
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
=== Claude Usage Snapshot (2026-04-22 15:59:11 UTC) ===
Session (last 5h):  total=   19205  in=     207  out=   18998  cache_read=  925255
Weekly  (last 7d):  total=  816212  in=   30709  out=  785503  cache_read=54172707
```

## Config

File config được tạo tự động tại `~/.claude/usage-tracker-config.json`:

```json
{
  "interval_secs": 900,
  "session_token_alert": 50000,
  "weekly_token_alert": 500000,
  "webhook_url": null,
  "output_path": "/Users/<you>/.claude/usage-tracker.json"
}
```

| Field | Mô tả |
|---|---|
| `interval_secs` | Tần suất poll (giây, mặc định 900 = 15 phút) |
| `session_token_alert` | Alert khi session total tokens vượt ngưỡng |
| `weekly_token_alert` | Alert khi weekly total tokens vượt ngưỡng |
| `webhook_url` | URL Slack/Discord webhook (optional) |
| `output_path` | File JSON lưu lịch sử snapshot |

## Setup launchd (tự động chạy khi login)

```bash
# Copy plist vào LaunchAgents
cp com.user.claude-usage-tracker.plist ~/Library/LaunchAgents/

# Load daemon
launchctl load ~/Library/LaunchAgents/com.user.claude-usage-tracker.plist

# Kiểm tra status
launchctl list | grep claude-usage-tracker

# Xem log
tail -f /tmp/claude-usage-tracker.log
```

Để dừng:
```bash
launchctl unload ~/Library/LaunchAgents/com.user.claude-usage-tracker.plist
```

## Cấu trúc

```
src/
├── main.rs              # CLI entry + scheduler loop
├── config.rs            # Plan limits, thresholds
├── sources/
│   ├── mod.rs
│   └── claude_code.rs   # Parse ~/.claude/projects/*.jsonl
├── aggregator.rs        # Gộp session + weekly
├── alert.rs             # macOS notification + webhook
└── output.rs            # JSON writer
```
