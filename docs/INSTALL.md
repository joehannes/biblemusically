# Installing Studio Lightkid

Every build comes from the same release. You get a download link with a one-time code, valid for fifteen
minutes — long enough to walk to another machine, short enough that the code in a chat log is worthless
by the time anyone finds it.

Trial and paid downloads are the same file. The download is not the product; the licence is. Making
people fight for the installer helps nobody.

---

## Linux (Debian, Ubuntu, Mint…)

```bash
sudo dpkg -i "AI Music Video Studio_*_amd64.deb"
sudo apt-get install -f      # only if dpkg complains about a missing dependency
```

**Standalone, no install:** the `.AppImage` from the same release needs nothing installed.

```bash
chmod +x "AI Music Video Studio_*.AppImage"
./"AI Music Video Studio_"*.AppImage
```

An `.rpm` is there too, for Fedora and openSUSE.

`ffmpeg` is worth having (`sudo apt install ffmpeg`) but is not required — every command-line step can run
on Modal instead, which is what the phone builds do.

## Windows

Run the `.msi` (or `.exe`). Windows will show **"Windows protected your PC"** because the build is not
signed with a code-signing certificate — those cost a few hundred a year and are not bought yet.

Click **More info → Run anyway**.

If that makes you uncomfortable, it should: it is exactly the dialog malware wants you to click through.
Check the file's SHA-256 against the release page before you do.

## macOS

Open the `.dmg`, drag the app to Applications. The first launch will be refused: *"cannot be opened
because the developer cannot be verified."* Same reason — no Apple Developer ID signature.

**Right-click the app → Open**, then confirm. Once, per version.

If macOS says the app is *damaged*, it is Gatekeeper's quarantine flag rather than actual damage:

```bash
xattr -dr com.apple.quarantine "/Applications/AI Music Video Studio.app"
```

## Android

The `.apk` installs directly. Android asks once for permission to install it, and where you grant that
has moved around over the years:

- **Android 8 and later:** open the APK, and when the block appears tap **Settings → Allow from this
  source**. The permission is per-app (your browser or file manager), not system-wide — which is the
  right way round.
- **Older versions:** Settings → Security → **Unknown sources**.

Turn it back off afterwards if you like; the installed app keeps working.

The file is named for its version — `lightkid_studio_0.107.0.apk` — so two of them in a downloads
folder are telling apart, and the newer one does not silently overwrite the older.

**Debug or release?** Both install the same way. The release APK is the one to hand to somebody else:
it is minified, roughly a tenth the size, and signed with the project's own release key. The debug APK
carries 350 MB of debug symbols and is only worth having when something needs diagnosing.

**An unsigned APK is a different matter, and it cannot be installed at all.** Android refuses it
outright — unlike an unknown *source*, which is one toggle, "unsigned" is not something anyone can
allow. If a build produces one, it is not a build anybody can use.

**Building one yourself:**

```bash
npm run build:apk           # arm64 — every Android phone since about 2017
npm run build:apk -- --all  # all four ABIs, roughly four times the size
```

It lands in `release/`, signed, correctly iconed, and named as above.

**About that signature.** Android identifies an app by the key it was signed with, not by its name. An
APK signed with a *different* key cannot update one already installed — the phone refuses, and the only
way through is to uninstall first, which takes the app's data with it. So every release has to be
signed with the same key, and that key lives at `~/.config/studio-lightkid/android-release.keystore`,
outside the repository. **Back it up.** Losing it means everyone who installed a previous version has
to uninstall and start again.

**What the phone does and does not do.** Everything except two things. It has no ffmpeg — command-line
steps run on Modal, which the app sets up for you. And it cannot drive a hidden browser, so Suno works
through your captured session rather than through automation. Guides, voice, microphone input, generation
and publishing all work.

## iOS — read this before downloading

**An unsigned app cannot be installed on a stock iPhone or iPad.** Apple requires a signature and there is
no "allow unknown sources" switch. Anyone telling you otherwise is describing a jailbroken device.

Two routes that genuinely work:

### TestFlight (easiest for you, needs a paid account from me)

Apple's own beta channel. It installs like any App Store app, updates itself, and lasts 90 days per
build. It needs an Apple Developer Program membership ($99/year) and a review of each build. If you want
iOS, ask — this is the route worth setting up.

### AltStore or SideStore (works today, needs a computer once)

You sign the app with **your own** Apple ID, on your own device:

1. Install [AltStore](https://altstore.io) or [SideStore](https://sidestore.io) — a one-off setup from a
   Mac or Windows PC.
2. Open the `.ipa` from the release with it.
3. Sign in with your Apple ID when asked.

The catch is real: a **free** Apple ID signature expires after **7 days** and the app must be re-signed
(AltStore can do it automatically while on the same Wi-Fi). A **paid** developer account lasts a year.

### Why not just ship a signed IPA?

Signing outside the App Store still ties the build to specific device UDIDs (an ad-hoc profile) or to an
Enterprise certificate Apple grants only to large organisations and revokes for exactly this use. Neither
is a way to hand an app to whoever downloads it. iOS is simply not Android here.

---

## Updates

The app checks every fifteen minutes.

- **Minor and patch updates** (0.87 → 0.88, 0.88.1) — an **Update** button appears. One click, covered by
  every licence including lifetime.
- **A new major version** (1.x → 2.x) — an **Upgrade** button appears and stays. It is a separate purchase,
  so it is announced rather than applied. Your current version keeps working exactly as it did, and you
  can dismiss the announcement permanently.

Trial installs do not get the update button. Trials are for deciding, and the version you downloaded is
the version you are deciding about.

## Verifying a download

Every release lists SHA-256 sums. Given that both Windows and macOS will warn you that this software is
unsigned, checking one is not paranoia — it is the only thing that actually tells you the file is the one
that was built.

```bash
sha256sum "AI Music Video Studio_0.88.0_amd64.deb"       # Linux
shasum -a 256 "AI Music Video Studio_0.88.0_x64.dmg"     # macOS
certutil -hashfile "AI Music Video Studio_0.88.0_x64.msi" SHA256   # Windows
```
