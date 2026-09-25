cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "1.0.4"
  sha256 arm:   "5568503b0f07b168aaac78cea3e00c2ae10d729751a2d47bdc5ef3074be0056c",
         intel: "4b0aeca2bba73be5349810d79ba9305c668f5c8f618aa4e49e0f60b4ecc47b8b"

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
