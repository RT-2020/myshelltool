#!/usr/bin/env node
// Bump every release version source in one pass:
// package.json, package-lock.json, tauri.conf.json, both Cargo manifests,
// and both Cargo.lock files.

import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync, execSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

const args = process.argv.slice(2);
const doCommit = args.includes('--commit');
const version = args.find((arg) => !arg.startsWith('--'));

const packageLockPath = 'package-lock.json';
const srcTauriLockPath = 'src-tauri/Cargo.lock';
const coreLockPath = 'crates/myshelltool-core/Cargo.lock';

if (!version) {
  console.error('Usage: node scripts/bump-version.mjs <version> [--commit]');
  console.error('Example: node scripts/bump-version.mjs 0.6.1');
  process.exit(1);
}

if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`Error: "${version}" is not a valid semver version. Use x.y.z, for example 0.6.1.`);
  process.exit(1);
}

function projectPath(file) {
  return resolve(root, file);
}

function readProjectFile(file) {
  return readFileSync(projectPath(file), 'utf8');
}

function writeProjectFile(file, content) {
  writeFileSync(projectPath(file), content);
}

function replaceOrFail(file, content, pattern, replacement) {
  const next = content.replace(pattern, replacement);
  if (next === content) {
    throw new Error(`${file}: version field was not found; file structure may have changed`);
  }
  return next;
}

function updatePackageLock(content) {
  const lock = JSON.parse(content);
  lock.version = version;
  lock.packages ??= {};
  lock.packages[''] ??= {};
  lock.packages[''].version = version;
  return JSON.stringify(lock, null, 2) + '\n';
}

function cargoLockPackageVersion(content, packageName) {
  const pattern = new RegExp(`\\[\\[package\\]\\]\\s+name = "${packageName}"\\s+version = "([^"]+)"`, 'm');
  return content.match(pattern)?.[1];
}

function assertVersion(label, actual) {
  if (actual !== version) {
    throw new Error(`${label} is ${actual ?? '<missing>'}, expected ${version}`);
  }
}

