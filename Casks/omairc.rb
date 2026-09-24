cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "1.0.1"
  sha256 arm:   "70ad8b35081ea1a056e5cfa608d1047b6d88b9c8c883258ff1dda8d7ccb749a1",
         intel: "a7da7775fc0fd87397896005499e0b8e0ad010ccaffffbbe6d41aa405a08d499"

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
