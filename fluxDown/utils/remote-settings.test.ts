import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  REMOTE_SETTINGS_KEY,
  REMOTE_VERIFY_RETRY_MS,
  RemoteSettingsStore,
} from "./remote-settings";
import type { RemoteSettings } from "./remote-settings";
import type { RemotePingResult } from "./remote-server";

class MemoryStorage {
  constructor(public remote?: Record<string, unknown>) {}

  async get(key: string): Promise<Record<string, unknown>> {
    return { [key]: this.remote ? structuredClone(this.remote) : undefined };
  }

  async set(items: Record<string, unknown>): Promise<void> {
    this.remote = structuredClone(items[REMOTE_SETTINGS_KEY]) as Record<string, unknown>;
  }
}

const CONFIG = {
  remoteMode: "always" as const,
  remoteUrl: "https://downloads.example.test",
  remoteToken: "device-secret",
};

const VERIFIED_FINGERPRINT = createHash("sha256")
  .update(JSON.stringify([CONFIG.remoteUrl, CONFIG.remoteToken]))
  .digest("hex");

describe("remote verification", () => {
  test("sync verification never unlocks a new device", async () => {
    for (const remoteVerified of [true, VERIFIED_FINGERPRINT]) {
      const storage = new MemoryStorage();
      let requests = 0;
      const store = new RemoteSettingsStore(storage, async () => {
        requests++;
        return { success: true };
      });

      const legacy = { ...CONFIG, remoteVerified, enabled: false };
      const migrated = await store.load(legacy);

      expect(migrated).toEqual({ ...CONFIG, remoteVerified: "" });
      expect(storage.remote).toEqual({ ...CONFIG, remoteVerified: "" });
      expect(requests).toBe(0);
      expect((await store.load()).remoteVerified).toBe("");
    }
  });

  test("existing local configuration takes precedence over synced credentials", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: "" });
    const store = new RemoteSettingsStore(storage);

    expect(await store.load({
      remoteMode: "fallback",
      remoteUrl: "https://different.example.test",
      remoteToken: "synced-secret",
      remoteVerified: true,
    })).toEqual({ ...CONFIG, remoteVerified: "" });
  });

  test("only a successful local verification can create a valid fingerprint", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: "" });
    const store = new RemoteSettingsStore(storage, async () => ({ success: true }));

    await store.persist({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    expect((await store.load()).remoteVerified).toBe("");
    await store.verify(CONFIG);
    expect((await store.load()).remoteVerified).toBe(VERIFIED_FINGERPRINT);
    expect(storage.remote?.remoteVerified).toBe(VERIFIED_FINGERPRINT);
    await store.persist({ ...CONFIG, remoteMode: "fallback", remoteVerified: VERIFIED_FINGERPRINT });
    expect(await store.load()).toEqual({
      ...CONFIG,
      remoteMode: "fallback",
      remoteVerified: VERIFIED_FINGERPRINT,
    });
  });

  for (const field of ["remoteUrl", "remoteToken"] as const) {
    test(`${field} changes invalidate the stored fingerprint on read`, async () => {
      const storage = new MemoryStorage({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
      const store = new RemoteSettingsStore(storage);

      storage.remote = {
        ...CONFIG,
        [field]: `${CONFIG[field]}-changed`,
        remoteVerified: VERIFIED_FINGERPRINT,
      };

      expect((await store.load()).remoteVerified).toBe("");
    });
  }

  test("saving changed credentials cannot retain or restore an old verification", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    const store = new RemoteSettingsStore(storage);

    await store.persist({
      ...CONFIG,
      remoteToken: "replacement-secret",
      remoteVerified: VERIFIED_FINGERPRINT,
    });
    expect(storage.remote?.remoteVerified).toBe("");
    await store.persist({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    expect((await store.load()).remoteVerified).toBe("");
  });

  test("failed verification clears the current connection's verification", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    const store = new RemoteSettingsStore(storage, async () => ({
      success: false,
      message: "remote_auth_failed",
    }));

    await store.verify(CONFIG);

    expect((await store.load()).remoteVerified).toBe("");
    expect(storage.remote?.remoteVerified).toBe("");
  });

  test("legacy local verification is upgraded once without changing the selected mode", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: true });
    let requests = 0;
    const store = new RemoteSettingsStore(storage, async () => {
      requests++;
      return { success: true };
    });

    expect(await store.load()).toEqual({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    await store.load();
    expect(requests).toBe(1);
    expect(storage.remote?.remoteVerified).toBe(VERIFIED_FINGERPRINT);
  });

  test("offline upgrade retries after the cooldown and restores remote routing eligibility", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: true });
    let now = 100;
    let requests = 0;
    const store = new RemoteSettingsStore(storage, async () => {
      requests++;
      return requests === 1
        ? { success: false, message: "remote_unreachable" }
        : { success: true };
    }, () => now);

    const offline = await store.load();
    expect(offline).toEqual({ ...CONFIG, remoteVerified: "" });
    now += REMOTE_VERIFY_RETRY_MS - 1;
    expect(await store.ensureVerified(offline)).toEqual(offline);
    expect(requests).toBe(1);
    now++;
    expect(await store.ensureVerified(offline)).toEqual({
      ...CONFIG,
      remoteVerified: VERIFIED_FINGERPRINT,
    });
    expect(requests).toBe(2);
  });

  test("migrated settings self-heal through a local connection verification", async () => {
    const storage = new MemoryStorage();
    let requests = 0;
    const store = new RemoteSettingsStore(storage, async () => {
      requests++;
      return { success: true };
    });
    const migrated = await store.load({ ...CONFIG, remoteVerified: true });

    const verified = await store.ensureVerified(migrated);

    expect(verified).toEqual({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    await store.ensureVerified(verified);
    expect(requests).toBe(1);
  });

  test("concurrent captures share one verification request", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: "" });
    let finish!: (result: RemotePingResult) => void;
    const response = new Promise<RemotePingResult>((resolve) => { finish = resolve; });
    let started!: () => void;
    const requestStarted = new Promise<void>((resolve) => { started = resolve; });
    let requests = 0;
    const store = new RemoteSettingsStore(storage, async () => {
      requests++;
      started();
      return response;
    });
    const remote = await store.load();
    const first = store.ensureVerified(remote);
    const second = store.ensureVerified(remote);
    await requestStarted;
    finish({ success: true });

    expect(await first).toEqual({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    expect(await second).toEqual({ ...CONFIG, remoteVerified: VERIFIED_FINGERPRINT });
    expect(requests).toBe(1);
  });

  test("late verification success cannot verify credentials changed during the request", async () => {
    const storage = new MemoryStorage({ ...CONFIG, remoteVerified: "" });
    let finish!: (result: RemotePingResult) => void;
    const response = new Promise<RemotePingResult>((resolve) => { finish = resolve; });
    let started!: () => void;
    const requestStarted = new Promise<void>((resolve) => { started = resolve; });
    const store = new RemoteSettingsStore(storage, async () => {
      started();
      return response;
    });
    const verification = store.verify(CONFIG);
    await requestStarted;
    storage.remote = { ...CONFIG, remoteToken: "replacement-secret", remoteVerified: "" };
    finish({ success: true });
    await verification;

    expect(await store.load()).toEqual({
      ...CONFIG,
      remoteToken: "replacement-secret",
      remoteVerified: "",
    });
  });

  test("desktop-only mode and incomplete credentials do not trigger automatic verification", async () => {
    let requests = 0;
    for (const overrides of [
      { remoteMode: "off" as const },
      { remoteUrl: "" },
      { remoteToken: "" },
    ]) {
      const remote: RemoteSettings = { ...CONFIG, ...overrides, remoteVerified: "" };
      const store = new RemoteSettingsStore(new MemoryStorage({ ...remote }), async () => {
        requests++;
        return { success: true };
      });
      expect(await store.ensureVerified(remote)).toEqual(remote);
    }
    expect(requests).toBe(0);
  });
});
