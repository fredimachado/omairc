class OmaircTui < Formula
  desc "Keyboard-first IRC client for the terminal"
  homepage "https://omairc.app/"
  license "MIT"
  version "1.11.1"

  on_macos do
    on_arm do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-arm64.tar.gz"
      sha256 "5ee1a4df5e52fbe9518417ade146bf0f3223e8f6e14cab76a05496fc54b1ddc7"
    end

    on_intel do
      url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-tui-#{version}-darwin-amd64.tar.gz"
      sha256 "9af87e3c2b0d9dd7231c685303a426adc6c190af5314212a503e0e44a0b47428"
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
