class OmaircTui < Formula
  desc "Keyboard-first IRC client for the terminal"
  homepage "https://omairc.app/"
  license "MIT"
  version "1.10.3"

  on_macos do
    on_arm do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-arm64.tar.gz"
      sha256 "9fff354ea706b72b246092e1de17609f57f9af03cb7e6799dcb14edc984674af"
    end

    on_intel do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-amd64.tar.gz"
      sha256 "bb3a47956157fea3186f829c6ead47eba72268d66725fb99e6017a82516b9cb7"
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
