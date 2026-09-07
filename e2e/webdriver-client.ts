// Shared low-level plumbing for driving the real Tauri app through
// tauri-driver + msedgedriver (Windows only). Used by the e2e smoke test
// (run-smoke.ts) and the dev-time inspection tool (scripts/inspect-app.ts).
import { spawn, type ChildProcess } from 'node:child_process';
import { createConnection } from 'node:net';

const TAURI_DRIVER_PORT = 4444;
const DRIVER_BASE = `http://127.0.0.1:${TAURI_DRIVER_PORT}`;

export interface DriverSession {
  driver: ChildProcess;
  sessionId: string;
  driverOutput: { value: string };
}

function waitForPort(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = createConnection({ port, host: '127.0.0.1' });
      socket.once('connect', () => {
        socket.destroy();
        resolve();
      });
      socket.once('error', () => {
        socket.destroy();
        if (Date.now() > deadline) {
          reject(new Error(`tauri-driver did not start listening on port ${port} within ${timeoutMs}ms`));
        } else {
          setTimeout(attempt, 200);
        }
      });
    };
    attempt();
  });
}

export async function webdriverRequest(method: string, urlPath: string, body?: unknown): Promise<any> {
  const res = await fetch(`${DRIVER_BASE}${urlPath}`, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json();
  if (!res.ok) {
    throw new Error(`WebDriver ${method} ${urlPath} failed (HTTP ${res.status}): ${JSON.stringify(json)}`);
  }
  return json.value;
}

/**
 * Starts tauri-driver and opens a WebDriver session against `appExe`.
 * `dataDir`, when given, is passed as LUMINOUS_DATA_DIR — omit it only to
 * deliberately point the app at a real user's actual library (see
 * src-tauri/src/paths.rs); otherwise always pass an isolated directory.
 */
export async function startSession(appExe: string, dataDir?: string): Promise<DriverSession> {
  const driverOutput = { value: '' };
  // `detached: true` is required on Windows so the driver (and the app
  // process it launches) survive past this Node process exiting — without
  // it, spawned children are tied to this process's job object and get
  // killed the moment it exits, which breaks the persisted-session model
  // scripts/inspect-app.ts relies on (start/screenshot/click/stop as
  // separate invocations).
  const driver: ChildProcess = spawn('tauri-driver', ['--port', String(TAURI_DRIVER_PORT)], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: dataDir ? { ...process.env, LUMINOUS_DATA_DIR: dataDir } : process.env,
    detached: true,
  });
  driver.unref();
  driver.stdout?.on('data', (d) => (driverOutput.value += d.toString()));
  driver.stderr?.on('data', (d) => (driverOutput.value += d.toString()));

  const driverExitedEarly = new Promise<never>((_, reject) => {
    driver.once('error', (err: NodeJS.ErrnoException) => {
      if (err.code === 'ENOENT') {
        reject(new Error('tauri-driver not found on PATH. Install it with: cargo install tauri-driver --locked'));
      } else {
        reject(err);
      }
    });
    driver.once('exit', (code) => {
      if (code !== 0) {
        reject(new Error(`tauri-driver exited early (code ${code}):\n${driverOutput.value}`));
      }
    });
  });

  try {
    await Promise.race([waitForPort(TAURI_DRIVER_PORT, 10_000), driverExitedEarly]);

    const session = await webdriverRequest('POST', '/session', {
      capabilities: {
        alwaysMatch: {
          browserName: 'wry',
          'tauri:options': { application: appExe },
        },
      },
    });

    return { driver, sessionId: session.sessionId, driverOutput };
  } catch (err) {
    driver.kill();
    const message = err instanceof Error ? err.message : String(err);
    throw new Error(`${message}${driverOutput.value ? `\n\ntauri-driver output:\n${driverOutput.value}` : ''}`);
  }
}

export async function stopSession(session: DriverSession): Promise<void> {
  await webdriverRequest('DELETE', `/session/${session.sessionId}`).catch(() => {});
  session.driver.kill();
}

export function screenshot(session: DriverSession): Promise<string> {
  return webdriverRequest('GET', `/session/${session.sessionId}/screenshot`);
}

export function getSource(session: DriverSession): Promise<string> {
  return webdriverRequest('GET', `/session/${session.sessionId}/source`);
}

export function getUrl(session: DriverSession): Promise<string> {
  return webdriverRequest('GET', `/session/${session.sessionId}/url`);
}

export function getTitle(session: DriverSession): Promise<string> {
  return webdriverRequest('GET', `/session/${session.sessionId}/title`);
}

/** `using`: 'css selector' | 'xpath' | 'link text' | 'partial link text' */
async function findElement(session: DriverSession, using: string, value: string): Promise<string> {
  const el = await webdriverRequest('POST', `/session/${session.sessionId}/element`, { using, value });
  return el['element-6066-11e4-a52e-4f735466cecf'];
}

export async function clickElement(session: DriverSession, using: string, value: string): Promise<void> {
  const elementId = await findElement(session, using, value);
  await webdriverRequest('POST', `/session/${session.sessionId}/element/${elementId}/click`, {});
}

export async function typeIntoElement(session: DriverSession, using: string, value: string, text: string): Promise<void> {
  const elementId = await findElement(session, using, value);
  await webdriverRequest('POST', `/session/${session.sessionId}/element/${elementId}/value`, { text });
}
