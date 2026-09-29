// 类型化 RPC 方法包装器：`rpc.<service>.<group>.<method>(params)`，与 wire 方法名一一对应。

import { agent } from './agent';
import { daemon } from './daemon';
import { system } from './system';

export const rpc = { system, daemon, agent };
