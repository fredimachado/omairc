class OmaircTui < Formula
  desc "Keyboard-first IRC client for the terminal"
  homepage "https://omairc.app/"
  license "MIT"
  version "1.0.4"

  on_macos do
    on_arm do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-arm64.tar.gz"
      sha256 "5d6079de8cc171b67dd74ae593594e22b83aca33b6b44523cf221edc6ab96e71"
    end

    on_intel do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-amd64.tar.gz"
      sha256 "e4d3b47326d1e5714a3593025bb1c15ebc1f41e0d68ba1f713557919ccadad91"
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
