# Fibo phone app

This Expo app is a companion to Fibo on your computer. It currently requires a source build and a [self-hosted relay](../docs/relay/self-host.en.md). There is no public mobile installer or default hosted relay.

Use the phone and desktop from the same commit. Remote protocol 2 refuses mismatched versions with an update message. After updating the desktop files, reopen Fibo so its running host also has the chat approval guard. Updating a file alone does not update an existing process.

## Build and check

Install Bun, then run from this directory:

```sh
bun install --frozen-lockfile
bun test
bun run typecheck
```

For Android, install the Android SDK and JDK 17, then:

```sh
bunx expo prebuild --platform android --no-install
cd android
./gradlew assembleRelease --no-daemon
```

The APK is under `android/app/build/outputs/apk/release/`. This local build uses the generated project's development signing setup; it is not an app-store release. To build and install a development version directly on a connected phone, use `bunx expo run:android` from `mobile/`.

For iOS, use macOS with Xcode and `bunx expo run:ios`; running on an iPhone requires your own signing configuration. Generated `android/` and `ios/` directories are not source files: configure native behavior in `app.json` and Expo config plugins.

## Connect

Open Settings › Phone & remote on the computer, enter your relay URL and registration key, enable it, and scan the pairing QR code with the app. The registration key admits computers to your relay; the QR pairs one phone. Keep both private.

Messages wait while an approval or folder-trust prompt is open. Answer that prompt explicitly first. If versions differ, update both apps; do not re-pair to work around a version error.

Unit tests and web previews do not verify native storage, authentication, image picking, or keyboard behavior. Use the full [real-phone walkthrough](../docs/phone-app/walkthrough.md) before accepting a candidate.
