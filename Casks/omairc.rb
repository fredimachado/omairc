cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "1.0.6"
  sha256 arm:   "95fd870457e01ca01c614df50c2c609b8d8608329d3975b1ba9903ef0b3d9d4c",
         intel: "a0a079c5bd7742ddb3ab2c71eea41b4b6b8750a063caf0bc25017f5bda40a54d"

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
