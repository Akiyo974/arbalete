# The cask the release workflow (.github/workflows/release.yml) publishes to the
# amenophis/homebrew-tap repository, with the release's version and checksum.
cask "arbalete" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "https://github.com/amenophis/arbalete/releases/download/v#{version}/Arbalete-#{version}-arm64.dmg"
  name "Arbalete"
  desc "Browser inspired by Arc, on Chromium"
  homepage "https://github.com/amenophis/arbalete"

  depends_on arch: :arm64
  depends_on macos: ">= :sonoma"

  app "Arbalete.app"

  # Not notarized yet: macOS would block the first launch of what Homebrew downloaded. Installing
  # from this tap already trusts it, so the app opens at once.
  postflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{appdir}}/Arbalete.app"]
  end

  zap trash: [
    "~/Library/Application Support/Arbalete",
    "~/Library/Caches/Arbalete",
    "~/Library/Preferences/io.github.amenophis.arbalete.plist",
  ]
end
