import fs from 'fs';
import path from 'path';

const root = path.resolve(new URL(import.meta.url).pathname, '..', '..');
const pkgPath = path.join(root, 'package.json');
const pkgLockPath = path.join(root, 'package-lock.json');
const tauriConfPath = path.join(root, 'src-tauri', 'tauri.conf.json');
const cargoPath = path.join(root, 'src-tauri', 'Cargo.toml');

function bumpVersion(version) {
  const parts = version.split('.').map((n) => parseInt(n, 10));
  if (parts.length < 3) {
    while (parts.length < 3) {
      parts.push(0);
    }
  }
  parts[2] = (parts[2] || 0) + 1;
  return parts.slice(0, 3).join('.');
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, obj) {
  fs.writeFileSync(filePath, JSON.stringify(obj, null, 2) + '\n', 'utf8');
}

const pkg = readJson(pkgPath);
const oldVersion = pkg.version;
const newVersion = bumpVersion(oldVersion);
pkg.version = newVersion;
writeJson(pkgPath, pkg);

if (fs.existsSync(pkgLockPath)) {
  const pkgLock = readJson(pkgLockPath);
  pkgLock.version = newVersion;
  if (pkgLock.packages && pkgLock.packages['']) {
    pkgLock.packages[''].version = newVersion;
  }
  writeJson(pkgLockPath, pkgLock);
}

const tauriConf = readJson(tauriConfPath);
if (tauriConf.version !== newVersion) {
  tauriConf.version = newVersion;
  writeJson(tauriConfPath, tauriConf);
}

const cargo = fs.readFileSync(cargoPath, 'utf8');
const cargoUpdated = cargo.replace(/^(version\s*=\s*").+("$)/m, `$1${newVersion}$2`);
if (cargoUpdated === cargo) {
  console.warn('Warning: could not automatically update src-tauri/Cargo.toml version field.');
} else {
  fs.writeFileSync(cargoPath, cargoUpdated, 'utf8');
}

console.log(`Auto-version bump: ${oldVersion} -> ${newVersion}`);
