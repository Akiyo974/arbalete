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

  zap trash: [
    "~/Library/Application Support/Arbalete",
    "~/Library/Caches/Arbalete",
    "~/Library/Preferences/io.github.amenophis.arbalete.plist",
  ]

  caveats <<~EOS
    Arbalete is not notarized by Apple yet, so macOS blocks its first launch.
    Open it once from System Settings > Privacy & Security > Open Anyway, or run:
      xattr -dr com.apple.quarantine /Applications/Arbalete.app
  EOS
end
