# TEMPLATE — release-time scaffold, NOT self-publishing.
#
# To ship, copy this file into a Homebrew tap repository
# (e.g. github.com/elci-group/homebrew-tap) as Formula/kaptaind.rb and replace
# every <...> placeholder with the real per-release values: the <VERSION>
# (without the leading "v") and the SHA256 of each tarball. Those values come
# straight out of the release workflow (SHA256SUMS.txt on the GitHub Release).
#
# End users then install with:
#   brew tap elci-group/tap
#   brew install kaptaind
class Kaptaind < Formula
  desc "Repository change-watcher that clusters edits, scores them, and ships semantic releases"
  homepage "https://github.com/elci-group/kaptaind"
  version "<VERSION>"
  license "MIT"

  on_macos do
    on_intel do
      url "https://github.com/elci-group/kaptaind/releases/download/v<VERSION>/kaptaind-<VERSION>-x86_64-apple-darwin.tar.gz"
      sha256 "<SHA256_X86_64_APPLE_DARWIN>"
    end
    on_arm do
      url "https://github.com/elci-group/kaptaind/releases/download/v<VERSION>/kaptaind-<VERSION>-aarch64-apple-darwin.tar.gz"
      sha256 "<SHA256_AARCH64_APPLE_DARWIN>"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/elci-group/kaptaind/releases/download/v<VERSION>/kaptaind-<VERSION>-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "<SHA256_X86_64_UNKNOWN_LINUX_GNU>"
    end
    on_arm do
      url "https://github.com/elci-group/kaptaind/releases/download/v<VERSION>/kaptaind-<VERSION>-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "<SHA256_AARCH64_UNKNOWN_LINUX_GNU>"
    end
  end

  depends_on "git"

  def install
    bin.install "kaptaind"
    bin.install "kaptaind-cli"
  end

  test do
    assert_predicate bin/"kaptaind", :exist?
    assert_predicate bin/"kaptaind-cli", :exist?
  end
end
