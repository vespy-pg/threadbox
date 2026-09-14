import { readFileSync } from "node:fs";

const releaseTag = process.env.THREADBOX_RELEASE_TAG;
if (!releaseTag) throw new Error("THREADBOX_RELEASE_TAG is required");

const packageVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const cargoManifest = readFileSync("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargoManifest.match(/^version = "([^"]+)"$/m)?.[1];
const tagVersion = releaseTag.replace(/^v/, "");

const versions = { packageVersion, tauriVersion, cargoVersion, tagVersion };
if (!cargoVersion || new Set(Object.values(versions)).size !== 1) {
  throw new Error(`Release versions do not match: ${JSON.stringify(versions)}`);
}

console.log(`Verified Threadbox ${tagVersion}`);
