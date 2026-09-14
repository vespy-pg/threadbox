# Windows support and desktop releases

Threadbox uses one Tauri codebase for Linux and Windows. Windows is not a separate product or a
second application. Platform adapters are selected at compile time while the database, archive,
domain model and React interface remain shared.

## Windows adapters

- Microphone recording uses the default Windows input through WASAPI.
- System recording opens the default render endpoint as a WASAPI loopback stream.
- Screenshots capture the full primary display after Threadbox hides its own window.
- Secrets use Windows Credential Manager through the operating system keyring.
- Notifications use the native Tauri notification adapter.
- Firefox native messaging is registered for the current user in the Windows registry.
- Google and Microsoft OAuth callbacks use the same random loopback listener as Linux.

The screenshot action intentionally captures the primary display instead of launching Snipping
Tool. Modern Snipping Tool callbacks require an MSIX package, while the first Windows release uses
an NSIS installer.

## Release signing

There are two independent signatures:

1. A Windows code-signing certificate signs the executable and installer for publisher identity.
2. A Tauri updater key signs update artifacts so an installed copy can reject substituted files.

Never commit either private key. Configure these GitHub repository secrets:

- `WINDOWS_CERTIFICATE` - base64-encoded PFX code-signing certificate;
- `WINDOWS_CERTIFICATE_PASSWORD` - PFX export password;
- `TAURI_SIGNING_PRIVATE_KEY` - Tauri updater private key content;
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - updater key password.

Configure these GitHub repository variables:

- `TAURI_UPDATER_PUBLIC_KEY` - public updater key generated with the private key;
- `WINDOWS_TIMESTAMP_URL` - RFC 3161 timestamp service supplied by the certificate issuer.
- `GOOGLE_OAUTH_CLIENT_ID` - verified production Google Desktop app client ID;
- `MICROSOFT_OAUTH_CLIENT_ID` - production Microsoft public desktop client ID.

The public updater key is safe to distribute. Losing or replacing the private updater key prevents
existing installations from accepting future updates, so keep an offline backup.

## Publishing

Keep the versions in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` equal.
Create and push a `vMAJOR.MINOR.PATCH` tag, or manually run `Build desktop release` against an
existing tag. The workflow:

1. checks out the immutable tag and verifies all versions;
2. runs the UI tests on both operating systems;
3. imports the Windows certificate into the ephemeral runner;
4. creates a release-only Tauri configuration from GitHub secrets and variables;
5. builds AppImage, Debian and NSIS packages;
6. signs updater artifacts and publishes a draft prerelease with `latest.json`;
7. removes the imported certificate even when the build fails.

Only a build with `VITE_UPDATER_ENABLED=true` shows the update controls. Users explicitly check for
an update and review its version and notes before installation. Tauri verifies the artifact
signature before installing it.

## Clean Windows acceptance run

Run this checklist in a new standard-user Windows account before promoting a draft release:

- install the signed NSIS package and confirm the publisher identity;
- create a project, thread, document and meeting, then restart the application;
- record microphone-only, system-only and combined meeting audio;
- capture a screenshot and confirm that Threadbox itself is absent from it;
- download a local speech model and transcribe a short recording;
- configure cloud speech, transcribe a second recording and confirm the privacy prompt;
- connect Google, read and create calendar events, and run a multi-person free-time search;
- connect Gmail read-only, then grant compose and send separately and test each capability;
- connect Microsoft read-only, then grant compose and send separately and test each capability;
- configure one independent IMAP-only account and one SMTP-only account;
- export a backup on Windows, import it on Linux and repeat in the opposite direction;
- publish a higher test version, check for it in Settings and install it;
- disconnect every provider and verify that credentials disappear while project history remains.

Hardware and real-provider checks cannot be represented by a cross-platform compile job. Record the
tested Windows version, hardware, provider accounts and result in the release notes.
