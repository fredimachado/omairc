cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "0.8.3"
  sha256 arm:   "07394edc34822294fc5e8716c3722a9ed51dcaa419112a8d1bdc1bf4835a088f",
         intel: "e40a22c522351f44d8446f08b70d193e3e00ae9d7bdba39345d7047594c85d8c"

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