const targets = [
  {
    file: 'package.json',
    replace: (content) => replaceOrFail(
      'package.json',
      content,
      /("version"\s*:\s*")\d+\.\d+\.\d+[^"]*(")/,
      `$1${version}$2`
    )
  },
  {
    file: packageLockPath,
    replace: updatePackageLock
  },
  {
    file: 'src-tauri/tauri.conf.json',
    replace: (content) => replaceOrFail(
      'src-tauri/tauri.conf.json',
      content,
      /("version"\s*:\s*")\d+\.\d+\.\d+[^"]*(")/,
      `$1${version}$2`
    )
  },
  {
    file: 'src-tauri/Cargo.toml',
    replace: (content) => replaceOrFail(
      'src-tauri/Cargo.toml',
      content,
      /(name\s*=\s*"myshelltool"\s*\n\s*version\s*=\s*")\d+\.\d+\.\d+[^"]*(")/,
      `$1${version}$2`
    )
  },
  {
    file: 'crates/myshelltool-core/Cargo.toml',
    replace: (content) => replaceOrFail(
      'crates/myshelltool-core/Cargo.toml',
      content,
      /(name\s*=\s*"myshelltool-core"\s*\n\s*version\s*=\s*")\d+\.\d+\.\d+[^"]*(")/,
      `$1${version}$2`
    )
  }
];

console.log(`Bump version -> ${version}\n`);

for (const target of targets) {
  const before = readProjectFile(target.file);
  const after = target.replace(before);
  writeProjectFile(target.file, after);
  console.log(`  updated ${target.file}`);
}

console.log('\nRefresh package-lock.json...');
try {
  execSync('npm install --package-lock-only --ignore-scripts', {
    cwd: root,
    stdio: 'inherit'
  });
  console.log('  package-lock.json synchronized');
} catch (error) {
  console.error(`  npm lockfile sync failed: ${error.message}`);
  process.exit(1);
}

console.log('\nRefresh Cargo.lock files...');
try {
  execSync(`cargo update -p myshelltool --precise ${version}`, {
    cwd: projectPath('src-tauri'),
    stdio: 'inherit'
  });
  execSync(`cargo update -p myshelltool-core --precise ${version}`, {
    cwd: projectPath('src-tauri'),
    stdio: 'inherit'
  });
  execSync(`cargo update -p myshelltool-core --precise ${version}`, {
    cwd: projectPath('crates/myshelltool-core'),
    stdio: 'inherit'
  });
  console.log('  Cargo.lock files synchronized');
} catch (error) {
  console.error(`  Cargo.lock sync failed: ${error.message}`);
  process.exit(1);
}

console.log('\nValidate version consistency...');
try {
  const packageJson = JSON.parse(readProjectFile('package.json'));
  const packageLock = JSON.parse(readProjectFile(packageLockPath));
  const tauriConfig = JSON.parse(readProjectFile('src-tauri/tauri.conf.json'));
  const tauriToml = readProjectFile('src-tauri/Cargo.toml');
  const coreToml = readProjectFile('crates/myshelltool-core/Cargo.toml');
  const tauriLock = readProjectFile(srcTauriLockPath);
  const coreLock = readProjectFile(coreLockPath);

  assertVersion('package.json', packageJson.version);
  assertVersion('package-lock.json', packageLock.version);
  assertVersion('package-lock.json packages[""]', packageLock.packages?.['']?.version);
  assertVersion('src-tauri/tauri.conf.json', tauriConfig.version);
  assertVersion('src-tauri/Cargo.toml', tauriToml.match(/name\s*=\s*"myshelltool"\s*\n\s*version\s*=\s*"([^"]+)"/)?.[1]);
  assertVersion('crates/myshelltool-core/Cargo.toml', coreToml.match(/name\s*=\s*"myshelltool-core"\s*\n\s*version\s*=\s*"([^"]+)"/)?.[1]);
  assertVersion('src-tauri/Cargo.lock myshelltool', cargoLockPackageVersion(tauriLock, 'myshelltool'));
  assertVersion('src-tauri/Cargo.lock myshelltool-core', cargoLockPackageVersion(tauriLock, 'myshelltool-core'));
  assertVersion('crates/myshelltool-core/Cargo.lock myshelltool-core', cargoLockPackageVersion(coreLock, 'myshelltool-core'));
  console.log('  all release version sources match');
} catch (error) {
  console.error(`  version consistency check failed: ${error.message}`);
  process.exit(1);
}

if (doCommit) {
  console.log('\nCommit release bump...');
  try {
    const files = [
      ...new Set([
        ...targets.map((target) => target.file),
        srcTauriLockPath,
        coreLockPath
      ])
    ];
    execFileSync('git', ['add', ...files], { cwd: root, stdio: 'inherit' });
    execFileSync('git', [
      'commit',
      '-m', `Release v${version}`,
      '-m', `This keeps all release manifests and lockfiles aligned at ${version} so the tag, installer metadata, updater manifest, and package locks agree.`,
      '-m', 'Constraint: Release commits must pass the local Lore commit guard and include the OmX co-author trailer',
      '-m', 'Rejected: Commit with a conventional one-line bump message | the hook blocks messages without Lore trailers',
      '-m', 'Confidence: high',
      '-m', 'Scope-risk: narrow',
      '-m', 'Directive: Do not hand-edit release versions; use scripts/bump-version.mjs so every manifest and lockfile is audited',
      '-m', `Tested: node scripts/bump-version.mjs ${version}; built-in version consistency audit`,
      '-m', 'Not-tested: GitHub Actions release artifacts before pushing the tag',
      '-m', 'Co-authored-by: OmX <omx@oh-my-codex.dev>'
    ], { cwd: root, stdio: 'inherit' });
    console.log(`  committed Release v${version}`);
  } catch (error) {
    console.error(`  git commit failed: ${error.message}`);
    process.exit(1);
  }
}

console.log(`\nDone. Next: git tag v${version} && git push origin master --tags`);
if (!doCommit) {
  console.log('Changes were written but not committed because --commit was not provided.');
}
