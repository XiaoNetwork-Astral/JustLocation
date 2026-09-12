import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { exe, output, root, run } from '../tool/build.mjs';

export function pack() {
  const stage = resolve(output, 'module');
  if (dirname(stage) !== resolve(output)) throw new Error('Module staging must stay inside build/.');
  rmSync(stage, { recursive: true, force: true });
  cpSync(join(root, 'packaging/module'), stage, { recursive: true });
  const inputs = [
    ['build/native/libjustlocation.so', 'zygisk/arm64-v8a.so'],
    ['build/cargo/aarch64-linux-android/release/justlocationd', 'bin/justlocationd'],
    ['build/bridge/classes.dex', 'bridge/classes.dex'],
    // 模块自带的摇杆 App：与 LSPosed 把 manager.apk 放在 bin/ 一样，
    // 附属应用统一收在 bin/ 下，便于打包脚本与安装脚本对齐。
    ['dist/justlocation-joystick.apk', 'bin/joystick.apk'],
    ['build/native/libjustlocation_runtime.so', 'lib/libjustlocation_runtime.so'],
    ['build/native/_deps/shadowhook-build/libshadowhook.so', 'lib/libshadowhook.so'],
    ['build/native/_deps/shadowhook-build/libshadowhook_nothing.so', 'lib/libshadowhook_nothing.so'],
    ['build/native/_deps/lsplant-src/LICENSE', 'licenses/lsplant.txt'],
    ['build/native/_deps/lsplant-src/lsplant/src/main/jni/external/dex_builder/LICENSE', 'licenses/dexbuilder.txt'],
    ['build/native/_deps/lsplant-src/lsplant/src/main/jni/external/dex_builder/external/parallel_hashmap/LICENSE', 'licenses/parallel-hashmap.txt'],
    ['build/native/_deps/shadowhook-src/LICENSE', 'licenses/shadowhook.txt'],
    ['build/native/_deps/shadowhook-src/shadowhook/src/main/cpp/third_party/xdl/LICENSE', 'licenses/xdl.txt'],
    ['build/native/_deps/shadowhook-src/shadowhook/src/main/cpp/third_party/lss/LICENSE', 'licenses/lss.txt'],
    ['zygisk/include/zygisk.hpp', 'licenses/zygisk.hpp'],
  ];
  for (const [source, target] of inputs) {
    const destination = join(stage, target);
    const from = join(root, source);
    // 摇杆 App 的 APK 由 `android` 步骤产出；只跑 pack 时会缺，给一句能照着做的提示。
    if (!existsSync(from)) throw new Error(`Missing ${source}; run node build.mjs build first.`);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(from, destination);
  }
  // 模块**不带 Web UI**（2026-09-12 用户决定：先让模块没有控制面板）。
  // 因此这里不拷 `ui/dist`，上面也不再带 React / Leaflet / lucide / KernelSU SDK 的许可文件——
  // 包里没有那些代码，就不该留着它们的许可声明。`ui/` 源码与它的测试原样保留，随时可以再装回去。
  writeChecksums(stage);
  writeManifest(stage);
  mkdirSync(join(root, 'dist'), { recursive: true });
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  const zip = join(root, 'dist/justlocation-0.1.0-dev-arm64.zip');
  run(jar, ['--create', '--file', zip, '--no-manifest', '-C', stage, '.']);
  run(jar, ['--list', '--file', zip]);
}

/**
 * 给组装出来的每个文件写一份 `.sha256` 清单，与模块包一起分发。
 *
 * 这样运行脚本能在启动后台前确认二进制没被改动（`util_functions.sh` 提供 `verify_file`）。
 * 清单必须在打包时生成，不能提交到仓库：它描述的是这一次构建的字节内容。
 */
function writeChecksums(stage) {
  let count = 0;
  for (const file of walkFiles(stage)) {
    const digest = createHash('sha256').update(readFileSync(file)).digest('hex');
    writeFileSync(`${file}.sha256`, `${digest}\n`);
    count += 1;
  }
  console.log(`checksums: ${count} files`);
}

/**
 * 整包校验清单 `checksums`：每行 `<sha256>  <包内路径>`，按路径字母序。
 *
 * 与上面的逐文件 `.sha256` 解决的不是同一件事：
 *   - `.sha256` 给**运行期**用（模块目录里的文件有没有被动过）；
 *   - `checksums` 给**安装期**用（ZIP 里的字节有没有坏）。
 * 安装时 SKIPUNZIP=1、由 customize.sh 自己解压，所以这份清单必须留在 ZIP 内层，
 * 不能在打包脚本里直接核对——那样只能证明打包时的字节是对的，证明不了手里这份 ZIP。
 */
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

/** 列出目录下的所有文件（不含目录本身）。 */
function walkFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const full = join(directory, entry.name);
    return entry.isDirectory() ? walkFiles(full) : [full];
  });
}
