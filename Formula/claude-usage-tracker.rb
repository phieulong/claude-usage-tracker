# Formula template — file này được tự động cập nhật bởi GitHub Actions khi release
# Đây là bản placeholder, SHA256 sẽ được điền sau lần release đầu tiên.
#
# Để dùng tap này:
#   brew tap phieulong/tap
#   brew install claude-usage-tracker
#   brew services start claude-usage-tracker

class ClaudeUsageTracker < Formula
  desc "macOS menu bar app tracking Claude AI session + weekly token usage"
  homepage "https://github.com/phieulong/claude-usage-tracker"
  version "1.0.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/phieulong/claude-usage-tracker/releases/download/v1.0.0/claude-usage-tracker-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_FILL_AFTER_FIRST_RELEASE"
    else
      url "https://github.com/phieulong/claude-usage-tracker/releases/download/v1.0.0/claude-usage-tracker-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_FILL_AFTER_FIRST_RELEASE"
    end
  end

  def install
    bin.install "claude-usage-tracker"
    (prefix/"LaunchAgents").install "com.user.claude-usage-tracker.plist"
  end

  service do
    run [opt_bin/"claude-usage-tracker", "daemon"]
    keep_alive true
    log_path "/tmp/claude-usage-tracker.log"
    error_log_path "/tmp/claude-usage-tracker.err"
    environment_variables RUST_LOG: "info"
  end

  def caveats
    <<~EOS
      Claude Usage Tracker là menu bar app — chạy ẩn, không có cửa sổ.

      Khởi động tự động khi login:
        brew services start claude-usage-tracker

      Dừng:
        brew services stop claude-usage-tracker
    EOS
  end

  test do
    assert_match "claude-usage-tracker", shell_output("#{bin}/claude-usage-tracker --help 2>&1")
  end
end

