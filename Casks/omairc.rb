cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "1.0.5"
  sha256 arm:   "0a0c2470f88a558ecd669b0371567e294157b5a0d97e1a1867c005be8b552adf",
         intel: "5113673a1166c628a37c7b5c5ef5d1652e32a0eef1e973a592b9016d97eafd3e"

  url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-#{version}-macos-#{arch}.zip"
  name "Omairc"
  desc "Keyboard-first IRC client"
  homepage "https://omairc.app/"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on :macos

  app "omairc.app"
  binary "omairc"

  # Do not symlink Contents/MacOS/omairc into bin/. Qt then treats that path as
  # the executable, skips the bundle qt.conf, and fails to load Quick Controls.
  preflight_steps do
    write_file "omairc", <<~SH
      #!/bin/sh
      exec "{{appdir}}/omairc.app/Contents/MacOS/omairc" "$@"
    SH
    set_permissions "omairc", "0755"
  end

  uninstall quit: "app.omairc.Omairc"

  zap trash: [
    "~/Library/Preferences/omairc",
    "~/Library/Preferences/State/omairc",
    "~/Library/Saved Application State/app.omairc.Omairc.savedState",
  ]

  caveats do
    <<~EOS
      The local CLI talks to the running window. Start Omairc first.
    EOS
  end
end
