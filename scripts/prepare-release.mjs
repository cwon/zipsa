import { readFile, appendFile } from "node:fs/promises";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const tauri = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
const cargo = await readFile("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargo.match(/^version = "([^"]+)"/m)?.[1];
const tag = process.env.GITHUB_REF_NAME;
if (
  !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(pkg.version) ||
  pkg.version !== tauri.version ||
  pkg.version !== cargoVersion ||
  tag !== `v${pkg.version}`
) {
  throw new Error(
    "태그와 package.json, Cargo.toml, tauri.conf.json의 버전이 일치하는 정식 버전(x.y.z)이어야 합니다.",
  );
}
console.log(`Release version verified: ${pkg.version}`);

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
