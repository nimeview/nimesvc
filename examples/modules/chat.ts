export async function onJoin(ctx: any, _payload: any) {
  ctx.sendRaw('MessageOut', { text: 'welcome' });
}

export async function onMessage(ctx: any, payload: any) {
  ctx.sendRaw('MessageOut', { text: payload?.text || 'echo' });
}

export async function sendMessage(ctx: any, payload: any) {
  ctx.sendRaw('MessageOut', payload);
}
