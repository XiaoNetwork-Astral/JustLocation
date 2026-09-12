import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { config, exe, output, root, run, sdk } from '../tool/build.mjs';

export function build(tests = false) {
  const host = { win32: 'windows-x86_64', linux: 'linux-x86_64', darwin: 'darwin-x86_64' }[
    process.platform
  ];
  if (!host) throw new Error(`Unsupported NDK host: ${process.platform}`);
  const clang = join(
    sdk(),
    'ndk',
    config.ndkVer,
    'toolchains/llvm/prebuilt',
    host,
    'bin',
    `clang${exe}`,
  );
  const androidTarget = `aarch64-linux-android${config.platform.replace('android-', '')}`;
  const result = run(
    `cargo${exe}`,
    [
      tests ? 'test' : 'build',
      ...(tests ? ['--lib', '--no-run', '--message-format=json'] : []),
      '--locked',
      '--release',
      '--target',
      config.rustTarget,
      '--target-dir',
      join(output, 'cargo'),
      '--manifest-path',
      join(root, 'backend/Cargo.toml'),
    ],
    root,
    {
      CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: clang,
      CC_aarch64_linux_android: clang,
      AR_aarch64_linux_android: join(dirname(clang), `llvm-ar${exe}`),
      CFLAGS_aarch64_linux_android: `--target=${androidTarget}`,
      CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS: `-C link-arg=--target=${androidTarget} -C link-arg=-Wl,-z,max-page-size=16384`,
    },
    tests,
  );
  if (tests) {
    const artifact = result
      .trim()
      .split(/\r?\n/)
      .map((line) => JSON.parse(line))
      .find((item) => item.reason === 'compiler-artifact' && item.profile.test && item.executable);
    if (!artifact) throw new Error('Android backend test executable was not produced.');
    mkdirSync(join(output, 'probe'), { recursive: true });
    copyFileSync(artifact.executable, join(output, 'probe/backend-tests'));
  }
}

export function test() {
  run(`cargo${exe}`, [
    'test',
    '--locked',
    '--manifest-path',
    join(root, 'backend/Cargo.toml'),
    '--target-dir',
    join(output, 'cargo'),
  ]);
}
