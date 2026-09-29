class OmaircTui < Formula
  desc "Keyboard-first IRC client for the terminal"
  homepage "https://omairc.app/"
  license "MIT"
  version "1.0.5"

  on_macos do
    on_arm do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-arm64.tar.gz"
      sha256 "f88e41fea1276db8b76782a97f26f42cb0b87dba89c2cfb1b72115fb9fcb9e6a"
    end

    on_intel do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-amd64.tar.gz"
      sha256 "023e63649141f4cbb91546d5348083ce3ddba08f91ca17bba1eb8d310c311127"
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
