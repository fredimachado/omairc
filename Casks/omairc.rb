cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "0.9.1"
  sha256 arm:   "a3fcd34eaa5f9740b793618be6250b2c61ec8afb3648d19c9b8e2fe84127ae68",
         intel: "ec7ecff03af63fc2760d5586e45be89c23ba61a8ed84a1f52ca1e4b4b17c7835"

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
