cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "1.11.0"
  sha256 arm:   "a977c30900f3739a5a6e28e05325cc0d0c1862f39a45b7b7546b0ba95ebec0ab",
         intel: "ba9af9a31b7cccf7235a33ed7f0745275c0a2812392e7cf29db355a7b6ef7c6f"

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

  # Exec wrapper, not a symlink: the binary must run from inside the bundle so
  # NSBundle.mainBundle and notifications resolve.
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
