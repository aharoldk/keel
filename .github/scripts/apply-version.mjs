import fs from "node:fs";

const version = process.argv[2];
if (!version) {
  console.error("usage: apply-version.mjs <version>");
  process.exit(1);
}

const read = (path) => JSON.parse(fs.readFileSync(path, "utf8"));
const write = (path, value) => fs.writeFileSync(path, JSON.stringify(value, null, 2) + "\n");

const pkg = read("package.json");
pkg.version = version;
write("package.json", pkg);

const pkgLock = read("package-lock.json");
pkgLock.version = version;
if (pkgLock.packages?.[""]) pkgLock.packages[""].version = version;
write("package-lock.json", pkgLock);

const uiPkg = read("ui/package.json");
uiPkg.version = version;
write("ui/package.json", uiPkg);

const conf = read("src-tauri/tauri.conf.json");
conf.version = version;

const rc = /^(\d+)\.(\d+)\.(\d+)-rc\.(\d+)$/.exec(version);
if (rc) {
  const patch = Number(rc[3]);
  const n = Number(rc[4]);
  if (patch === 0 || n > 65535) {
    console.error(`cannot map ${version} to an MSI version that stays below the stable release`);
    process.exit(1);
  }
  conf.bundle.windows ??= {};
  conf.bundle.windows.wix ??= {};
  conf.bundle.windows.wix.version = `${rc[1]}.${rc[2]}.${patch - 1}.${n}`;
} else if (conf.bundle?.windows?.wix) {
  delete conf.bundle.windows.wix.version;
  if (Object.keys(conf.bundle.windows.wix).length === 0) delete conf.bundle.windows.wix;
  if (Object.keys(conf.bundle.windows).length === 0) delete conf.bundle.windows;
}
write("src-tauri/tauri.conf.json", conf);

const cargo = fs.readFileSync("src-tauri/Cargo.toml", "utf8").split("\n");
const cargoAt = cargo.findIndex((line) => line.startsWith("version = "));
if (cargoAt === -1) {
  console.error("version field not found in Cargo.toml");
  process.exit(1);
}
cargo[cargoAt] = `version = "${version}"`;
fs.writeFileSync("src-tauri/Cargo.toml", cargo.join("\n"));

const lock = fs.readFileSync("Cargo.lock", "utf8").split("\n");
let locked = false;
for (let i = 0; i < lock.length - 1; i++) {
  if (lock[i].trim() === 'name = "keel"' && lock[i + 1].startsWith("version = ")) {
    lock[i + 1] = `version = "${version}"`;
    locked = true;
    break;
  }
}
if (!locked) {
  console.error("keel package not found in Cargo.lock");
  process.exit(1);
}
fs.writeFileSync("Cargo.lock", lock.join("\n"));

console.log(`applied version ${version}`);
