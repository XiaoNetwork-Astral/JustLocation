import net from 'node:net';
import { createInterface } from 'node:readline';
import { readFileSync } from 'node:fs';
import { once } from 'node:events';

// Read the token from the path advertised by our local emulator. adb emu uses
// its own home directory, which can differ on this Windows installation.
const command = process.argv.slice(2).join(' ');
if (!command || /[\r\n]/.test(command)) throw new Error('Provide one emulator console command.');
const socket = net.connect(5580, '127.0.0.1');
socket.setTimeout(5000, () => socket.destroy(new Error('AVD console timed out')));
const lines = createInterface({ input: socket, crlfDelay: Infinity });
const iterator = lines[Symbol.asyncIterator]();
async function response() {
  const body = [];
  for (;;) {
    const { value, done } = await iterator.next();
    if (done) throw new Error('AVD console disconnected before acknowledging the command.');
    if (value === 'OK') return body.join('\n');
    if (value.startsWith('KO:')) throw new Error(value);
    body.push(value);
  }
}
async function send(value) { socket.write(value + '\n'); return response(); }
try {
  await once(socket, 'connect');
  const greeting = await response();
  if (greeting.includes('Authentication required')) {
    const tokenPath = greeting.match(/^'([^'\r\n]+[\\/]\.emulator_console_auth_token)'$/m)?.[1];
    if (!tokenPath) throw new Error('AVD console did not advertise its token path.');
    const token = readFileSync(tokenPath, 'utf8').trim();
    if (!token || /\s/.test(token)) throw new Error('AVD console token is invalid.');
    await send('auth ' + token);
  }
  if ((await send('avd name')).trim() !== 'JustLocation_API35') throw new Error('Port 5580 belongs to another AVD.');
  console.log(await send(command));
} finally {
  lines.close();
  socket.destroy();
}
