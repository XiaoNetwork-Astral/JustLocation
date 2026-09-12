import { createHash } from 'node:crypto';
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { exe, output, root, run } from '../tool/build.mjs';

export function pack() {
  const stage = resolve(output, 'module');
  if (dirname(stage) !== resolve(output))
    throw new Error('Module staging must stay inside build/.');
  rmSync(stage, { recursive: true, force: true });
  cpSync(join(root, 'packaging/module'), stage, { recursive: true });
  const inputs = [
    ['build/native/libjustlocation.so', 'zygisk/arm64-v8a.so'],
    ['build/cargo/aarch64-linux-android/release/justlocationd', 'bin/justlocationd'],
    ['build/bridge/classes.dex', 'bridge/classes.dex'],
    // Keep the companion APK under bin to match the installer.

    ['dist/justlocation-joystick.apk', 'bin/joystick.apk'],
    ['build/native/libjustlocation_runtime.so', 'lib/libjustlocation_runtime.so'],
    ['build/native/_deps/shadowhook-build/libshadowhook.so', 'lib/libshadowhook.so'],
    [
      'build/native/_deps/shadowhook-build/libshadowhook_nothing.so',
      'lib/libshadowhook_nothing.so',
    ],
    ['build/native/_deps/lsplant-src/LICENSE', 'licenses/lsplant.txt'],
    [
      'build/native/_deps/lsplant-src/lsplant/src/main/jni/external/dex_builder/LICENSE',
      'licenses/dexbuilder.txt',
    ],
    [
      'build/native/_deps/lsplant-src/lsplant/src/main/jni/external/dex_builder/external/parallel_hashmap/LICENSE',
      'licenses/parallel-hashmap.txt',
    ],
    ['build/native/_deps/shadowhook-src/LICENSE', 'licenses/shadowhook.txt'],
    [
      'build/native/_deps/shadowhook-src/shadowhook/src/main/cpp/third_party/xdl/LICENSE',
      'licenses/xdl.txt',
    ],
    [
      'build/native/_deps/shadowhook-src/shadowhook/src/main/cpp/third_party/lss/LICENSE',
      'licenses/lss.txt',
    ],
    ['zygisk/include/zygisk.hpp', 'licenses/zygisk.hpp'],
  ];
  for (const [source, target] of inputs) {
    const destination = join(stage, target);
    const from = join(root, source);

    if (!existsSync(from)) throw new Error(`Missing ${source}; run node build.mjs build first.`);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(from, destination);
  }
  // Web UI sources and their dependencies are not included in the module ZIP.

  writeChecksums(stage);
  writeManifest(stage);
  mkdirSync(join(root, 'dist'), { recursive: true });
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  const zip = join(root, 'dist/justlocation-0.1.0-dev-arm64.zip');
  run(jar, ['--create', '--file', zip, '--no-manifest', '-C', stage, '.']);
  run(jar, ['--list', '--file', zip]);
}

/* Generate per-file digests for runtime verification from the current build output. */
function writeChecksums(stage) {
  let count = 0;
  for (const file of walkFiles(stage)) {
    const digest = createHash('sha256').update(readFileSync(file)).digest('hex');
    writeFileSync(`${file}.sha256`, `${digest}\n`);
    count += 1;
  }
  console.log(`checksums: ${count} files`);
}

/* Generate the sorted installation manifest; the installer verifies bytes from the ZIP. */
function writeManifest(stage) {
  const path = join(stage, 'checksums');
  const lines = [];
  for (const file of walkFiles(stage)) {
    if (file.endsWith('.sha256') || file === path) continue;
    const name = relative(stage, file).split(sep).join('/');
    const digest = createHash('sha256').update(readFileSync(file)).digest('hex');
    lines.push(`${digest}  ${name}`);
  }
  lines.sort();
  writeFileSync(path, `${lines.join('\n')}\n`);
  console.log(`manifest: ${lines.length} entries`);
}

function walkFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const full = join(directory, entry.name);
    return entry.isDirectory() ? walkFiles(full) : [full];
  });
}
