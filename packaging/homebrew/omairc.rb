cask "omairc" do
  arch arm: "arm64", intel: "x64"

  version "0.8.2"
  sha256 arm:   "44bec2819bf30ed555e9f5d21aab816e5fe8587ec4aec54db1009d131b3fab83",
         intel: "59393c50247011024728999f5af56cb19fa550f80d2dfe11a5bf0f51c39cb3ed"

  url "https://github.com/fredimachado/omairc/releases/download/v#{version}/omairc-#{version}-macos-#{arch}.zip",
      verified: "github.com/fredimachado/omairc/"
  name "Omairc"
  desc "Keyboard-first IRC client"
  homepage "https://omairc.app"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "omairc.app"
  binary "#{appdir}/omairc.app/Contents/MacOS/omairc"

  uninstall quit: "app.omairc.Omairc"

  zap trash: [
    "~/Library/Preferences/State/omairc",
    "~/Library/Preferences/omairc",
    "~/Library/Saved Application State/app.omairc.Omairc.savedState",
  ]
end
