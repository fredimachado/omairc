class OmaircTui < Formula
  desc "Keyboard-first IRC client for the terminal"
  homepage "https://omairc.app/"
  license "MIT"
  version "1.0.6"

  on_macos do
    on_arm do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-arm64.tar.gz"
      sha256 "2f45a84e8fcde0bb06d53946bfe9f0b2ccde316559abfa4ad5c5f659076f1071"
    end

    on_intel do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-amd64.tar.gz"
      sha256 "f2af4fc5aa94bb63088e6e06905b23f436befdaef818c8e36245201ca8072d09"
    end
  end

  livecheck do
    url "https://github.com/fredimachado/omairc/releases/latest"
    strategy :github_latest
  end

  def install
    bin.install "omairc-tui"
  end
end
