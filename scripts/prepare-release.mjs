import { readFile, writeFile, appendFile } from "node:fs/promises";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const tauri = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
const cargo = await readFile("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargo.match(/^version = "([^"]+)"/m)?.[1];
const tag = process.env.GITHUB_REF_NAME;
if (
  pkg.version !== tauri.version ||
  pkg.version !== cargoVersion ||
  tag !== `v${pkg.version}`
) {
  throw new Error(
    "태그와 package.json, Cargo.toml, tauri.conf.json의 버전이 일치해야 합니다.",
  );
}
const pubkey = process.env.TAURI_SIGNING_PUBLIC_KEY?.trim();
const repository = process.env.GITHUB_REPOSITORY;
if (!pubkey || !repository || !process.env.TAURI_SIGNING_PRIVATE_KEY) {
  throw new Error(
    "TAURI_SIGNING_PUBLIC_KEY 변수와 TAURI_SIGNING_PRIVATE_KEY 시크릿을 설정해 주세요.",
  );
}
const release = {
  bundle: { createUpdaterArtifacts: true },
  plugins: {
    updater: {
      pubkey,
      endpoints: [
        `https://github.com/${repository}/releases/latest/download/latest.json`,
      ],
    },
  },
};
await writeFile(
  "src-tauri/tauri.release.conf.json",
  JSON.stringify(release, null, 2),
);

// Only export configured signing values. Empty Apple variables break unsigned builds.
if (process.env.GITHUB_ENV) {
  for (const name of [
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
  ]) {
    const value = process.env[`ZIPSA_${name}`];
    if (value) {
      const delimiter = `ZIPSA_${crypto.randomUUID()}`;
      await appendFile(
        process.env.GITHUB_ENV,
        `${name}<<${delimiter}\n${value}\n${delimiter}\n`,
      );
    }
  }
}
