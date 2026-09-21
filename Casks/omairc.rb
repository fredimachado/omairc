cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "0.9.0"
  sha256 arm:   "7bd608117d7ecec41c59bd258c8f8546c4ee00f054efb78cd2cfa778b68a8b6f",
         intel: "c33780fdcb69de5afa880c65056dfb9f744bb8399b96529818f4e2c2608fc27a"

  url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-#{version}-macos-#{arch}.zip",
      verified: "github.com/fredimachado/omairc/"
  name "Omairc"
  desc "Keyboard-first IRC client"
  homepage "https://omairc.app"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :big_sur

  app "omairc.app"

  # Do not symlink Contents/MacOS/omairc into bin/. Qt then treats that path as
  # the executable, skips the bundle qt.conf, and fails to load Quick Controls.
  preflight do
    wrapper = staged_path/"omairc"
    wrapper.write <<~SH
      #!/bin/sh
      exec "#{appdir}/omairc.app/Contents/MacOS/omairc" "$@"
    SH
    File.chmod(0755, wrapper)
  end

  binary "omairc"

  uninstall quit: "app.omairc.Omairc"

  zap trash: [
    "~/Library/Preferences/State/omairc",
    "~/Library/Preferences/omairc",
    "~/Library/Saved Application State/app.omairc.Omairc.savedState",
  ]

  caveats do
    <<~EOS
      The local CLI talks to the running window. Start Omairc first.
    EOS
  end
end
