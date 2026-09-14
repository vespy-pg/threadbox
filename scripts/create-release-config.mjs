import { writeFileSync } from "node:fs";

const publicKey = process.env.THREADBOX_UPDATER_PUBLIC_KEY?.trim();
if (!publicKey) throw new Error("THREADBOX_UPDATER_PUBLIC_KEY is required");

const config = {
  bundle: {
    createUpdaterArtifacts: true,
  },
  plugins: {
    updater: {
      pubkey: publicKey,
      endpoints: ["https://github.com/vespy-pg/threadbox/releases/latest/download/latest.json"],
      windows: {
        installMode: "passive",
      },
    },
  },
};

const certificateThumbprint = process.env.THREADBOX_WINDOWS_CERTIFICATE_THUMBPRINT?.trim();
if (process.platform === "win32") {
  const timestampUrl = process.env.THREADBOX_WINDOWS_TIMESTAMP_URL?.trim();
  if (!certificateThumbprint || !timestampUrl) {
    throw new Error("Windows certificate thumbprint and timestamp URL are required");
  }
  config.bundle.windows = {
    certificateThumbprint,
    digestAlgorithm: "sha256",
    timestampUrl,
  };
}

writeFileSync("src-tauri/tauri.release.conf.json", `${JSON.stringify(config, null, 2)}\n`);
console.log("Created ephemeral release configuration");
