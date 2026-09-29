// system.* 方法。握手（hello）与快照（snapshot）由连接层（client.ts）独占，不在此暴露。

import { call } from '../client';
import { METHOD } from '../protocol';
import type { OkResult } from '../protocol';

export const system = {
  ping: () => call<OkResult>(METHOD.SYSTEM_PING),
  /** 让服务优雅退出：daemon 关停引擎；agent 先关停 daemon 再退出。 */
  shutdown: () => call<OkResult>(METHOD.SYSTEM_SHUTDOWN),
};
