import { remoteVerify } from "./remote-server";
import type { RemotePingResult, RemoteServerConfig } from "./remote-server";
import type { RemoteMode } from "./settings";

/** 远程配置与验证凭据只保存在本机，不随浏览器账号同步。 */
export const REMOTE_SETTINGS_KEY = "remoteSettings";
export const REMOTE_VERIFY_RETRY_MS = 30_000;

export interface RemoteSettings extends RemoteServerConfig {
  remoteMode: RemoteMode;
  /** 当前地址与 token 的 SHA-256 指纹；空串表示尚未验证。 */
  remoteVerified: string;
}

type StoredRemoteSettings = Partial<Omit<RemoteSettings, "remoteVerified">> & {
  remoteVerified?: string | boolean;
};

interface LocalStorage {
  get(key: string): Promise<Record<string, unknown>>;
  set(items: Record<string, unknown>): Promise<void>;
}

type VerifyConnection = (config: RemoteServerConfig) => Promise<RemotePingResult>;

const DEFAULT_REMOTE_SETTINGS: RemoteSettings = {
  remoteMode: "off",
  remoteUrl: "",
  remoteToken: "",
  remoteVerified: "",
};

function connectionKey(config: RemoteServerConfig): string {
  return JSON.stringify([config.remoteUrl, config.remoteToken]);
}

async function fingerprint(config: RemoteServerConfig): Promise<string> {
  const bytes = new TextEncoder().encode(connectionKey(config));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

/** 保留旧配置，但只有本机成功验证过的当前连接才能取得验证指纹。 */
export class RemoteSettingsStore {
  private readonly pending = new Map<string, Promise<RemotePingResult>>();
  private readonly failures = new Map<string, number>();

  constructor(
    private readonly storage: LocalStorage,
    private readonly verifyConnection: VerifyConnection = remoteVerify,
    private readonly now: () => number = Date.now,
  ) {}

  private async read(): Promise<StoredRemoteSettings | undefined> {
    const result = await this.storage.get(REMOTE_SETTINGS_KEY);
    return result[REMOTE_SETTINGS_KEY] as StoredRemoteSettings | undefined;
  }

  async load(synced?: StoredRemoteSettings): Promise<RemoteSettings> {
    const stored = await this.read();
    if (!stored) {
      const remote: RemoteSettings = {
        remoteMode: synced?.remoteMode ?? "off",
        remoteUrl: synced?.remoteUrl ?? "",
        remoteToken: synced?.remoteToken ?? "",
        // 同步残留的布尔值或指纹都不能作为本机已验证的依据。
        remoteVerified: "",
      };
      if (synced) {
        await this.storage.set({ [REMOTE_SETTINGS_KEY]: remote });
      }
      return remote;
    }

    const remote = {
      ...DEFAULT_REMOTE_SETTINGS,
      ...stored,
      remoteVerified: "",
    };
    if (stored.remoteVerified === true) {
      // 旧 local 验证状态在本机自动补验一次；先移除布尔值，失败后由投递路径节流重试。
      await this.storage.set({ [REMOTE_SETTINGS_KEY]: remote });
      await this.verify(remote);
      return this.load();
    }
    if (
      typeof stored.remoteVerified === "string" &&
      stored.remoteVerified &&
      stored.remoteVerified === await fingerprint(remote)
    ) {
      remote.remoteVerified = stored.remoteVerified;
    }
    return remote;
  }

  async persist(next: RemoteSettings): Promise<void> {
    const stored = await this.read();
    const remote = { ...next, remoteVerified: "" };
    // 普通设置写入只能保留现有指纹，不能凭调用方给出的验证结论解锁连接。
    if (
      next.remoteVerified &&
      stored &&
      stored.remoteUrl === next.remoteUrl &&
      stored.remoteToken === next.remoteToken &&
      typeof stored.remoteVerified === "string" &&
      stored.remoteVerified === await fingerprint(next)
    ) {
      remote.remoteVerified = stored.remoteVerified;
    }
    if (JSON.stringify(stored) !== JSON.stringify(remote)) {
      await this.storage.set({ [REMOTE_SETTINGS_KEY]: remote });
    }
  }

  async verify(config: RemoteServerConfig): Promise<RemotePingResult> {
    // 固定被验证的连接，不能让请求期间的输入修改取得另一组凭据的验证结论。
    const connection = {
      remoteUrl: config.remoteUrl,
      remoteToken: config.remoteToken,
    };
    const key = connectionKey(connection);
    const existing = this.pending.get(key);
    if (existing) return existing;

    const attempt = this.verifyAndPersist(connection, key);
    this.pending.set(key, attempt);
    try {
      return await attempt;
    } finally {
      this.pending.delete(key);
    }
  }

  private async verifyAndPersist(
    config: RemoteServerConfig,
    key: string,
  ): Promise<RemotePingResult> {
    const result = await this.verifyConnection(config);
    const now = this.now();
    for (const [failedKey, at] of this.failures) {
      if (now - at >= REMOTE_VERIFY_RETRY_MS) this.failures.delete(failedKey);
    }
    if (result.success) this.failures.delete(key);
    else this.failures.set(key, now);

    const verifiedFingerprint = result.success ? await fingerprint(config) : "";
    const stored = await this.read();
    if (
      stored &&
      stored.remoteUrl === config.remoteUrl &&
      stored.remoteToken === config.remoteToken
    ) {
      await this.storage.set({
        [REMOTE_SETTINGS_KEY]: {
          ...DEFAULT_REMOTE_SETTINGS,
          ...stored,
          remoteVerified: verifiedFingerprint,
        },
      });
    }
    return result;
  }

  async ensureVerified(remote: RemoteSettings): Promise<RemoteSettings> {
    if (
      remote.remoteVerified ||
      remote.remoteMode === "off" ||
      !remote.remoteUrl ||
      !remote.remoteToken
    ) {
      return remote;
    }
    const key = connectionKey(remote);
    const failedAt = this.failures.get(key);
    if (failedAt !== undefined && this.now() - failedAt < REMOTE_VERIFY_RETRY_MS) {
      return remote;
    }
    // 同一连接共享验证请求；失败后 30 秒内不再让下载捕获等待网络超时。
    await this.verify(remote);
    return this.load();
  }
}
