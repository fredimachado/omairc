cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "1.11.1"
  sha256 arm:   "2595661893a78973c45e348fb74c992ed21da6b4a523b43559436b95dd9a07f2",
         intel: "7a6ac85940cf8592987487cd0753a712645cf87d28c63fb49b67fb088e9a7934"

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
