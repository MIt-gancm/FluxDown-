// provider 入口固定为 globalThis.subscribe。
// 订阅记录的 providerConfig 会原样出现在 ctx.providerConfig。
globalThis.subscribe = async (ctx) => {
  const response = await flux.fetch({
    method: 'GET',
    url: ctx.url,
    headers: ctx.cookies ? { Cookie: ctx.cookies } : {},
  });
  if (response.status < 200 || response.status >= 300) {
    throw new Error(`subscription HTTP ${response.status}`);
  }

  // 插件只负责平台请求/解析，返回值交给核心的统一订阅状态机。
  const payload = JSON.parse(response.body);
  return {
    title: payload.title || '',
    link: payload.link || ctx.url,
    items: (payload.items || []).map((item) => ({
      guid: String(item.guid),
      title: item.title || '',
      link: item.link || '',
      enclosureUrl: item.enclosureUrl || '',
      enclosureLength: Number(item.enclosureLength || 0),
      pubDate: Number(item.pubDate || 0),
    })),
  };
};
